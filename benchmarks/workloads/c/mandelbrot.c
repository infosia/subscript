/* benchmark: mandelbrot (C baseline)
 * 800x800 escape-iteration grid, escape test x^2 + y^2 >= 4, cap 255, all f64.
 * Products are stored in temporaries and only added/subtracted afterwards; with
 * -ffp-contract=off no multiply-add is fused, so escape counts are bit-identical
 * across subjects. Checksum: sum of escape counts (int64).
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <time.h>

enum { GRID = 800, MAX_ITER = 255 };

static int32_t escapes(double cx, double cy) {
    double zx = 0.0, zy = 0.0;
    for (int32_t i = 0; i < MAX_ITER; i++) {
        double zx2 = zx * zx;
        double zy2 = zy * zy;
        if (zx2 + zy2 >= 4.0) {
            return i;
        }
        double xy = zx * zy;
        zy = xy + xy + cy;
        zx = zx2 - zy2 + cx;
    }
    return MAX_ITER;
}

static int64_t workload(void) {
    const double xmin = -2.0, xmax = 0.5, ymin = -1.25, ymax = 1.25;
    int64_t checksum = 0;
    for (int32_t py = 0; py < GRID; py++) {
        double cy = ymin + (ymax - ymin) * (double)py / (double)GRID;
        for (int32_t px = 0; px < GRID; px++) {
            double cx = xmin + (xmax - xmin) * (double)px / (double)GRID;
            checksum += (int64_t)escapes(cx, cy);
        }
    }
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
    int64_t checksum = 0;
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
    volatile int64_t warmup_sink = checksum;
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
    printf("%lld %.9f\n", (long long)checksum, median);
    free(times);
    return 0;
}
