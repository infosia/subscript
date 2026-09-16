/*
 * main.c — a C host for content it did not write.
 *
 * The script is compiled under the sandbox profile
 * (specs/blocks/compiler.md section 109). The host owns the three limits:
 * it sets an allocation quota and a stack budget before the first script
 * call, and it sets the interrupt flag from a second thread while the
 * script runs. The limits are the host's facts; the compiled program
 * carries none of them.
 *
 * The stack budget is below this thread's stack size. That is the host's
 * fact to supply: the runtime measures the script's depth against the
 * budget, not against the real stack.
 *
 * After the interrupt, two phases show how memory returns under the
 * profile (section 109.8a). Phase A paces the collect from the host.
 * Phase B lets the script collect at the end of its own frame.
 */

#include "subscript_runtime.h"

#include <inttypes.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#if defined(_WIN32)
#include <fcntl.h>
#include <io.h>
#include <windows.h>
#else
#include <pthread.h>
#include <time.h>
#endif

/* The generated symbol of the exported `tick(): void`. */
void subscript_export_tick(subscript_rt_context *ctx);

/* The generated symbols of the two memory-pattern exports. `frame`
 * allocates and never collects; `frameAndCollect` collects at its end. */
void subscript_export_frame(subscript_rt_context *ctx);
void subscript_export_frameAndCollect(subscript_rt_context *ctx);

/* The limits this host gives untrusted content. Both are well under the
 * defaults the CLI applies (64 MiB and 512 KiB), because this host knows
 * its own budget and its own thread. */
#define HOST_ALLOC_QUOTA UINT64_C(16777216)  /* 16 MiB */
#define HOST_STACK_BUDGET UINT64_C(262144)   /* 256 KiB */

/* The memory phases below run under a tighter quota, so a small program
 * shows the pacer at work. The threshold is three quarters of the quota:
 * the host picks that fraction, and the runtime knows nothing about it. */
#define HOST_MEMORY_QUOTA UINT64_C(1048576)  /* 1 MiB */
#define HOST_MEMORY_THRESHOLD (HOST_MEMORY_QUOTA / 4u * 3u)
#define HOST_MEMORY_FRAMES 24u

/* How long the script runs before the second thread stops it. */
#define HOST_INTERRUPT_AFTER_MILLIS 20u

/* The observer receives one script line without its trailing newline.
 * Writing it to the host's own stdout keeps host and script lines in
 * order and keeps the cumulative sink from growing. */
static void hostObserveScriptPrint(
    void *userdata,
    const uint8_t *line,
    uint64_t lineLength) {
    FILE *stream = (FILE *)userdata;
    if (lineLength != 0u) {
        fwrite(line, 1u, (size_t)lineLength, stream);
    }
    fputc('\n', stream);
}

/* The second thread. The host obtains the interrupt handle on this
 * thread, before the run, and passes it to the thread.
 * `subscript_rt_interrupt_set` is the one call any thread can make while
 * the owning thread runs script code: it sets one atomic flag in a cell
 * outside the Context and reads no Context field. */
#if defined(_WIN32)
static DWORD WINAPI hostInterruptThread(LPVOID argument) {
    Sleep(HOST_INTERRUPT_AFTER_MILLIS);
    subscript_rt_interrupt_set((const subscript_rt_interrupt *)argument);
    return 0u;
}

static bool hostStartInterruptThread(
    const subscript_rt_interrupt *handle,
    HANDLE *thread) {
    *thread = CreateThread(NULL, 0, hostInterruptThread, (LPVOID)handle, 0, NULL);
    return *thread != NULL;
}

static void hostJoinInterruptThread(HANDLE thread) {
    WaitForSingleObject(thread, INFINITE);
    CloseHandle(thread);
}
#else
static void *hostInterruptThread(void *argument) {
    struct timespec delay;
    delay.tv_sec = 0;
    delay.tv_nsec = (long)HOST_INTERRUPT_AFTER_MILLIS * 1000000L;
    nanosleep(&delay, NULL);
    subscript_rt_interrupt_set((const subscript_rt_interrupt *)argument);
    return NULL;
}

static bool hostStartInterruptThread(
    const subscript_rt_interrupt *handle,
    pthread_t *thread) {
    return pthread_create(thread, NULL, hostInterruptThread, (void *)handle) == 0;
}

static void hostJoinInterruptThread(pthread_t thread) {
    pthread_join(thread, NULL);
}
#endif

/* Every script entry is bracketed so the runtime tracks script depth.
 * enter_script at depth 0 also records the stack floor the budget is
 * measured against. */
static bool hostCallScript(
    subscript_rt_context *ctx,
    subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
    return subscript_rt_ctx_trap_kind(ctx) == 0u;
}

/* The trap accessors are the only failure channel: every exported entry
 * returns void. The position id is not printed here, because the
 * checkpoint the interrupt lands on is a property of the moment, not of
 * the program. */
