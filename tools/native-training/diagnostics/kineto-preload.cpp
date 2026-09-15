// Linux-only, explicit diagnostic launcher for the existing LibTorch CLI.
// No Rust/runtime edits, stack collection or normal-run hooks.
#include <torch/csrc/autograd/profiler_kineto.h>
#include <torch/csrc/profiler/orchestration/observer.h>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <iostream>

using Main = int (*)(int, char**, char**);
static Main original_main;

static int profile_main(int argc, char** argv, char** env) {
  const char* output = std::getenv("AIC_KINETO_TRACE");
  // Construction subprocesses inherit LD_PRELOAD, but must not start profilers.
  if (!output || argc < 3 || std::strcmp(argv[1], "learning") != 0 ||
      std::strcmp(argv[2], "train") != 0) {
    return original_main(argc, argv, env);
  }
  namespace profiler = torch::autograd::profiler;
  using namespace torch::profiler::impl;
  const char* shapes = std::getenv("AIC_KINETO_SHAPES");
  ProfilerConfig config(ProfilerState::KINETO, shapes && std::strcmp(shapes, "1") == 0);
  const std::set<ActivityType> activities{ActivityType::CPU, ActivityType::CUDA};
  try {
    profiler::prepareProfiler(config, activities);
    profiler::enableProfiler(config, activities);
  } catch (const std::exception& error) {
    std::cerr << "aic-profile.start: " << error.what() << '\n';
    return 125;
  }
  const int result = original_main(argc, argv, env);
  try {
    profiler::disableProfiler()->save(output);
  } catch (const std::exception& error) {
    std::cerr << "aic-profile.save: " << error.what() << '\n';
    return 125;
  }
  return result;
}

extern "C" int __libc_start_main(Main main, int argc, char** argv,
                                  void (*init)(), void (*fini)(),
                                  void (*rtld_fini)(), void* stack_end) {
  using Start = int (*)(Main, int, char**, void (*)(), void (*)(), void (*)(), void*);
  auto start = reinterpret_cast<Start>(dlsym(RTLD_NEXT, "__libc_start_main"));
  if (!start) std::abort();
  original_main = main;
  return start(profile_main, argc, argv, init, fini, rtld_fini, stack_end);
}
