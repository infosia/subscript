/* Timing entry program for the ship-tier AOT subject of the P4
 * performance gate (specs/blocks/compiler.md sections 3 and 9).
 *
 * It is the measurement counterpart of the gate's normal entry
 * (`AOT_ENTRY_C` in codegen/src/aot.rs): same C-ABI surface, but the
 * exported `main` is called `warmup + timed` times and each call is
 * timed on its own with CLOCK_MONOTONIC.
 *
 * Timed span: the `ss_export_main` call alone. Context creation, the
 * module initializer `ss_init`, reading the stdout sink, and Context
 * release are all outside it.
 *
 * A fresh Context per run makes each run start from the same state,
 * and `ss_init` restores the module globals, so every run is the same
 * computation; the entry checks that by comparing the sink bytes of
 * every run against the first.
 *
 * stdout: the sink bytes of the workload (the program's output).
 * stderr: one machine-readable line per timed run,
 * `sample <index> <ns>`, followed by `checksum-stable <0|1>`.
 */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#if defined(_WIN32)
#include <io.h>
#include <fcntl.h>
#include <windows.h>
#endif

extern void *sub_rt_ctx_new(void);
extern void sub_rt_ctx_release(void *ctx);
extern const unsigned char *sub_rt_ctx_stdout(const void *ctx, uint64_t *len);
extern uint32_t sub_rt_ctx_trap_kind(const void *ctx);
extern uint32_t sub_rt_ctx_trap_pos_id(const void *ctx);
extern const unsigned char *sub_rt_ctx_trap_message(const void *ctx, uint64_t *len);

extern void ss_init(void *ctx);
extern void ss_export_main(void *ctx);

/* Nanoseconds since an unspecified monotonic epoch. The MSVC UCRT has no
 * clock_gettime/CLOCK_MONOTONIC, so on Windows the same monotonic span is
 * read from QueryPerformanceCounter, converted to nanoseconds by exact
 * integer arithmetic (no precision loss for the reported timed span). The
 * POSIX path is unchanged on every other platform. */
#if defined(_WIN32)
static uint64_t monotonic_ns(void) {
    LARGE_INTEGER freq;
    LARGE_INTEGER counter;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&counter);
    const uint64_t f = (uint64_t)freq.QuadPart;
    const uint64_t c = (uint64_t)counter.QuadPart;
    return (c / f) * 1000000000ull + (c % f) * 1000000000ull / f;
}
#else
static uint64_t monotonic_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}
#endif

/* Reports a trap the way the gate's entry program does, so a failing
 * benchmark run is diagnosable with the same reader. */
static void report_trap(const void *ctx) {
    uint64_t mlen = 0;
    const unsigned char *msg = sub_rt_ctx_trap_message(ctx, &mlen);
    fprintf(stderr, "trap %u %u ", sub_rt_ctx_trap_kind(ctx), sub_rt_ctx_trap_pos_id(ctx));
    if (mlen > 0) {
        fwrite(msg, 1, (size_t)mlen, stderr);
    }
    fputc('\n', stderr);
}

int main(int argc, char **argv) {
#if defined(_WIN32)
    /* The sink bytes are compared byte-for-byte against the LF golden; the
     * MSVCRT opens stdout in text mode and would translate '\n' to '\r\n'.
     * Binary mode writes the sink through unchanged. No-op off Windows. */
    _setmode(_fileno(stdout), _O_BINARY);
#endif
    int warmup = 3;
    int timed = 11;
    if (argc >= 3) {
        warmup = atoi(argv[1]);
        timed = atoi(argv[2]);
    }
    if (warmup < 0 || timed < 1) {
        fprintf(stderr, "usage: aot-bench <warmup-runs> <timed-runs>\n");
        return 2;
    }

    unsigned char *first = NULL;
    size_t first_len = 0;
    int stable = 1;

    for (int run = 0; run < warmup + timed; run += 1) {
        void *ctx = sub_rt_ctx_new();
        if (ctx == NULL) {
            free(first);
            return 2;
        }
        ss_init(ctx);
        if (sub_rt_ctx_trap_kind(ctx) != 0) {
            report_trap(ctx);
            sub_rt_ctx_release(ctx);
            free(first);
            return 3;
        }

        const uint64_t start = monotonic_ns();
        ss_export_main(ctx);
        const uint64_t end = monotonic_ns();

        if (sub_rt_ctx_trap_kind(ctx) != 0) {
            report_trap(ctx);
            sub_rt_ctx_release(ctx);
            free(first);
            return 3;
        }

        uint64_t len = 0;
        const unsigned char *out = sub_rt_ctx_stdout(ctx, &len);
        if (first == NULL) {
            first_len = (size_t)len;
            first = (unsigned char *)malloc(first_len + 1);
            if (first == NULL) {
                sub_rt_ctx_release(ctx);
                return 2;
            }
            if (first_len > 0) {
                memcpy(first, out, first_len);
            }
        } else if ((size_t)len != first_len || (first_len > 0 && memcmp(out, first, first_len) != 0)) {
            stable = 0;
        }
        sub_rt_ctx_release(ctx);

        if (run >= warmup) {
            fprintf(stderr, "sample %d %llu\n", run - warmup, (unsigned long long)(end - start));
        }
    }

    fprintf(stderr, "checksum-stable %d\n", stable);
    if (first_len > 0) {
        fwrite(first, 1, first_len, stdout);
    }
    fflush(stdout);
    free(first);
    return 0;
}
