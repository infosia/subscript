/* benchmark: fib-recursive (C baseline)
 * Naive recursive Fibonacci, fib(31) = 1346269.
 * Self-timed: >=3 warm-up + >=11 timed runs, prints "<checksum> <median_seconds>".
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

static double now_seconds(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec + (double)ts.tv_nsec * 1e-9;
}

static int cmp_double(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return (x > y) - (x < y);
}

int main(void) {
    const int warmup = 3, timed = 11;
    int32_t checksum = 0;
    double times[11];
    for (int i = 0; i < warmup; i++) {
        checksum = workload();
    }
    for (int i = 0; i < timed; i++) {
        double t0 = now_seconds();
        checksum = workload();
        double t1 = now_seconds();
        times[i] = t1 - t0;
    }
    qsort(times, (size_t)timed, sizeof(double), cmp_double);
    printf("%d %.9f\n", checksum, times[timed / 2]);
    return 0;
}
