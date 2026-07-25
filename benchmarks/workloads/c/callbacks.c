/* benchmark: callbacks (C baseline)
 * Loop spelling of the indexed map/filter/reduce pipeline over 1000000 signed
 * i32 values from the fixed LCG, repeated 20 times. The filter removes exactly
 * 250000 values per round. All signed arithmetic wraps under -fwrapv.
 * Checksum: the i32-wrapping sum of each round's reduce result.
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <time.h>

enum { COUNT = 1000000, ROUNDS = 20 };

static int32_t workload(void) {
    int32_t *input = (int32_t *)malloc((size_t)COUNT * sizeof(int32_t));
    int32_t *mapped = (int32_t *)malloc((size_t)COUNT * sizeof(int32_t));
    int32_t *filtered = (int32_t *)malloc((size_t)COUNT * sizeof(int32_t));
    if (input == NULL || mapped == NULL || filtered == NULL) {
        free(filtered);
        free(mapped);
        free(input);
        return 0;
    }

    int32_t state = (int32_t)0x12345678u;
    for (int32_t i = 0; i < COUNT; i++) {
        state = state * 1664525 + 1013904223;
        input[i] = state;
    }

    int32_t checksum = 0;
    for (int32_t round = 0; round < ROUNDS; round++) {
        for (int32_t i = 0; i < COUNT; i++) {
            mapped[i] = input[i] + i;
        }
        int32_t kept = 0;
        for (int32_t i = 0; i < COUNT; i++) {
            int32_t value = mapped[i];
            if (((value ^ i) & 3) != 0) {
                filtered[kept] = value;
                kept++;
            }
        }
        int32_t reduced = 0;
        for (int32_t i = 0; i < kept; i++) {
            reduced = reduced + filtered[i];
            reduced = reduced + i;
        }
        checksum = checksum + reduced;
    }

    free(filtered);
    free(mapped);
    free(input);
    return checksum;
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
    fprintf(stderr, "spread %.9f %.9f\n", times[0], times[timed - 1]);
    free(times);
    return 0;
}
