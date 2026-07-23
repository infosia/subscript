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
