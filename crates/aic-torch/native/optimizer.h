#pragma once
#include <ATen/ATen.h>
#include <vector>

namespace aic {
struct AdamState { at::Tensor mean, square, step; };
// Only the current learner's dense FP32, one-device, no-decay Adam contract.
// Mode 0 mirrors Python foreach; mode 1 invokes the ATen fused Adam kernel.
class Adam {
 public:
  Adam(std::vector<at::Tensor> parameters, double lr, double beta1, double beta2, double eps, int mode);
  void step();
  void zero_grad();
  double clip_grad_norm(double limit);
  const AdamState& state(size_t index) const { return states_.at(index); }
  void restore(size_t index, int64_t step, const at::Tensor& mean, const at::Tensor& square);
 private:
  std::vector<at::Tensor> parameters_;
  std::vector<AdamState> states_;
  double lr_, beta1_, beta2_, eps_;
  int mode_;
};
}