static void hostReportTrap(const subscript_rt_context *ctx) {
    uint64_t length = 0u;
    const uint8_t *message = subscript_rt_ctx_trap_message(ctx, &length);
    printf("host:trap kind=%" PRIu32 " message=", subscript_rt_ctx_trap_kind(ctx));
    if (length != 0u) {
        fwrite(message, 1u, (size_t)length, stdout);
    }
    fputc('\n', stdout);
}

int main(void) {
#if defined(_WIN32)
    /* Exact golden bytes require binary stdout on Windows. */
    _setmode(_fileno(stdout), _O_BINARY);
#endif

    subscript_rt_context *ctx = subscript_rt_ctx_new();
    if (ctx == NULL) {
        return 2;
    }
    subscript_rt_ctx_set_print_observer(ctx, hostObserveScriptPrint, stdout);

    /* Both limits are set before the first script call, so the module
     * initializer already runs under them. */
    subscript_rt_ctx_set_alloc_quota(ctx, HOST_ALLOC_QUOTA);
    subscript_rt_ctx_set_stack_budget(ctx, HOST_STACK_BUDGET);
    printf(
        "host:limits quota=%" PRIu64 " stack-budget=%" PRIu64 "\n",
        HOST_ALLOC_QUOTA,
        HOST_STACK_BUDGET);

    if (!hostCallScript(ctx, subscript_init)) {
        hostReportTrap(ctx);
        subscript_rt_ctx_release(ctx);
        return 3;
    }

    /* The handle is obtained on this thread, before the run, and it is
     * valid until the Context is released. */
    const subscript_rt_interrupt *interrupt = subscript_rt_ctx_interrupt_handle(ctx);
#if defined(_WIN32)
    HANDLE interrupter;
#else
    pthread_t interrupter;
#endif
    if (!hostStartInterruptThread(interrupt, &interrupter)) {
        subscript_rt_ctx_release(ctx);
        return 2;
    }
    printf("host:interrupt armed after %ums\n", HOST_INTERRUPT_AFTER_MILLIS);

    /* The call is synchronous and the script never returns on its own.
     * It returns here because a checkpoint read the flag. */
    if (hostCallScript(ctx, subscript_export_tick)) {
        printf("host:tick returned with no trap\n");
        hostJoinInterruptThread(interrupter);
        subscript_rt_ctx_release(ctx);
        return 3;
    }
    hostJoinInterruptThread(interrupter);
    hostReportTrap(ctx);

    /* The Context survives its trap. Clearing the trap clears the
     * interrupt flag with it, so this Context can run script again. */
    (void)subscript_rt_ctx_clear_trap(ctx);
    printf(
        "host:cleared trap kind=%" PRIu32 "\n",
        subscript_rt_ctx_trap_kind(ctx));

    /* Memory under the profile (compiler.md section 109.8a). The profile
     * rejects Context.free, so collection is the one way memory returns.
     * Every live figure printed below is stable across runs, so the
     * golden pins each one exactly. */
    subscript_rt_ctx_set_alloc_quota(ctx, HOST_MEMORY_QUOTA);
    printf(
        "host:memory quota=%" PRIu64 " threshold=%" PRIu64 "\n",
        HOST_MEMORY_QUOTA,
        (uint64_t)HOST_MEMORY_THRESHOLD);

    /* Pattern 1 — the host paces. live_bytes is a counter, so this read
     * costs the same at every live count. The collect runs here, outside
     * the script call, at a moment the host picked. */
    unsigned hostCollects = 0u;
    for (unsigned frameIndex = 1u; frameIndex <= HOST_MEMORY_FRAMES; ++frameIndex) {
        if (!hostCallScript(ctx, subscript_export_frame)) {
            hostReportTrap(ctx);
            subscript_rt_ctx_release(ctx);
            return 3;
        }
        uint64_t live = subscript_rt_ctx_live_bytes(ctx);
        if (live > (uint64_t)HOST_MEMORY_THRESHOLD) {
            subscript_rt_ctx_collect(ctx);
            hostCollects += 1u;
            printf(
                "host:collect frame=%u live=%" PRIu64 " -> %" PRIu64 "\n",
                frameIndex,
                live,
                subscript_rt_ctx_live_bytes(ctx));
        }
    }
    printf(
        "host:phase-a frames=%u collects=%u live=%" PRIu64 "\n",
        HOST_MEMORY_FRAMES,
        hostCollects,
        subscript_rt_ctx_live_bytes(ctx));

    /* Pattern 2 — the script collects at its own boundary. One host
     * collect resets the live set to the window, and the host then
     * collects nothing for the whole phase. */
    subscript_rt_ctx_collect(ctx);
    for (unsigned frameIndex = 1u; frameIndex <= HOST_MEMORY_FRAMES; ++frameIndex) {
        if (!hostCallScript(ctx, subscript_export_frameAndCollect)) {
            hostReportTrap(ctx);
            subscript_rt_ctx_release(ctx);
            return 3;
        }
    }
    printf(
        "host:phase-b frames=%u collects=0 live=%" PRIu64 "\n",
        HOST_MEMORY_FRAMES,
        subscript_rt_ctx_live_bytes(ctx));

    subscript_rt_ctx_release(ctx);
    return 0;
}
