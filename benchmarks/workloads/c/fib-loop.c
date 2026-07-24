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
    /* Warm-up and timed counts come from argv, defaulting to the 3/11 floor, so
     * the runner drives this baseline with the same counts as every other
     * subject. Only the workload call is timed. */
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
    int mid = timed / 2;
    double median = (timed % 2 == 1) ? times[mid] : (times[mid - 1] + times[mid]) / 2.0;
    printf("%d %.9f\n", checksum, median);
    free(times);
    return 0;
}
