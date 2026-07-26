/* benchmark: particles (C baseline)
 * 100000 value-struct particles over 1000 fixed-dt steps (velocity += acc*dt;
 * position += velocity*dt), dt = 1.0 with integer-valued accelerations so every
 * f64 intermediate is exact. Checksum: i32-wrapping sum of each final position
 * cast to i32 (accumulated in uint32_t for defined wrap, reinterpreted signed).
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <time.h>

enum { COUNT = 100000, STEPS = 1000 };

typedef struct {
    double position;
    double velocity;
} Particle;

static int32_t workload(void) {
    const double dt = 1.0;
    Particle *particles = (Particle *)malloc((size_t)COUNT * sizeof(Particle));
    for (int i = 0; i < COUNT; i++) {
        particles[i].position = 0.0;
        particles[i].velocity = 0.0;
    }
    for (int step = 0; step < STEPS; step++) {
        for (int i = 0; i < COUNT; i++) {
            double acc = (double)(i % 16);
            particles[i].velocity += acc * dt;
            particles[i].position += particles[i].velocity * dt;
        }
    }
    uint32_t sum = 0;
    for (int i = 0; i < COUNT; i++) {
        sum += (uint32_t)(int32_t)particles[i].position;
    }
    free(particles);
    return (int32_t)sum;
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
