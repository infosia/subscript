/*
 * main.c — two game scenes, each owning one fresh script Context.
 *
 * The engine's frame record belongs to this host thread and survives both
 * scenes. Script globals and allocations belong to one Context and do not.
 */

#include "engine.h"
#include "subscript_runtime.h"

#include <inttypes.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#if defined(_WIN32)
#include <fcntl.h>
#include <io.h>
#endif

void ss_export_update(Context *ctx);
void ss_export_finish(Context *ctx);

/* Every script entry is bracketed so the runtime can track script depth.
 * Its void return is not a success signal; the trap kind is. */
static bool hostCallScript(
    Context *ctx,
    sub_script_main_entry entry) {
    sub_rt_ctx_enter_script(ctx);
    entry(ctx);
    sub_rt_ctx_exit_script(ctx);
    return sub_rt_ctx_trap_kind(ctx) == 0u;
}

/* The sink is cumulative within one Context. Each scene starts its own
 * drained offset because each scene starts a fresh sink. */
static void hostDrainScriptOutput(
    const Context *ctx,
    uint64_t *drained) {
    uint64_t length = 0u;
    const uint8_t *bytes = sub_rt_ctx_stdout(ctx, &length);
    if (length > *drained) {
        size_t available = (size_t)(length - *drained);
        fwrite(bytes + *drained, 1u, available, stdout);
        *drained = length;
    }
}

/* A trap detaches script from the current scene. The host reports it through
 * the trap accessors, releases that scene's resources, and returns failure. */
static void hostReportTrap(const Context *ctx) {
    uint64_t length = 0u;
    const uint8_t *message = sub_rt_ctx_trap_message(ctx, &length);
    fprintf(
        stderr,
        "script trap kind=%" PRIu32 " position=%" PRIu32 ": ",
        sub_rt_ctx_trap_kind(ctx),
        sub_rt_ctx_trap_pos_id(ctx));
    if (length != 0u) {
        fwrite(message, 1u, (size_t)length, stderr);
    }
    fputc('\n', stderr);
}

static bool hostRunScene(uint32_t sceneNumber) {
    Context *ctx = sub_rt_ctx_new();
    if (ctx == NULL) {
        return false;
    }
    EngWorld world = engWorldCreate(NULL);
    if (world == NULL) {
        sub_rt_ctx_release(ctx);
        return false;
    }

    uint64_t drained = 0u;
    bool scriptAttached = hostCallScript(ctx, ss_init);
    if (!scriptAttached) {
        hostReportTrap(ctx);
    }

    /* The host prints integer scene/frame indices. The adjacent script line
     * prints its f32 counter: it restarts while this thread-local index keeps
     * climbing across the Context boundary. */
    for (uint32_t sceneFrame = 1u;
         sceneFrame <= 2u && scriptAttached;
         sceneFrame += 1u) {
        engFrameBegin(world, 0.25f);
        printf(
            "host:scene=%" PRIu32 " scene-frame=%" PRIu32
            " engine-index=%" PRIu64 "\n",
            sceneNumber,
            sceneFrame,
            engFrameIndex());
        scriptAttached = hostCallScript(ctx, ss_export_update);
        hostDrainScriptOutput(ctx, &drained);
        if (!scriptAttached) {
            hostReportTrap(ctx);
        }
    }

    if (scriptAttached) {
        scriptAttached = hostCallScript(ctx, ss_export_finish);
        hostDrainScriptOutput(ctx, &drained);
        if (!scriptAttached) {
            hostReportTrap(ctx);
        }
    }

    /* live_bytes is sampled before release because release ends the Context.
     * Equal-sized scenes start from the same allocator floor rather than
     * inheriting the preceding scene's peak. */
    printf(
        "host:scene=%" PRIu32 " end live-bytes=%" PRIu64 "\n",
        sceneNumber,
        sub_rt_ctx_live_bytes(ctx));

    engWorldRelease(world);
    sub_rt_ctx_release(ctx);
    return scriptAttached;
}

int main(void) {
#if defined(_WIN32)
    /* Exact golden bytes require binary stdout on Windows. */
    _setmode(_fileno(stdout), _O_BINARY);
#endif

    /* Context release and Context.collect answer different questions.
     * Releasing ends one scene; collection reclaims unreachable allocations
     * while a Context lives. A fresh Context re-runs ss_init, so state needed
     * by the next scene must remain host-side, as engFrameIndex does here. */
    for (uint32_t sceneNumber = 1u;
         sceneNumber <= 2u;
         sceneNumber += 1u) {
        if (!hostRunScene(sceneNumber)) {
            return 3;
        }
    }
    return 0;
}
