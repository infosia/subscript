/*
 * boundary-noop.h — C fixture for the bound-call boundary-price benchmark.
 *
 * The backend lives in its own translation unit so the calls measured by the
 * benchmark remain real calls.  The (pointer, count) BnOffsets parameter is a
 * borrowed view: bnSetBindGroup reads the elements during the call and does
 * not retain the pointer.
 */

#ifndef SUBSCRIPT_BOUNDARY_NOOP_H
#define SUBSCRIPT_BOUNDARY_NOOP_H

#include <stddef.h>
#include <stdint.h>

/* Opaque bind-group handle; callers never observe its representation. */
typedef struct BnBindGroup_T *BnBindGroup;

/* Borrowed view over a contiguous run of dynamic offsets. */
typedef struct BnOffsets {
    const uint32_t *data;
    size_t count;
} BnOffsets;

/* Creates one opaque bind-group handle. */
BnBindGroup bnBindGroupCreate(void);

/* Releases a handle returned by bnBindGroupCreate. */
void bnBindGroupRelease(BnBindGroup g);

/* Adds index and every borrowed offset to the benchmark checksum. */
void bnSetBindGroup(uint32_t index, BnBindGroup group, BnOffsets offsets);

/* Adds the four draw arguments to the benchmark checksum. */
void bnDraw(uint32_t a, uint32_t b, uint32_t c, uint32_t d);

/* Returns CLOCK_MONOTONIC_RAW in nanoseconds. */
int64_t bnNow(void);

/* Returns one while another warm-up or timed region is required. */
int32_t bnMoreSamples(void);

/* Records one measured region span. */
void bnRecordSample(int64_t t0, int64_t t1);

/* Prints `bound-call checksum=<u64> warmup_ms=<n> quantum_ns=<n>
 * samples_ns=<s1>,...,<s15>` as one machine-readable line. */
void bnReport(void);

#endif
