/*
 * main.c — complete C host for the capstone script.
 *
 * The host owns main, the engine world, the frame loop, trap policy,
 * script-output draining, and subscript_rt_context lifetime. Script entry calls cross
 * only the generated C ABI.
 */

#include "engine.h"
#include "program.h"

#include <inttypes.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#if defined(_WIN32)
#include <fcntl.h>
#include <io.h>
#endif

/* The host brackets every entry so script_depth makes trap clearing safe;
 * this helper returns only the post-hoc trap state, never a script result. */
static bool hostCallScript(
    subscript_rt_context *ctx,
    subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
    return subscript_rt_ctx_trap_kind(ctx) == 0u;
}

/* The runtime sink is cumulative. Tracking the drained prefix preserves
 * ordering between host lines and script lines without giving print access
 * to the process stdout. */
static void hostDrainScriptOutput(
    const subscript_rt_context *ctx,
    uint64_t *drained) {
    uint64_t length = 0u;
    const uint8_t *bytes = subscript_rt_ctx_stdout(ctx, &length);
    if (length > *drained) {
        size_t available = (size_t)(length - *drained);
        fwrite(bytes + *drained, 1u, available, stdout);
        *drained = length;
    }
}

/* Trap accessors are the only reliable failure channel because every
 * exported entry returns void. This host chooses §18.1b's detach response:
 * it stops calling script after a trap, so damaged script state cannot
 * continue, while the independently owned host loop can finish cleanly. */
static void hostReportTrap(const subscript_rt_context *ctx) {
    uint64_t length = 0u;
    const uint8_t *message = subscript_rt_ctx_trap_message(ctx, &length);
    fprintf(
        stderr,
        "script trap kind=%" PRIu32 " position=%" PRIu32 ": ",
        subscript_rt_ctx_trap_kind(ctx),
        subscript_rt_ctx_trap_pos_id(ctx));
    if (length != 0u) {
        fwrite(message, 1u, (size_t)length, stderr);
    }
    fputc('\n', stderr);
}

/* Reading entity state lets the host report only integer observables:
 * count, flags, and layer. Fractional transform fields remain script output. */
static size_t hostReadEntity(
    EngineWorld world,
    EngineEntityState *state) {
    return engineWorldReadEntities(
        world,
        (EngineEntityStateOut){state, 1u});
}

int main(void) {
#if defined(_WIN32)
    /* The golden compares exact bytes, so Windows stdout must not translate
     * line endings while host and script output share the stream. */
    _setmode(_fileno(stdout), _O_BINARY);
#endif

    /* subscript_rt_context creation owns all script allocations; subscript_init establishes
     * module globals, and the matching release at the end frees the subscript_rt_context. */
    subscript_rt_context *ctx = subscript_rt_ctx_new();
    if (ctx == NULL) {
        return 2;
    }
    EngineWorld world = engineWorldCreate(NULL);
    if (world == NULL) {
        subscript_rt_ctx_release(ctx);
        return 2;
    }

    uint64_t drained = 0u;
    bool scriptAttached = hostCallScript(ctx, subscript_init);
    if (!scriptAttached) {
        hostReportTrap(ctx);
    }

    /* init uses the same frame-state path as update, so its zero-argument
     * entry sees an explicit world, fixed step, and first frame index. */
    if (scriptAttached) {
        engineFrameBegin(world, 0.25f);
        printf("host:init index=%" PRIu64 "\n", engineFrameIndex());
        scriptAttached = hostCallScript(ctx, subscript_export_init);
        hostDrainScriptOutput(ctx, &drained);
        if (!scriptAttached) {
            hostReportTrap(ctx);
        }
    }

    EngineEntityState state = {0};
    size_t entityCount = hostReadEntity(world, &state);
    printf(
        "host:state entities=%zu flags=%" PRIu64 " layer=%" PRIu16 "\n",
        entityCount,
        state.engineFlags,
        state.engineTransform.engineLayer);

    /* The host advances its own loop state first, records it in the facade,
     * calls update inside enter/exit, then pumps deferred engine work. */
    for (uint32_t hostFrame = 0u; hostFrame < 3u; hostFrame += 1u) {
        engineFrameBegin(world, 0.25f);
        printf(
            "host:frame=%" PRIu32 " index=%" PRIu64 "\n",
            hostFrame,
            engineFrameIndex());
        if (scriptAttached) {
            scriptAttached = hostCallScript(ctx, subscript_export_update);
            hostDrainScriptOutput(ctx, &drained);
            if (!scriptAttached) {
                hostReportTrap(ctx);
            }
        }
        engineWorldPump(world);
        /* Invariant 2 and §18.2d: the recommended release pattern. The host
         * collects once per frame, after the script calls and the pump and
         * outside the enter/exit bracket, so the script writes no release
         * code for per-frame garbage. The counts before and after the
         * collection make invariant 2 visible from the host: nothing is
         * reclaimed until the host asks, and the live count stays bounded. */
        uint64_t allocationsBefore = subscript_rt_ctx_live_allocations(ctx);
        subscript_rt_ctx_collect(ctx);
        printf(
            "host:frame=%" PRIu32 " allocations-before=%" PRIu64
            " allocations-after=%" PRIu64 "\n",
            hostFrame,
            allocationsBefore,
            subscript_rt_ctx_live_allocations(ctx));
        state = (EngineEntityState){0};
        entityCount = hostReadEntity(world, &state);
        printf(
            "host:state entities=%zu flags=%" PRIu64
            " layer=%" PRIu16 "\n",
            entityCount,
            state.engineFlags,
            state.engineTransform.engineLayer);
    }

    /* shutdown is the script's last entry; the host only drains its output. */
    if (scriptAttached) {
        scriptAttached = hostCallScript(ctx, subscript_export_shutdown);
        hostDrainScriptOutput(ctx, &drained);
        if (!scriptAttached) {
            hostReportTrap(ctx);
        }
    }

    /* The world is released on its loop thread, then the subscript_rt_context release
     * ends every remaining script allocation and completes host ownership. */
    engineWorldRelease(world);
    subscript_rt_ctx_release(ctx);
    return scriptAttached ? 0 : 3;
}
