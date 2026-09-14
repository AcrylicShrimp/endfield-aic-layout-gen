//! The only unsafe Rust boundary. Handles are thread-affine; all calls are
//! synchronous and C++ catches exceptions. Returned tensor wrappers transfer
//! ownership to tch, whose destructor uses the same LibTorch allocation ABI.
use anyhow::{Result, bail, ensure};
use std::{
    ffi::{CStr, c_char, c_void},
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
};
use tch::Tensor;

unsafe extern "C" {
    fn aic_torch_error() -> *const c_char;
    fn aic_torch_deterministic() -> bool;
    fn aic_rng_new(seed: u64, out: *mut *mut c_void) -> bool;
    fn aic_rng_free(handle: *mut c_void);
    fn aic_rng_state(handle: *mut c_void, out: *mut *mut c_void) -> bool;
    fn aic_rng_restore(handle: *mut c_void, state: *const c_void) -> bool;
    fn aic_rng_draw(
        handle: *mut c_void,
        mode: i32,
        n: i64,
        high: i64,
        input: *const c_void,
        out: *mut *mut c_void,
    ) -> bool;
    fn aic_adam_new(
        tensors: *const *const c_void,
        n: usize,
        lr: f64,
        b1: f64,
        b2: f64,
        eps: f64,
        mode: i32,
        out: *mut *mut c_void,
    ) -> bool;
    fn aic_adam_free(handle: *mut c_void);
    fn aic_adam_step(handle: *mut c_void) -> bool;
    fn aic_adam_zero(handle: *mut c_void) -> bool;
    fn aic_adam_clip(handle: *mut c_void, limit: f64, norm: *mut f64) -> bool;
    fn aic_adam_state(
        handle: *mut c_void,
        index: usize,
        step: *mut i64,
        mean: *mut *mut c_void,
        square: *mut *mut c_void,
    ) -> bool;
    fn aic_adam_restore(
        handle: *mut c_void,
        index: usize,
        step: i64,
        mean: *const c_void,
        square: *const c_void,
    ) -> bool;
}

fn checked(ok: bool) -> Result<()> {
    if !ok {
        // SAFETY: valid until the next bridge call on this thread; copied now.
        let message = unsafe { CStr::from_ptr(aic_torch_error()) }.to_string_lossy();
        bail!("aic-torch.backend: {message}");
    }
    Ok(())
}

fn tensor_output(call: impl FnOnce(*mut *mut c_void) -> bool) -> Result<Tensor> {
    let mut pointer = std::ptr::null_mut();
    checked(call(&mut pointer))?;
    ensure!(
        !pointer.is_null(),
        "aic-torch.backend: missing owned tensor"
    );
    // SAFETY: the successful ABI call allocated a new torch::Tensor wrapper.
    Ok(unsafe { Tensor::from_ptr(pointer.cast()) })
}

pub(crate) fn deterministic() -> Result<()> {
    // SAFETY: no pointer arguments; mutates LibTorch's global execution policy.
    checked(unsafe { aic_torch_deterministic() })
}

pub(crate) struct RngHandle {
    pointer: NonNull<c_void>,
    _thread: PhantomData<Rc<()>>,
}
impl RngHandle {
    pub(crate) fn new(seed: u64) -> Result<Self> {
        let mut pointer = std::ptr::null_mut();
        // SAFETY: out is writable and C++ returns an exclusively owned handle.
        checked(unsafe { aic_rng_new(seed, &mut pointer) })?;
        Ok(Self {
            pointer: NonNull::new(pointer)
                .ok_or_else(|| anyhow::anyhow!("aic-torch.rng: null handle"))?,
            _thread: PhantomData,
        })
    }
    pub(crate) fn state(&mut self) -> Result<Tensor> {
        tensor_output(|out| unsafe { aic_rng_state(self.pointer.as_ptr(), out) })
    }
    pub(crate) fn restore(&mut self, state: &Tensor) -> Result<()> {
        // SAFETY: the handle and borrowed tensor remain live for the call.
        checked(unsafe { aic_rng_restore(self.pointer.as_ptr(), state.as_ptr().cast()) })
    }
    pub(crate) fn draw(
        &mut self,
        mode: i32,
        n: i64,
        high: i64,
        input: Option<&Tensor>,
    ) -> Result<Tensor> {
        let input = input.map_or(std::ptr::null(), |t| t.as_ptr().cast());
        tensor_output(|out| unsafe {
            aic_rng_draw(self.pointer.as_ptr(), mode, n, high, input, out)
        })
    }
}
impl Drop for RngHandle {
    fn drop(&mut self) {
        unsafe { aic_rng_free(self.pointer.as_ptr()) }
    }
}

pub(crate) struct AdamHandle {
    pointer: NonNull<c_void>,
    _thread: PhantomData<Rc<()>>,
}
impl AdamHandle {
    pub(crate) fn new(
        parameters: &[Tensor],
        lr: f64,
        beta1: f64,
        beta2: f64,
        eps: f64,
        mode: i32,
    ) -> Result<Self> {
        let tensors = parameters
            .iter()
            .map(|t| t.as_ptr().cast())
            .collect::<Vec<_>>();
        let mut pointer = std::ptr::null_mut();
        // SAFETY: C++ copies tensor wrappers, retaining their storage after return.
        checked(unsafe {
            aic_adam_new(
                tensors.as_ptr(),
                tensors.len(),
                lr,
                beta1,
                beta2,
                eps,
                mode,
                &mut pointer,
            )
        })?;
        Ok(Self {
            pointer: NonNull::new(pointer)
                .ok_or_else(|| anyhow::anyhow!("aic-torch.adam: null handle"))?,
            _thread: PhantomData,
        })
    }
    pub(crate) fn step(&mut self) -> Result<()> {
        checked(unsafe { aic_adam_step(self.pointer.as_ptr()) })
    }
    pub(crate) fn zero_grad(&mut self) -> Result<()> {
        checked(unsafe { aic_adam_zero(self.pointer.as_ptr()) })
    }
    pub(crate) fn clip_grad_norm(&self, limit: f64) -> Result<f64> {
        let mut norm = 0.;
        checked(unsafe { aic_adam_clip(self.pointer.as_ptr(), limit, &mut norm) })?;
        Ok(norm)
    }
    pub(crate) fn state(&mut self, index: usize) -> Result<Option<(i64, Tensor, Tensor)>> {
        let (mut step, mut mean, mut square) = (0, std::ptr::null_mut(), std::ptr::null_mut());
        checked(unsafe {
            aic_adam_state(
                self.pointer.as_ptr(),
                index,
                &mut step,
                &mut mean,
                &mut square,
            )
        })?;
        if step == 0 {
            return Ok(None);
        }
        // SAFETY: a successful populated state returns two owned wrappers.
        Ok(Some(unsafe {
            (
                step,
                Tensor::from_ptr(mean.cast()),
                Tensor::from_ptr(square.cast()),
            )
        }))
    }
    pub(crate) fn restore(
        &mut self,
        index: usize,
        step: i64,
        mean: &Tensor,
        square: &Tensor,
    ) -> Result<()> {
        checked(unsafe {
            aic_adam_restore(
                self.pointer.as_ptr(),
                index,
                step,
                mean.as_ptr().cast(),
                square.as_ptr().cast(),
            )
        })
    }
}
impl Drop for AdamHandle {
    fn drop(&mut self) {
        unsafe { aic_adam_free(self.pointer.as_ptr()) }
    }
}
