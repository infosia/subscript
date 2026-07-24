/* benchmark: queen (C baseline)
 * Count solutions to the 13-queens problem by bitmask backtracking.
 * Masks use uint32_t (defined shifts and negation) matching subscript's i32
 * bitwise ops over non-negative masks. Checksum: solution count (int32) = 73712.
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <time.h>

enum { N = 13 };

static int32_t solve(uint32_t cols, uint32_t ld, uint32_t rd, uint32_t all) {
    if (cols == all) {
        return 1;
    }
    int32_t count = 0;
    uint32_t poss = ~(cols | ld | rd) & all;
    while (poss != 0) {
        uint32_t p = poss & (0u - poss);
        poss = poss - p;
        count += solve(cols | p, (ld | p) << 1, (rd | p) >> 1, all);
    }
    return count;
}

static int32_t workload(void) {
    /* volatile defeats constant-folding of the backtracking to 73712. */
    volatile int n = N;
    uint32_t all = ((uint32_t)1 << n) - 1u;
    return solve(0, 0, 0, all);
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
