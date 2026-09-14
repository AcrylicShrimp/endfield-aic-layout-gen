// Operation order follows PyTorch 2.13 torch/optim/adam.py's noncapturable
// foreach/fused paths. Numerical kernels remain in ATen; unsupported modes fail.
#include "optimizer.h"
#include <ATen/ops/_fused_adam.h>
#include <ATen/core/grad_mode.h>
#include <cmath>
#include <limits>

namespace aic {
Adam::Adam(std::vector<at::Tensor> parameters, double lr, double b1, double b2, double eps, int mode)
    : parameters_(std::move(parameters)), states_(parameters_.size()), lr_(lr), beta1_(b1), beta2_(b2), eps_(eps), mode_(mode) {
  TORCH_CHECK(!parameters_.empty() && (mode==0 || mode==1),"Invalid Adam parameters or mode");
  for(const auto& p:parameters_) {
    TORCH_CHECK(p.scalar_type()==at::kFloat && p.device()==parameters_[0].device() && !p.is_sparse(),"Adam requires dense FP32 parameters on one device");
  }
  TORCH_CHECK(mode==0 || parameters_[0].is_cuda(),"Fused Adam requires CUDA");
}

void Adam::zero_grad() {
  for(auto& p:parameters_) p.mutable_grad()=at::Tensor();
}

double Adam::clip_grad_norm(double limit) {
  at::NoGradGuard guard;
  std::vector<at::Tensor> grads;
  for(const auto& p:parameters_) if(p.grad().defined()) grads.push_back(p.grad());
  if(grads.empty()) return 0.;
  const auto norms=at::_foreach_norm(grads,2.);
  const auto norm=at::stack(norms).norm();
  const double value=norm.item<double>();
  TORCH_CHECK(std::isfinite(value),"Nonfinite gradient norm");
  at::_foreach_mul_(grads,(limit/(norm+1e-6)).clamp_max(1.));
  return value;
}

void Adam::step() {
  at::NoGradGuard guard;
  std::vector<at::Tensor> params, grads, means, squares, steps;
  for(size_t i=0;i<parameters_.size();++i) {
    const auto& p=parameters_[i];auto g=p.grad();
    if(!g.defined()) continue;
    TORCH_CHECK(!g.is_sparse() && g.scalar_type()==at::kFloat && g.device()==p.device(),"Unsupported Adam gradient");
    auto& s=states_[i];
    if(!s.mean.defined()) {
      s.mean=at::zeros_like(p);s.square=at::zeros_like(p);
      s.step=at::zeros({},p.options().device(mode_==1 ? p.device() : at::Device(at::kCPU)));
    }
    params.push_back(p);grads.push_back(g);means.push_back(s.mean);squares.push_back(s.square);steps.push_back(s.step);
  }
  if(params.empty()) return;
  if(mode_==1) {
    at::_foreach_add_(steps,1);
    at::_fused_adam_(params,grads,means,squares,{},steps,lr_,beta1_,beta2_,0.,eps_,false,false);
    return;
  }
  at::_foreach_add_(steps,at::scalar_tensor(1.,at::TensorOptions().dtype(at::kFloat).device(at::kCPU)),1.);
  at::_foreach_lerp_(means,grads,1.-beta1_);
  at::_foreach_mul_(squares,beta2_);
  at::_foreach_addcmul_(squares,grads,grads,1.-beta2_);
  std::vector<at::Scalar> sizes,corrections;
  for(const auto& step:steps) {
    const double t=step.item<double>();
    sizes.emplace_back(-lr_/(1.-std::pow(beta1_,t)));
    corrections.emplace_back(std::sqrt(1.-std::pow(beta2_,t)));
  }
  auto denominator=at::_foreach_sqrt(squares);
  at::_foreach_div_(denominator,corrections);
  at::_foreach_add_(denominator,eps_);
  at::_foreach_addcdiv_(params,means,denominator,sizes);
}

void Adam::restore(size_t index, int64_t step, const at::Tensor& mean, const at::Tensor& square) {
  const auto& p=parameters_.at(index);
  const auto counter = static_cast<float>(step);
  // Check the upper bound before casting back: INT64_MAX rounds up to 2^63
  // in FP32 and converting that value to int64 would be undefined behavior.
  TORCH_CHECK(step>0 && counter<static_cast<float>(std::numeric_limits<int64_t>::max()) &&
    static_cast<int64_t>(counter)==step,"Adam step cannot be represented by its FP32 counter");
  TORCH_CHECK(mean.sizes()==p.sizes() && square.sizes()==p.sizes(),"Adam accumulator shape mismatch");
  TORCH_CHECK(mean.scalar_type()==p.scalar_type() && square.scalar_type()==p.scalar_type(),"Adam accumulator dtype mismatch");
  TORCH_CHECK(mean.device()==p.device() && square.device()==p.device(),"Adam accumulator device mismatch");
  TORCH_CHECK(at::isfinite(mean).all().item<bool>() && at::isfinite(square).all().item<bool>() && (square>=0).all().item<bool>(),"Invalid Adam accumulator");
  states_.at(index)={mean.detach().clone(),square.detach().clone(),
    at::scalar_tensor(static_cast<double>(step),p.options().device(mode_==1 ? p.device() : at::Device(at::kCPU)))};
}
}
