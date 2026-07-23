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
