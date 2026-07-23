/* benchmark: fib-loop (C baseline)
 * Iterative Fibonacci seeded by the outer index, accumulated with 32-bit wrap.
 * The wrapping arithmetic uses uint32_t (defined two's-complement wrap; signed
 * overflow would be UB without -fwrapv) and is reinterpreted to int32_t for the
 * signed checksum, matching subscript's i32 wrap.
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <time.h>

enum { INNER = 32, OUTER = 3000000 };

static int32_t workload(void) {
    uint32_t result = 0;
    for (uint32_t iter = 0; iter < (uint32_t)OUTER; iter++) {
        uint32_t a = iter & 1023u;
        uint32_t b = 1u + (result & 1023u);
        for (int i = 0; i < INNER; i++) {
            uint32_t t = a + b;
            a = b;
            b = t;
        }
        result += b;
    }
    return (int32_t)result;
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
