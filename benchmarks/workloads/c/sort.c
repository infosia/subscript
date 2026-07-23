/* benchmark: sort (C baseline)
 * Quicksort (median-of-three pivot, recurse-smaller/iterate-larger) of 300000
 * u32 values from the fixed LCG state = state*1664525 + 1013904223, compared as
 * unsigned. Checksum: order-sensitive rolling hash h = h*31 + a[i] (u32 wrap).
 * Array construction, sort, and hash are all inside the timed workload, so the
 * timed span matches subscript's whole main.
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <time.h>

enum { COUNT = 300000 };

static int median3(const uint32_t *a, int lo, int mid, int hi) {
    uint32_t x = a[lo], y = a[mid], z = a[hi];
    if (x < y) {
        if (y < z) return mid;
        if (x < z) return hi;
        return lo;
    }
    if (x < z) return lo;
    if (y < z) return hi;
    return mid;
}

static void quicksort(uint32_t *a, int lo, int hi) {
    int l = lo, h = hi;
    while (l < h) {
        int mid = l + ((h - l) / 2);
        int pivot_index = median3(a, l, mid, h);
        uint32_t tmp = a[pivot_index];
        a[pivot_index] = a[h];
        a[h] = tmp;
        uint32_t pivot = a[h];
        int store = l;
        for (int i = l; i < h; i++) {
            if (a[i] < pivot) {
                tmp = a[i];
                a[i] = a[store];
                a[store] = tmp;
                store++;
            }
        }
        tmp = a[store];
        a[store] = a[h];
        a[h] = tmp;
        if (store - l < h - store) {
            quicksort(a, l, store - 1);
            l = store + 1;
        } else {
            quicksort(a, store + 1, h);
            h = store - 1;
        }
    }
}

static uint32_t workload(void) {
    uint32_t state = 0x12345678u;
    uint32_t *a = (uint32_t *)malloc((size_t)COUNT * sizeof(uint32_t));
    for (int i = 0; i < COUNT; i++) {
        state = state * 1664525u + 1013904223u;
        a[i] = state;
    }
    quicksort(a, 0, COUNT - 1);
    uint32_t h = 0;
    for (int i = 0; i < COUNT; i++) {
        h = h * 31u + a[i];
    }
    free(a);
    return h;
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
    uint32_t checksum = 0;
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
    printf("%u %.9f\n", checksum, median);
    free(times);
    return 0;
}
