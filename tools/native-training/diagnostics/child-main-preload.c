// Explicit Linux diagnostic: separate child main work from process start/wait.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <time.h>

typedef int (*Main)(int, char**, char**);
static Main original_main;
static double now(void) {
    struct timespec value;
    clock_gettime(CLOCK_MONOTONIC, &value);
    return value.tv_sec + value.tv_nsec * 1e-9;
}
static double seconds(struct timeval value) {
    return value.tv_sec + value.tv_usec * 1e-6;
}
static int profile_main(int argc, char** argv, char** env) {
    struct rusage before, after;
    getrusage(RUSAGE_SELF, &before);
    double began = now();
    int result = original_main(argc, argv, env);
    double elapsed = now() - began;
    getrusage(RUSAGE_SELF, &after);
    fprintf(stderr, "AIC_CHILD_PROFILE {\"main_seconds\":%.9f,"
            "\"main_cpu_seconds\":%.9f,\"total_cpu_seconds\":%.9f}\n", elapsed,
            seconds(after.ru_utime) + seconds(after.ru_stime) -
            seconds(before.ru_utime) - seconds(before.ru_stime),
            seconds(after.ru_utime) + seconds(after.ru_stime));
    return result;
}
int __libc_start_main(Main main, int argc, char** argv, void (*init)(void),
                      void (*fini)(void), void (*rtld_fini)(void), void* stack_end) {
    typedef int (*Start)(Main, int, char**, void (*)(void), void (*)(void), void (*)(void), void*);
    Start start = (Start)dlsym(RTLD_NEXT, "__libc_start_main");
    if (!start) abort();
    original_main = main;
    return start(profile_main, argc, argv, init, fini, rtld_fini, stack_end);
}
