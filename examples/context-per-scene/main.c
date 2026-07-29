/*
 * main.c — two game scenes, each owning one fresh script subscript_rt_context.
 *
 * The engine's frame record belongs to this host thread and survives both
 * scenes. Script globals and allocations belong to one subscript_rt_context and do not.
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

void subscript_export_update(subscript_rt_context *ctx);
void subscript_export_finish(subscript_rt_context *ctx);

/* Every script entry is bracketed so the runtime can track script depth.
 * Its void return is not a success signal; the trap kind is. */
static bool hostCallScript(
    subscript_rt_context *ctx,
    subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
    return subscript_rt_ctx_trap_kind(ctx) == 0u;
}

/* The observer receives one script line without its trailing newline.
 * Using the same buffered stdout stream as host printf preserves ordering. */
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

/* A trap detaches script from the current scene. The host reports it through
 * the trap accessors, releases that scene's resources, and returns failure. */
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

static bool hostRunScene(uint32_t sceneNumber) {
    subscript_rt_context *ctx = subscript_rt_ctx_new();
    if (ctx == NULL) {
        return false;
    }
    EngWorld world = engWorldCreate(NULL);
    if (world == NULL) {
        subscript_rt_ctx_release(ctx);
        return false;
    }

    subscript_rt_ctx_set_print_observer(ctx, hostObserveScriptPrint, stdout);
    bool scriptAttached = hostCallScript(ctx, subscript_init);
    if (!scriptAttached) {
        hostReportTrap(ctx);
    }

    /* The host prints integer scene/frame indices. The adjacent script line
     * prints its f32 counter: it restarts while this thread-local index keeps
     * climbing across the subscript_rt_context boundary. */
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
        scriptAttached = hostCallScript(ctx, subscript_export_update);
        if (!scriptAttached) {
            hostReportTrap(ctx);
        }
    }

    if (scriptAttached) {
        scriptAttached = hostCallScript(ctx, subscript_export_finish);
        if (!scriptAttached) {
            hostReportTrap(ctx);
        }
    }

    /* live_bytes is sampled before release because release ends the subscript_rt_context.
     * Equal-sized scenes start from the same allocator floor rather than
     * inheriting the preceding scene's peak. */
    printf(
        "host:scene=%" PRIu32 " end live-bytes=%" PRIu64 "\n",
        sceneNumber,
        subscript_rt_ctx_live_bytes(ctx));

    engWorldRelease(world);
    subscript_rt_ctx_release(ctx);
    return scriptAttached;
}

int main(void) {
#if defined(_WIN32)
    /* Exact golden bytes require binary stdout on Windows. */
    _setmode(_fileno(stdout), _O_BINARY);
#endif

    /* subscript_rt_context release and subscript_rt_context.collect answer different questions.
     * Releasing ends one scene; collection reclaims unreachable allocations
     * while a subscript_rt_context lives. A fresh subscript_rt_context re-runs subscript_init, so state needed
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
