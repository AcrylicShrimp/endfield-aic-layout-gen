//! Cooperative interruption for a scoped native computation and its joined workers.
use std::{
    cell::RefCell,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

pub struct WorkControl {
    deadline: Instant,
    guard: Arc<dyn Fn() -> Result<(), String> + Send + Sync>,
    failure: OnceLock<String>,
}
thread_local! {
    static ACTIVE: RefCell<Option<Arc<WorkControl>>> = const { RefCell::new(None) };
}
impl WorkControl {
    pub fn new(
        duration: Duration,
        guard: Arc<dyn Fn() -> Result<(), String> + Send + Sync>,
    ) -> Result<Arc<Self>, String> {
        let deadline = Instant::now()
            .checked_add(duration)
            .ok_or("native-work.invalid-deadline")?;
        Ok(Arc::new(Self {
            deadline,
            guard,
            failure: OnceLock::new(),
        }))
    }
    pub fn check(&self) -> Result<(), String> {
        if let Some(error) = self.failure.get() {
            return Err(error.clone());
        }
        let result = (self.guard)().and_then(|()| {
            if Instant::now() >= self.deadline {
                Err(
                    "native-work.elapsed-budget: computation interrupted; not global infeasibility"
                        .into(),
                )
            } else {
                Ok(())
            }
        });
        if let Err(error) = &result {
            let _ = self.failure.set(error.clone());
        }
        result
    }
    /// Install only for this call; restore the caller's scope on return or panic.
    /// Internal rejected candidates cannot turn a cancellation into a successful result.
    pub fn run<T>(self: &Arc<Self>, action: impl FnOnce() -> T) -> Result<T, String> {
        self.check()?;
        struct Restore(Option<Arc<WorkControl>>);
        impl Drop for Restore {
            fn drop(&mut self) {
                ACTIVE.with(|slot| *slot.borrow_mut() = self.0.take());
            }
        }
        let _restore = Restore(ACTIVE.with(|slot| slot.replace(Some(self.clone()))));
        let result = action();
        self.check()?;
        Ok(result)
    }
}
pub(crate) fn current() -> Option<Arc<WorkControl>> {
    ACTIVE.with(|slot| slot.borrow().clone())
}
pub(crate) fn stopped() -> bool {
    ACTIVE.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|control| control.check().is_err())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    #[test]
    fn cancellation_is_sticky_and_scopes_restore_on_return_and_panic() {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let control = WorkControl::new(
            Duration::from_secs(10),
            Arc::new(move || {
                if flag.load(Ordering::Relaxed) {
                    Err("test-cancelled".into())
                } else {
                    Ok(())
                }
            }),
        )
        .unwrap();
        assert!(current().is_none());
        let result = control.run(|| {
            cancel.store(true, Ordering::Relaxed);
            assert!(stopped());
            cancel.store(false, Ordering::Relaxed);
            42
        });
        assert_eq!(result.unwrap_err(), "test-cancelled");
        assert!(current().is_none());
        let fresh = WorkControl::new(Duration::from_secs(10), Arc::new(|| Ok(()))).unwrap();
        fresh
            .run(|| {
                assert!(current().is_some());
                assert!(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        fresh.run(|| panic!("controlled panic"))
                    }))
                    .is_err()
                );
                assert!(current().is_some());
                std::thread::scope(|scope| {
                    scope.spawn(|| assert!(current().is_none())).join().unwrap();
                });
            })
            .unwrap();
        assert!(current().is_none());
        assert!(
            WorkControl::new(Duration::ZERO, Arc::new(|| Ok(())))
                .unwrap()
                .run(|| 42)
                .is_err()
        );
    }
}
