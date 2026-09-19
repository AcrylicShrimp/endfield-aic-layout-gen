// Every exported function catches C++ exceptions. Tensor results are owned heap
// torch::Tensor wrappers, transferred to tch; parameter storage is shared by value.
#include <ATen/CPUGeneratorImpl.h>
#include <ATen/Context.h>
#ifdef AIC_LIBTORCH_CUDA
#include <ATen/cuda/detail/CUDAHooks.h>
#endif
#include <torch/types.h>
#include "optimizer.h"
#include <memory>
#include <string>
#include <vector>

namespace {
thread_local std::string last_error;
#ifdef AIC_LIBTORCH_CUDA
// Keep CUDA registration linked under --as-needed, without initializing a GPU
// on CPU runs. The volatile read prevents elimination of the symbol reference.
auto (*volatile cuda_link_anchor)(void (*)()) = &at::cuda::detail::set_magma_init_fn;
#endif
struct Rng { at::Generator value; };
using Adam=aic::Adam;
#define AIC_TRY try { last_error.clear();
#define AIC_CATCH } catch (const std::exception& e) { last_error=e.what(); return false; } \
  catch (...) { last_error="Unknown LibTorch exception"; return false; }
}

extern "C" {
const char* aic_torch_error() noexcept { return last_error.c_str(); }
bool aic_torch_deterministic() noexcept {
  AIC_TRY
#ifdef AIC_LIBTORCH_CUDA
  (void)cuda_link_anchor;
#endif
  at::globalContext().setDeterministicAlgorithms(true, false); return true; AIC_CATCH
}
bool aic_rng_new(uint64_t seed, void** out) noexcept {
  AIC_TRY *out = new Rng{at::make_generator<at::CPUGeneratorImpl>(seed)}; return true; AIC_CATCH
}
void aic_rng_free(void* h) noexcept { delete static_cast<Rng*>(h); }
bool aic_rng_state(void* h, void** out) noexcept {
  AIC_TRY *out=new torch::Tensor(static_cast<Rng*>(h)->value.get_state()); return true; AIC_CATCH
}
bool aic_rng_restore(void* h, const void* state) noexcept {
  AIC_TRY static_cast<Rng*>(h)->value.set_state(*static_cast<const torch::Tensor*>(state)); return true; AIC_CATCH
}
bool aic_rng_draw(void* h, int mode, int64_t n, int64_t high, const void* input, void** out) noexcept {
  AIC_TRY
  auto& g=static_cast<Rng*>(h)->value;
  torch::Tensor result;
  if (mode==0) result=at::randperm(n,g,torch::TensorOptions().dtype(torch::kInt64).device(torch::kCPU));
  else if (mode==1) result=at::randint(high,{n},g,torch::TensorOptions().dtype(torch::kInt64).device(torch::kCPU));
  else if (mode==2) result=at::multinomial(*static_cast<const torch::Tensor*>(input),n,false,g);
  else if (mode==3) result=at::rand({n},g,torch::TensorOptions().dtype(torch::kFloat32).device(torch::kCPU));
  else TORCH_CHECK(false,"Unknown RNG operation");
  *out=new torch::Tensor(std::move(result)); return true;
  AIC_CATCH
}
bool aic_adam_new(const void* const* tensors, size_t n, double lr, double b1, double b2, double eps, int mode, void** out) noexcept {
  AIC_TRY
  std::vector<torch::Tensor> p; p.reserve(n);
  for(size_t i=0;i<n;++i) p.push_back(*static_cast<const torch::Tensor*>(tensors[i]));
  *out=new Adam(std::move(p),lr,b1,b2,eps,mode); return true;
  AIC_CATCH
}
void aic_adam_free(void* h) noexcept { delete static_cast<Adam*>(h); }
bool aic_adam_step(void* h) noexcept {
  AIC_TRY static_cast<Adam*>(h)->step(); return true; AIC_CATCH
}
bool aic_adam_zero(void* h) noexcept {
  AIC_TRY static_cast<Adam*>(h)->zero_grad(); return true; AIC_CATCH
}
bool aic_adam_clip(void* h, double limit, double* norm) noexcept {
  AIC_TRY *norm=static_cast<Adam*>(h)->clip_grad_norm(limit); return true; AIC_CATCH
}
bool aic_adam_state(void* h, size_t index, int64_t* step, void** mean, void** square) noexcept {
  AIC_TRY
  const auto& s=static_cast<Adam*>(h)->state(index);
  if(!s.mean.defined()) { *step=0; *mean=nullptr; *square=nullptr; return true; }
  // Allocate under RAII so failure of the second allocation cannot leak the first.
  auto m=std::make_unique<torch::Tensor>(s.mean.detach().clone());
  auto v=std::make_unique<torch::Tensor>(s.square.detach().clone());
  *step=s.step.item<int64_t>(); *mean=m.release(); *square=v.release(); return true;
  AIC_CATCH
}
bool aic_adam_restore(void* h, size_t index, int64_t step, const void* mean, const void* square) noexcept {
  AIC_TRY
  const auto& m=*static_cast<const torch::Tensor*>(mean);
  const auto& v=*static_cast<const torch::Tensor*>(square);
  static_cast<Adam*>(h)->restore(index,step,m,v); return true;
  AIC_CATCH
}
}
