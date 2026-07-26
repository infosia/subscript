/* benchmark: fib-recursive (C baseline)
 * Naive recursive Fibonacci, fib(31) = 1346269.
 * Self-timed: >=200 ms and >=3 warm-up iterations + >=11 timed runs; prints
 * "<checksum> <median_seconds>".
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <time.h>

static int32_t fib(int32_t n) {
    if (n < 2) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

static int32_t workload(void) {
    /* volatile defeats constant-folding of fib(31) to 1346269, so the
     * recursion is actually evaluated (matching every other subject). */
    volatile int32_t n = 31;
    return fib(n);
}

#if defined(_WIN32)
#include <windows.h>
static double now_seconds(void) {
    LARGE_INTEGER freq, counter;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&counter);
    return (double)counter.QuadPart / (double)freq.QuadPart;
}
#else
static double now_seconds(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec + (double)ts.tv_nsec * 1e-9;
}
#endif

static int cmp_double(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return (x > y) - (x < y);
}

int main(int argc, char **argv) {
    /* The argv warm-up count is a minimum; measured workload execution also
     * has to reach the 200 ms methodology floor. */
    int warmup = 3, timed = 11;
    if (argc >= 3) {
        warmup = atoi(argv[1]);
        timed = atoi(argv[2]);
    }
    if (warmup < 0 || timed < 1) {
        fprintf(stderr, "usage: %s <warmup> <timed>\n", argv[0]);
        return 2;
    }
    double *times = (double *)malloc((size_t)timed * sizeof(double));
    if (times == NULL) {
        return 2;
    }
    int32_t checksum = 0;
    int warmup_iterations = 0;
    double warmup_seconds = 0.0;
    while (warmup_iterations < warmup || warmup_iterations < 3 ||
           warmup_seconds < 0.200) {
        double t0 = now_seconds();
        checksum = workload();
        double t1 = now_seconds();
        warmup_seconds += t1 - t0;
        warmup_iterations++;
    }
    /* This volatile sink makes the warm-up result observable so the optimizer
     * cannot delete the contractually required warm-up as dead code. */
    volatile int32_t warmup_sink = checksum;
    (void)warmup_sink;
    for (int i = 0; i < timed; i++) {
        double t0 = now_seconds();
        checksum = workload();
        double t1 = now_seconds();
        times[i] = t1 - t0;
    }
    fprintf(stderr, "warmup %d %.9f\n", warmup_iterations, warmup_seconds);
    for (int i = 0; i < timed; i++) {
        fprintf(stderr, "sample %d %.9f\n", i, times[i]);
    }
    qsort(times, (size_t)timed, sizeof(double), cmp_double);
    int mid = timed / 2;
    double median = (timed % 2 == 1) ? times[mid] : (times[mid - 1] + times[mid]) / 2.0;
    printf("%d %.9f\n", checksum, median);
    free(times);
    return 0;
}
