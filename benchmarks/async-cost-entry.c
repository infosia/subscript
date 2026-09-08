/* Timing entry program for the §94.4 async cost measurement.
 *
 * Timed span (compiler.md section 94.4): one whole program run on a fresh
 * subscript_rt_context — creation, `subscript_init`, the exported `main`,
 * the other exported async functions, every host checkpoint to quiescence,
 * and Context release. Code compilation is outside it.
 *
 * argv[1] is the minimum warm-up iteration count, argv[2] the timed
 * iteration count, and argv[3] an optional minimum warm-up execution time in
 * nanoseconds. Warm-up ends when both floors are met. stdout carries the workload's sink bytes from the first run.
 * stderr carries `warmup <iterations> <ns>`, then one
 * `sample <index> <ns>` line per timed run, then
 * `checkpoints <n>`, `unfinished <n>`, `live-bytes <n>`,
 * `live-allocations <n>`, and `checksum-stable <0|1>`. The four counters
 * are read on the first warm-up run, so no timed sample includes an
 * observer call. `SUBSCRIPT_ASYNC_COST_NO_UNFINISHED` drops the unfinished
 * read, for a revision whose runtime predates that observer.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static uint64_t monotonic_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}

extern void subscript_kick_async_exports(subscript_rt_context *ctx);

static void call_entry(subscript_rt_context *ctx, subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
}

int main(int argc, char **argv) {
    const long warmup = argc > 1 ? atol(argv[1]) : 3;
    const long timed = argc > 2 ? atol(argv[2]) : 11;
    const uint64_t warmup_floor_ns =
        argc > 3 ? (uint64_t)strtoull(argv[3], NULL, 10) : 0ull;
    if (warmup < 0 || timed < 1) {
        fprintf(stderr, "usage: <warmup> <timed> [warmup-floor-ns]\n");
        return 2;
    }
    unsigned char *first = NULL;
    uint64_t first_len = 0;
    int stable = 1;
    uint64_t checkpoints = 0;
    uint64_t unfinished = 0;
    uint64_t live_bytes = 0;
    uint64_t live_allocations = 0;
    /* The warm-up phase ends only when both floors are met: the iteration
     * count and the measured execution time of compiler.md section 9. Only
     * timed spans accumulate, so neither compilation nor process startup
     * counts toward the floor. */
    long warmup_iterations = 0;
    uint64_t warmup_elapsed_ns = 0;
    long timed_iterations = 0;
    for (long index = 0; timed_iterations < timed; index++) {
        const int warming = warmup_iterations < warmup ||
                            warmup_elapsed_ns < warmup_floor_ns;
        if (!warming && timed_iterations == 0) {
            fprintf(stderr, "warmup %ld %llu\n", warmup_iterations,
                    (unsigned long long)warmup_elapsed_ns);
        }
        const uint64_t started = monotonic_ns();
        subscript_rt_context *ctx = subscript_rt_ctx_new();
        if (ctx == NULL) {
            fprintf(stderr, "context creation failed\n");
            return 2;
        }
        call_entry(ctx, subscript_init);
        if (subscript_rt_ctx_trap_kind(ctx) == 0) {
            call_entry(ctx, subscript_export_main);
        }
        if (subscript_rt_ctx_trap_kind(ctx) == 0) {
            call_entry(ctx, subscript_kick_async_exports);
        }
        uint64_t steps = 0;
        while (subscript_rt_ctx_trap_kind(ctx) == 0 &&
               subscript_rt_ctx_async_pending(ctx) != 0) {
            (void)subscript_rt_ctx_async_step(ctx);
            steps++;
        }
        if (index == 0) {
            /* The first run is always a warm-up run. */
            /* Counter reads happen on a warm-up run only, so no timed
             * sample includes an observer call. */
            checkpoints = steps;
            live_bytes = subscript_rt_ctx_live_bytes(ctx);
            live_allocations = subscript_rt_ctx_live_allocations(ctx);
#ifndef SUBSCRIPT_ASYNC_COST_NO_UNFINISHED
            unfinished = subscript_rt_ctx_async_unfinished(ctx);
#endif
        }
        uint64_t len = 0;
        const unsigned char *sink = subscript_rt_ctx_stdout(ctx, &len);
        if (first == NULL) {
            first = (unsigned char *)malloc(len == 0 ? 1 : (size_t)len);
            if (first == NULL) {
                fprintf(stderr, "sink copy failed\n");
                return 2;
            }
            memcpy(first, sink, (size_t)len);
            first_len = len;
            fwrite(sink, 1, (size_t)len, stdout);
        } else if (len != first_len || memcmp(first, sink, (size_t)len) != 0) {
            stable = 0;
        }
        const uint32_t trap = subscript_rt_ctx_trap_kind(ctx);
        subscript_rt_ctx_release(ctx);
        const uint64_t elapsed = monotonic_ns() - started;
        if (trap != 0) {
            fprintf(stderr, "trap %u\n", trap);
            return 3;
        }
        if (warming) {
            warmup_elapsed_ns += elapsed;
            warmup_iterations += 1;
        } else {
            fprintf(stderr, "sample %ld %llu\n", timed_iterations,
                    (unsigned long long)elapsed);
            timed_iterations += 1;
        }
    }
    fprintf(stderr, "checkpoints %llu\n", (unsigned long long)checkpoints);
    fprintf(stderr, "unfinished %llu\n", (unsigned long long)unfinished);
    fprintf(stderr, "live-bytes %llu\n", (unsigned long long)live_bytes);
    fprintf(stderr, "live-allocations %llu\n",
            (unsigned long long)live_allocations);
    fprintf(stderr, "checksum-stable %d\n", stable);
    free(first);
    fflush(stdout);
    return 0;
}
