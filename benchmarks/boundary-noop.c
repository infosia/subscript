#define _POSIX_C_SOURCE 200809L
#if defined(__APPLE__)
/* Darwin hides its non-POSIX raw clock ids when only _POSIX_C_SOURCE is set. */
#define _DARWIN_C_SOURCE
#endif

#include "boundary-noop.h"

#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define BN_WARMUP_FLOOR_NS INT64_C(200000000)
#define BN_MIN_WARMUP_ITERATIONS 3
#define BN_TIMED_SAMPLES 15
#define BN_QUANTUM_PROBE_PAIRS 100000

struct BnBindGroup_T {
    uint32_t live;
};

/* Every argument addition is an observable part of the noop fixture. Keeping
 * the shared checksum volatile prevents the backend translation unit from
 * coalescing the four bnDraw additions into one store. */
static volatile uint64_t bn_checksum;
static int64_t bn_warmup_ns;
static int32_t bn_warmup_iterations;
static int32_t bn_warming = 1;
static int32_t bn_sample_count;
static int64_t bn_samples[BN_TIMED_SAMPLES];
static int64_t bn_quantum_ns;
static int32_t bn_quantum_measured;

BnBindGroup bnBindGroupCreate(void) {
    BnBindGroup group = (BnBindGroup)malloc(sizeof(*group));
    if (group != NULL) {
        group->live = UINT32_C(1);
    }
    return group;
}

void bnBindGroupRelease(BnBindGroup g) {
    free(g);
}

void bnSetBindGroup(uint32_t index, BnBindGroup group, BnOffsets offsets) {
    (void)group;
    bn_checksum += index;
    for (size_t i = 0; i < offsets.count; ++i) {
        bn_checksum += offsets.data[i];
    }
}

void bnDraw(uint32_t a, uint32_t b, uint32_t c, uint32_t d) {
    bn_checksum += a;
    bn_checksum += b;
    bn_checksum += c;
    bn_checksum += d;
}

int64_t bnNow(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC_RAW, &ts) != 0) {
        return INT64_C(-1);
    }
    return (int64_t)ts.tv_sec * INT64_C(1000000000) + (int64_t)ts.tv_nsec;
}

static void bnMeasureClockQuantum(void) {
    int64_t minimum = INT64_MAX;
    for (int32_t i = 0; i < BN_QUANTUM_PROBE_PAIRS; ++i) {
        int64_t t0 = bnNow();
        int64_t t1 = bnNow();
        if (t0 >= INT64_C(0) && t1 > t0 && t1 - t0 < minimum) {
            minimum = t1 - t0;
        }
    }
    /* INT64_MAX fails the runner's quantum gate if the clock did not advance
     * during any probe pair. */
    bn_quantum_ns = minimum;
    bn_quantum_measured = 1;
}

int32_t bnMoreSamples(void) {
    if (!bn_quantum_measured) {
        bnMeasureClockQuantum();
    }
    return bn_warming || bn_sample_count < BN_TIMED_SAMPLES;
}

void bnRecordSample(int64_t t0, int64_t t1) {
    int64_t elapsed = t1 >= t0 ? t1 - t0 : INT64_C(0);
    if (bn_warming) {
        bn_warmup_ns += elapsed;
        bn_warmup_iterations += 1;
        if (bn_warmup_ns >= BN_WARMUP_FLOOR_NS &&
            bn_warmup_iterations >= BN_MIN_WARMUP_ITERATIONS) {
            bn_warming = 0;
            /* Timed work has a fixed call count; discarded warm-up work does
             * not belong in the cross-variant checksum. */
            bn_checksum = UINT64_C(0);
        }
        return;
    }
    if (bn_sample_count < BN_TIMED_SAMPLES) {
        bn_samples[bn_sample_count] = elapsed;
        bn_sample_count += 1;
    }
}

void bnReport(void) {
    printf(
        "bound-call checksum=%" PRIu64 " warmup_ms=%" PRIi64
        " quantum_ns=%" PRIi64 " samples_ns=",
        bn_checksum,
        bn_warmup_ns / INT64_C(1000000),
        bn_quantum_ns
    );
    for (int32_t i = 0; i < BN_TIMED_SAMPLES; ++i) {
        if (i != 0) {
            putchar(',');
        }
        printf("%" PRIi64, bn_samples[i]);
    }
    putchar('\n');
    fflush(stdout);
}
