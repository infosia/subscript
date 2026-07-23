/* benchmark: primes (C baseline)
 * Count primes up to 500000 by trial division (j*j <= n, no sqrt).
 * Checksum: the prime count (int32).
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <time.h>

enum { LIMIT = 500000 };

static int is_prime(int32_t n) {
    if (n < 2) {
        return 0;
    }
    for (int32_t j = 2; j * j <= n; j++) {
        if (n % j == 0) {
            return 0;
        }
    }
    return 1;
}

static int32_t workload(void) {
    int32_t count = 0;
    for (int32_t n = 2; n <= LIMIT; n++) {
        if (is_prime(n)) {
            count++;
        }
    }
    return count;
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
