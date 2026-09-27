//! A visit-driven cursor over the independent validator's single rule path.

use std::cell::RefCell;
use std::future::{Future, poll_fn};
use std::ops::Deref;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use super::{Catalogs, Result, error, validate_core};
use crate::geometry::Diagnostic;
use crate::physical::{Layout, Metrics};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Request {
    Visit,
    Reserve(usize),
    Release(usize),
}

#[derive(Default)]
struct Control {
    request: Option<Request>,
    response: Option<Result<()>>,
    reserved_bytes: usize,
}

#[derive(Clone)]
pub(super) struct Permit(Rc<RefCell<Control>>);

impl Permit {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(Control::default())))
    }

    async fn request(&self, request: Request) -> Result<()> {
        poll_fn(|_| {
            let mut control = self.0.borrow_mut();
            if let Some(response) = control.response.take() {
                Poll::Ready(response)
            } else {
                control.request = Some(request);
                Poll::Pending
            }
        })
        .await
    }

    pub(super) async fn visit(&self) -> Result<()> {
        self.request(Request::Visit).await
    }

    pub(super) async fn reserve(&self, bytes: usize) -> Result<()> {
        if bytes == 0 {
            return Ok(());
        }
        self.request(Request::Reserve(bytes)).await
    }

    pub(super) async fn release(&self, bytes: usize) {
        if bytes > 0 {
            self.request(Request::Release(bytes))
                .await
                .expect("releasing held validation memory cannot fail");
        }
    }
}

pub enum ValidationVisit {
    Granted,
    Retry,
    Denied(Diagnostic),
}

pub enum ValidationReservation {
    Granted,
    Retry,
    Denied,
}

pub enum ValidationAdvance {
    Pending,
    Complete(Result<Metrics>),
}

/// The future owns its local maps, nested cursors and exact position at every
/// await. Owned Arc inputs work for retained native replay; borrowed inputs
/// keep ordinary external validation free of a full-layout copy.
pub struct ValidationCursor<'a> {
    future: Option<Pin<Box<dyn Future<Output = Result<Metrics>> + 'a>>>,
    permit: Permit,
    finished: bool,
}

impl<'a> ValidationCursor<'a> {
    pub fn new<L, C>(layout: L, catalogs: C) -> Self
    where
        L: Deref<Target = Layout> + 'a,
        C: Deref<Target = Catalogs> + 'a,
    {
        let permit = Permit::new();
        let future = Box::pin(validate_core(layout, catalogs, permit.clone()));
        Self {
            future: Some(future),
            permit,
            finished: false,
        }
    }

    pub fn reserved_bytes(&self) -> usize {
        self.permit.0.borrow().reserved_bytes
    }

    /// The owner must call this before abandoning a pending cursor whose
    /// reservation callback charged an external request allowance.
    pub fn cancel(mut self, mut release: impl FnMut(usize)) {
        self.release_owned(&mut release);
    }

    fn release_owned(&mut self, release: &mut impl FnMut(usize)) {
        self.future = None;
        self.finished = true;
        let bytes = std::mem::take(&mut self.permit.0.borrow_mut().reserved_bytes);
        if bytes > 0 {
            release(bytes);
        }
    }

    pub fn advance(
        &mut self,
        mut visit: impl FnMut() -> ValidationVisit,
        mut reserve: impl FnMut(usize) -> ValidationReservation,
        mut release: impl FnMut(usize),
    ) -> ValidationAdvance {
        debug_assert!(!self.finished, "completed validation cursor cannot advance");
        let mut context = Context::from_waker(Waker::noop());
        loop {
            let pending_request = self.permit.0.borrow_mut().request.take();
            if let Some(request) = pending_request {
                let response = match request {
                    Request::Visit => match visit() {
                        ValidationVisit::Granted => Ok(()),
                        ValidationVisit::Retry => {
                            self.permit.0.borrow_mut().request = Some(request);
                            return ValidationAdvance::Pending;
                        }
                        ValidationVisit::Denied(failure) => Err(failure),
                    },
                    Request::Reserve(bytes) => match reserve(bytes) {
                        ValidationReservation::Granted => {
                            let mut control = self.permit.0.borrow_mut();
                            control.reserved_bytes = control.reserved_bytes.saturating_add(bytes);
                            Ok(())
                        }
                        ValidationReservation::Retry => {
                            self.permit.0.borrow_mut().request = Some(request);
                            return ValidationAdvance::Pending;
                        }
                        ValidationReservation::Denied => Err(error(
                            "request-memory-limit",
                            "validation",
                            "Independent validation cannot fit the request memory allowance",
                        )),
                    },
                    Request::Release(bytes) => {
                        {
                            let mut control = self.permit.0.borrow_mut();
                            control.reserved_bytes -= bytes;
                        }
                        release(bytes);
                        Ok(())
                    }
                };
                self.permit.0.borrow_mut().response = Some(response);
                continue;
            }
            match self
                .future
                .as_mut()
                .expect("active validator held")
                .as_mut()
                .poll(&mut context)
            {
                Poll::Ready(result) => {
                    self.release_owned(&mut release);
                    return ValidationAdvance::Complete(result);
                }
                Poll::Pending => {
                    debug_assert!(
                        self.permit.0.borrow().request.is_some(),
                        "validation can only yield at an admission request"
                    );
                }
            }
        }
    }
}
