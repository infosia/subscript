/*
 * engine.c — deterministic, headless implementation of the neutral facade.
 *
 * Each created handle owns one zero-initialized allocation with a fixed
 * entity capacity; allocation failure returns NULL and capacity overflow is
 * truncated or ignored as documented by the header. Frame access uses one
 * per-thread record, so independently driven host threads do not share
 * mutable engine state.
 */

#include "engine.h"

#include <stdlib.h>
#include <string.h>

/* MSVC needs its thread-storage spelling when the gate omits C11 mode. */
#if defined(_MSC_VER)
#define ENGINE_THREAD_LOCAL __declspec(thread)
#else
#define ENGINE_THREAD_LOCAL _Thread_local
#endif

/* The three runtime entries the host adapter of compiler.md 111 calls
 * below. The source of truth for each signature is the generated
 * runtime/include/subscript_runtime.h; this facade repeats the three it
 * needs, with the incomplete Context type, so it depends on no include
 * path. */
typedef struct subscript_rt_context subscript_rt_context;
extern subscript_rt_context *subscript_rt_cb_registration_context(void *registration);
extern int32_t subscript_rt_ctx_callback_release(
    subscript_rt_context *ctx,
    void *registration);
extern uint64_t subscript_rt_ctx_live_bytes(const subscript_rt_context *ctx);

enum {
    ENGINE_WORLD_ENTITY_CAPACITY = 32,
    ENGINE_WORLD_NAME_CAPACITY = 64,
    ENGINE_WORLD_OPTION_LIMIT = 32,
    /* The requests one world holds at the same time. More than one is
     * required, because a callback can start the next request while its
     * own call runs (compiler.md 111.2 one-shot). Two is also small
     * enough that a short program reaches the refusal. */
    ENGINE_WORLD_REQUEST_CAPACITY = 2,
    /* What engineRequestStart returns for a crossing it refuses. */
    ENGINE_REQUEST_REFUSED = -1
};

/* One pending request. The slot is free when engineRegistration is NULL.
 * A slot stays held through its own fire, so a callback that starts the
 * next request never lands in the slot that fires. */
typedef struct EngineRequestSlot {
    EngineEventCallback engineCallback;
    void *engineRegistration;
    bool enginePending;
} EngineRequestSlot;

/* The concrete opaque-world layout owns every per-world mutable byte:
 * lifecycle, simulation state, bounded entities, copied name bytes, and
 * deferred sink. */
struct EngineWorld_T {
    uint32_t engineReferenceCount;
    uint32_t engineTicksPerFrame;
    uint32_t engineSimulationTicks;
    uint64_t engineSimulationStepIndex;
    size_t engineEntityCapacity;
    size_t engineEntityCount;
    EngineEntityState engineEntities[ENGINE_WORLD_ENTITY_CAPACITY];
    char engineName[ENGINE_WORLD_NAME_CAPACITY];
    size_t engineNameLength;
    float engineSimulationStepValue;
    EngineEventSink engineEventSink;
    EngineEventKind enginePendingEvent;
    EngineEventKind engineLastEvent;
    bool engineEventPending;
    /* Host-adapter state of compiler.md 111. engineRequestContext is the
     * Context this world read at its last crossing (111 rule 5a); every
     * release goes through it. Every field belongs to one world, so this
     * adapter keeps no static state and one run leaves none for the
     * next. */
    subscript_rt_context *engineRequestContext;
    EngineRequestSlot engineRequests[ENGINE_WORLD_REQUEST_CAPACITY];
    int32_t engineRequestNumber;
    int32_t engineReleaseCount;
    uint64_t engineLiveBytesMark;
    bool engineLiveBytesMarked;
};

/* The frame record is separate from simulation state; thread-local storage
 * gives each host-loop thread its own current world, dt, and index. */
typedef struct EngineFrameRecord {
    EngineWorld engineWorld;
    float engineFixedStep;
    uint64_t engineFrameIndex;
} EngineFrameRecord;

/* Static thread-local initialization supplies the documented zeroed record
 * before the calling thread's first engineFrameBegin. */
static ENGINE_THREAD_LOCAL EngineFrameRecord engineFrameRecord;

/* Handle checking maps NULL to a no-op sentinel; a released non-NULL handle
 * is outside the C API contract and is not read for attempted validation. */
static struct EngineWorld_T *engineWorldChecked(EngineWorld engineWorld) {
    if (engineWorld == NULL) {
        return NULL;
    }
    return engineWorld;
}

/* A transform copy assigns every semantic field rather than copying caller
 * padding; destination padding therefore remains the deterministic zero
 * written when its enclosing state was cleared. */
static void engineTransformCopy(
    EngineTransform *engineDestination,
    const EngineTransform *engineSource) {
    engineDestination->engineInheritScale = engineSource->engineInheritScale;
    engineDestination->engineX = engineSource->engineX;
    engineDestination->engineY = engineSource->engineY;
    engineDestination->engineRotation = engineSource->engineRotation;
    engineDestination->engineLayer = engineSource->engineLayer;
}

/* An entity-state copy composes the field-wise transform copy with exact
 * integer assignments, so neither source padding nor evaluation order
 * affects stored or returned bytes. */
static void engineEntityStateCopy(
    EngineEntityState *engineDestination,
    const EngineEntityState *engineSource) {
    engineDestination->engineId = engineSource->engineId;
    engineTransformCopy(
        &engineDestination->engineTransform,
        &engineSource->engineTransform);
    engineDestination->engineFlags = engineSource->engineFlags;
}

/* Option traversal consumes at most the bounded node count in pointer order.
 * Reaching the limit ignores the remainder, unknown tags are ignored, and
 * entity capacity is clamped to fixed storage. */
static void engineWorldApplyOptions(
    struct EngineWorld_T *engineWorld,
    const EngineWorldOption *engineOptions) {
    const EngineWorldOption *engineOption = engineOptions;
    uint32_t engineOptionCount = 0u;
    while (engineOption != NULL) {
        if (engineOptionCount == ENGINE_WORLD_OPTION_LIMIT) {
            break;
        }
        engineOptionCount += 1u;
        switch (engineOption->engineKind) {
            case ENGINE_WORLD_OPTION_TICK: {
                const EngineTickOption *engineTickOption =
                    (const EngineTickOption *)(const void *)engineOption;
                engineWorld->engineTicksPerFrame =
                    engineTickOption->engineTicksPerFrame;
                break;
            }
            case ENGINE_WORLD_OPTION_ENTITY_LIMIT: {
                const EngineEntityLimitOption *engineLimitOption =
                    (const EngineEntityLimitOption *)(const void *)engineOption;
                size_t engineMaximumEntities =
                    (size_t)engineLimitOption->engineMaximumEntities;
                if (engineMaximumEntities > ENGINE_WORLD_ENTITY_CAPACITY) {
                    engineMaximumEntities = ENGINE_WORLD_ENTITY_CAPACITY;
                }
                engineWorld->engineEntityCapacity = engineMaximumEntities;
                break;
            }
            default:
                break;
        }
        engineOption = engineOption->engineNext;
    }
}

/* Creation allocates one independent zeroed world, applies the optional
 * intrusive chain, and returns one opaque-handle reference or NULL. */
EngineWorld engineWorldCreate(const EngineWorldOption *engineOptions) {
    struct EngineWorld_T *engineWorld =
        (struct EngineWorld_T *)calloc(1u, sizeof(*engineWorld));
    if (engineWorld == NULL) {
        return NULL;
    }
    engineWorld->engineReferenceCount = 1u;
    engineWorld->engineTicksPerFrame = 1u;
    engineWorld->engineEntityCapacity = ENGINE_WORLD_ENTITY_CAPACITY;
    engineWorld->engineEventSink.engineCallback = NULL;
    engineWorld->engineEventSink.engineUserdata1 = NULL;
    engineWorld->engineEventSink.engineUserdata2 = NULL;
    engineWorldApplyOptions(engineWorld, engineOptions);
    return engineWorld;
}

/* Retain advances the opaque handle's checked unsigned reference count;
 * NULL and saturated handles leave state unchanged. */
void engineWorldRetain(EngineWorld engineWorld) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    if (engineCheckedWorld == NULL ||
        engineCheckedWorld->engineReferenceCount == UINT32_MAX) {
        return;
    }
    engineCheckedWorld->engineReferenceCount += 1u;
}

/* Release frees the per-handle allocation after its last reference; a NULL
 * handle leaves state unchanged. */
void engineWorldRelease(EngineWorld engineWorld) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    if (engineCheckedWorld == NULL) {
        return;
    }
    engineCheckedWorld->engineReferenceCount -= 1u;
    if (engineCheckedWorld->engineReferenceCount == 0u) {
        if (engineFrameRecord.engineWorld == engineWorld) {
            engineFrameRecord.engineWorld = NULL;
            engineFrameRecord.engineFixedStep = 0.0f;
        }
        free(engineCheckedWorld);
    }
}

/* Name storage copies the length-carrying view without reading or adding a
 * terminator; missing data is a no-op and the fixed bound truncates. */
void engineWorldSetName(EngineWorld engineWorld, EngineStringView engineName) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    if (engineCheckedWorld == NULL ||
        (engineName.engineLen != 0u && engineName.engineData == NULL)) {
        return;
    }
    size_t engineStoredLength = engineName.engineLen;
    if (engineStoredLength > ENGINE_WORLD_NAME_CAPACITY) {
        engineStoredLength = ENGINE_WORLD_NAME_CAPACITY;
    }
    memset(
        engineCheckedWorld->engineName,
        0,
        sizeof(engineCheckedWorld->engineName));
    if (engineStoredLength != 0u) {
        memcpy(
            engineCheckedWorld->engineName,
            engineName.engineData,
            engineStoredLength);
    }
    engineCheckedWorld->engineNameLength = engineStoredLength;
}

/* The by-value transform updates an existing entity or creates one within
 * the configured bound; semantic fields are copied and padding stays zero. */
void engineWorldSetTransform(
    EngineWorld engineWorld,
    uint32_t engineEntityId,
    EngineTransform engineTransform) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    if (engineCheckedWorld == NULL) {
        return;
    }
    EngineEntityState *engineState = NULL;
    for (size_t engineIndex = 0u;
         engineIndex < engineCheckedWorld->engineEntityCount;
         engineIndex += 1u) {
        if (engineCheckedWorld->engineEntities[engineIndex].engineId == engineEntityId) {
            engineState = &engineCheckedWorld->engineEntities[engineIndex];
            break;
        }
    }
    if (engineState == NULL) {
        if (engineCheckedWorld->engineEntityCount ==
            engineCheckedWorld->engineEntityCapacity) {
            return;
        }
        engineState =
            &engineCheckedWorld->engineEntities[engineCheckedWorld->engineEntityCount];
        memset(engineState, 0, sizeof(*engineState));
        engineState->engineId = engineEntityId;
        engineState->engineFlags = ENGINE_ENTITY_FLAG_NONE;
        engineCheckedWorld->engineEntityCount += 1u;
    }
    engineTransformCopy(&engineState->engineTransform, &engineTransform);
    engineCheckedWorld->enginePendingEvent = ENGINE_EVENT_ENTITY_CHANGED;
    engineCheckedWorld->engineEventPending = true;
}

/* Const-slice replacement reads every caller-owned entity in increasing
 * index order and stores only semantic fields into zeroed fixed storage. */
void engineWorldReplaceEntities(
    EngineWorld engineWorld,
    EngineEntityStateView engineStates) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    if (engineCheckedWorld == NULL ||
        (engineStates.engineCount != 0u && engineStates.engineItems == NULL)) {
        return;
    }
    size_t engineStoredCount = engineStates.engineCount;
    if (engineStoredCount > engineCheckedWorld->engineEntityCapacity) {
        engineStoredCount = engineCheckedWorld->engineEntityCapacity;
    }
    memset(
        engineCheckedWorld->engineEntities,
        0,
        sizeof(engineCheckedWorld->engineEntities));
    for (size_t engineIndex = 0u;
         engineIndex < engineStoredCount;
         engineIndex += 1u) {
        engineEntityStateCopy(
            &engineCheckedWorld->engineEntities[engineIndex],
            &engineStates.engineItems[engineIndex]);
    }
    engineCheckedWorld->engineEntityCount = engineStoredCount;
    engineCheckedWorld->enginePendingEvent = ENGINE_EVENT_ENTITY_CHANGED;
    engineCheckedWorld->engineEventPending = true;
}

/* Mutable out-array access writes the caller's own storage in increasing
 * index order; clearing each destination first makes padding deterministic. */
size_t engineWorldReadEntities(
    EngineWorld engineWorld,
    EngineEntityStateOut engineStates) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    if (engineCheckedWorld == NULL ||
        (engineStates.engineCount != 0u && engineStates.engineItems == NULL)) {
        return 0u;
    }
    size_t engineWritten = engineStates.engineCount;
    if (engineWritten > engineCheckedWorld->engineEntityCount) {
        engineWritten = engineCheckedWorld->engineEntityCount;
    }
    for (size_t engineIndex = 0u;
         engineIndex < engineWritten;
         engineIndex += 1u) {
        memset(
            &engineStates.engineItems[engineIndex],
            0,
            sizeof(engineStates.engineItems[engineIndex]));
        engineEntityStateCopy(
            &engineStates.engineItems[engineIndex],
            &engineCheckedWorld->engineEntities[engineIndex]);
    }
    return engineWritten;
}

/* The embedded count/pointer batch reads ids in caller order and applies
 * flag bits with `|`; all matching counts advance without signed overflow. */
size_t engineWorldApplyFlags(
    EngineWorld engineWorld,
    EngineEntityBatch engineBatch) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    if (engineCheckedWorld == NULL ||
        (engineBatch.engineEntityIdsCount != 0u &&
         engineBatch.engineEntityIds == NULL)) {
        return 0u;
    }
    size_t engineMatched = 0u;
    for (size_t engineIdIndex = 0u;
         engineIdIndex < engineBatch.engineEntityIdsCount;
         engineIdIndex += 1u) {
        for (size_t engineStateIndex = 0u;
             engineStateIndex < engineCheckedWorld->engineEntityCount;
             engineStateIndex += 1u) {
            EngineEntityState *engineState =
                &engineCheckedWorld->engineEntities[engineStateIndex];
            if (engineState->engineId == engineBatch.engineEntityIds[engineIdIndex]) {
                engineState->engineFlags =
                    engineState->engineFlags | engineBatch.engineFlags;
                engineMatched += 1u;
                break;
            }
        }
    }
    if (engineMatched != 0u) {
        engineCheckedWorld->enginePendingEvent = ENGINE_EVENT_ENTITY_CHANGED;
        engineCheckedWorld->engineEventPending = true;
    }
    return engineMatched;
}

/* Registration stores the callback and both userdata slots, queues a ready
 * event, and returns without invoking the callback. */
void engineWorldSetEventSink(
    EngineWorld engineWorld,
    EngineEventSink engineSink) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    if (engineCheckedWorld == NULL) {
        return;
    }
    engineCheckedWorld->engineEventSink.engineCallback = engineSink.engineCallback;
    engineCheckedWorld->engineEventSink.engineUserdata1 = engineSink.engineUserdata1;
    engineCheckedWorld->engineEventSink.engineUserdata2 = engineSink.engineUserdata2;
    engineCheckedWorld->enginePendingEvent = ENGINE_EVENT_WORLD_READY;
    engineCheckedWorld->engineEventPending = engineSink.engineCallback != NULL;
}

/* The host-driven pump clears the pending marker before invoking the stored
 * sink; a callback may therefore queue later work without losing it. */
void engineWorldPump(EngineWorld engineWorld) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    if (engineCheckedWorld == NULL ||
        !engineCheckedWorld->engineEventPending ||
        engineCheckedWorld->engineEventSink.engineCallback == NULL) {
        return;
    }
    EngineEventCallback engineCallback =
        engineCheckedWorld->engineEventSink.engineCallback;
    void *engineUserdata1 =
        engineCheckedWorld->engineEventSink.engineUserdata1;
    void *engineUserdata2 =
        engineCheckedWorld->engineEventSink.engineUserdata2;
    EngineStringView engineMessage;
    engineMessage.engineData = engineCheckedWorld->engineName;
    engineMessage.engineLen = engineCheckedWorld->engineNameLength;
    engineCheckedWorld->engineLastEvent =
        engineCheckedWorld->enginePendingEvent;
    engineCheckedWorld->engineEventPending = false;
    engineCallback(engineMessage, engineUserdata1, engineUserdata2);
}

/* The last delivered event is per-world state set immediately before the
 * deferred callback fires; NULL maps to the enum's zero member. */
EngineEventKind engineWorldLastEvent(EngineWorld engineWorld) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    if (engineCheckedWorld == NULL) {
        return ENGINE_EVENT_WORLD_READY;
    }
    return engineCheckedWorld->engineLastEvent;
}

/* A simulation step records its input without floating accumulation,
 * advances per-world counters in unsigned arithmetic, and queues an event. */
void engineWorldStep(EngineWorld engineWorld, float engineFixedStep) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    if (engineCheckedWorld == NULL) {
        return;
    }
    engineCheckedWorld->engineSimulationStepValue = engineFixedStep;
    engineCheckedWorld->engineSimulationStepIndex += UINT64_C(1);
    engineCheckedWorld->engineSimulationTicks +=
        engineCheckedWorld->engineTicksPerFrame;
    engineCheckedWorld->enginePendingEvent = ENGINE_EVENT_FRAME_STEPPED;
    engineCheckedWorld->engineEventPending = true;
}

/* Frame begin updates only the calling thread's record; it does not mutate
 * the world or perform a simulation step. */
void engineFrameBegin(EngineWorld engineWorld, float engineFixedStep) {
    engineFrameRecord.engineWorld = engineWorldChecked(engineWorld);
    engineFrameRecord.engineFixedStep = engineFixedStep;
    engineFrameRecord.engineFrameIndex += UINT64_C(1);
}

/* The current frame world is the calling thread's explicit record; static
 * thread-local initialization supplies NULL before the first begin. */
EngineWorld engineFrameWorld(void) {
    return engineFrameRecord.engineWorld;
}

/* The current fixed step is exactly the calling thread's begin value, with
 * no clock read or floating accumulation. */
float engineFrameFixedStep(void) {
    return engineFrameRecord.engineFixedStep;
}

/* The current frame index is the calling thread's unsigned counter; no
 * process-global frame state exists. */
uint64_t engineFrameIndex(void) {
    return engineFrameRecord.engineFrameIndex;
}

/*
 * The host adapter of compiler.md 111.2. The adapter knows when the
 * native side can no longer fire; the compiler derives that fact from no
 * name and from no mode value. Each rule is stated where it applies.
 */

/* Ends one registration one time (111 rule 5) and counts it on the world
 * that holds it. Every release of this adapter goes through here. The
 * caller drops its own reference to the registration first. */
static void engineRequestRelease(
    struct EngineWorld_T *engineWorld,
    subscript_rt_context *engineContext,
    void *engineRegistration) {
    if (engineContext == NULL || engineRegistration == NULL) {
        return;
    }
    if (subscript_rt_ctx_callback_release(engineContext, engineRegistration) != 1) {
        return;
    }
    if (engineWorld != NULL) {
        engineWorld->engineReleaseCount += 1;
    }
}

/* The free request slot of the lowest index, or NULL when the world
 * holds its bound of pending requests. */
static EngineRequestSlot *engineRequestFreeSlot(struct EngineWorld_T *engineWorld) {
    for (size_t engineIndex = 0u;
         engineIndex < (size_t)ENGINE_WORLD_REQUEST_CAPACITY;
         engineIndex += 1u) {
        if (engineWorld->engineRequests[engineIndex].engineRegistration == NULL) {
            return &engineWorld->engineRequests[engineIndex];
        }
    }
    return NULL;
}

/* Completes one request: it fires the callback one time, and then ends
 * the registration, which is the point where no later call can occur
 * (111.2 one-shot). The slot is held through the fire and freed here. */
static void engineRequestComplete(
    struct EngineWorld_T *engineWorld,
    EngineRequestSlot *engineSlot) {
    EngineEventCallback engineCallback = engineSlot->engineCallback;
    void *engineRegistration = engineSlot->engineRegistration;
    EngineStringView engineMessage;
    engineMessage.engineData = engineWorld->engineName;
    engineMessage.engineLen = engineWorld->engineNameLength;
    engineSlot->enginePending = false;
    if (engineCallback != NULL) {
        /* 111 rule 4: the registration goes back in the first userdata
         * slot, and the second slot carries the null the marshaling
         * wrote. */
        engineCallback(engineMessage, engineRegistration, NULL);
    }
    engineSlot->engineCallback = NULL;
    engineSlot->engineRegistration = NULL;
    engineRequestRelease(
        engineWorld,
        engineWorld->engineRequestContext,
        engineRegistration);
}

/* A start crossing creates one registration (111 rule 3). The adapter
 * either stores it for exactly one completion, or ends it at once. */
int32_t engineRequestStart(
    EngineWorld engineWorld,
    bool engineImmediate,
    EngineRequestInfo engineInfo) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    /* 111 rule 5a: a function the script calls receives no Context. The
     * registration answers, and it is certainly live at this crossing. */
    subscript_rt_context *engineContext =
        subscript_rt_cb_registration_context(engineInfo.engineUserdata1);
    EngineRequestSlot *engineSlot;
    int32_t engineNumber;
    if (engineCheckedWorld == NULL) {
        /* A refused crossing still received a registration, so it ends
         * here and no later call can occur through it. */
        engineRequestRelease(NULL, engineContext, engineInfo.engineUserdata1);
        return ENGINE_REQUEST_REFUSED;
    }
    if (engineContext != NULL) {
        engineCheckedWorld->engineRequestContext = engineContext;
    }
    engineSlot = engineRequestFreeSlot(engineCheckedWorld);
    if (engineSlot == NULL) {
        engineRequestRelease(
            engineCheckedWorld,
            engineCheckedWorld->engineRequestContext,
            engineInfo.engineUserdata1);
        return ENGINE_REQUEST_REFUSED;
    }
    engineSlot->engineCallback = engineInfo.engineCallback;
    engineSlot->engineRegistration = engineInfo.engineUserdata1;
    engineSlot->enginePending = true;
    engineCheckedWorld->engineRequestNumber += 1;
    engineNumber = engineCheckedWorld->engineRequestNumber;
    if (engineImmediate) {
        /* The start completes before it returns, through the release
         * path the pump uses (111.2 one-shot). */
        engineRequestComplete(engineCheckedWorld, engineSlot);
    }
    return engineNumber;
}

/* The drain reads which requests are pending before it fires any of
 * them, so a request that a callback starts during the drain stays
 * pending for the next pump. */
void engineRequestPump(EngineWorld engineWorld) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    bool engineDue[ENGINE_WORLD_REQUEST_CAPACITY];
    size_t engineIndex;
    if (engineCheckedWorld == NULL) {
        return;
    }
    for (engineIndex = 0u;
         engineIndex < (size_t)ENGINE_WORLD_REQUEST_CAPACITY;
         engineIndex += 1u) {
        engineDue[engineIndex] =
            engineCheckedWorld->engineRequests[engineIndex].enginePending;
    }
    for (engineIndex = 0u;
         engineIndex < (size_t)ENGINE_WORLD_REQUEST_CAPACITY;
         engineIndex += 1u) {
        if (engineDue[engineIndex]) {
            engineRequestComplete(
                engineCheckedWorld,
                &engineCheckedWorld->engineRequests[engineIndex]);
        }
    }
}

/* The release count advances only when the runtime answered 1, so it
 * counts the registrations this world ended (111 rule 5). */
int32_t engineRequestReleaseCount(EngineWorld engineWorld) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    if (engineCheckedWorld == NULL) {
        return 0;
    }
    return engineCheckedWorld->engineReleaseCount;
}

/* The mark reads the Context live bytes through the kept Context, so
 * the script observes reclamation without new language surface. */
void engineRequestMarkLiveBytes(EngineWorld engineWorld) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    if (engineCheckedWorld == NULL ||
        engineCheckedWorld->engineRequestContext == NULL) {
        return;
    }
    engineCheckedWorld->engineLiveBytesMark =
        subscript_rt_ctx_live_bytes(engineCheckedWorld->engineRequestContext);
    engineCheckedWorld->engineLiveBytesMarked = true;
}

/* The answer is a comparison against the mark, not a byte count: the
 * live bytes are tier- and memory-mode-dependent, and the fall is not. */
int32_t engineRequestLiveBytesFellBy(
    EngineWorld engineWorld,
    uint32_t engineAtLeast) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    uint64_t engineNow;
    if (engineCheckedWorld == NULL || !engineCheckedWorld->engineLiveBytesMarked) {
        return 0;
    }
    engineNow =
        subscript_rt_ctx_live_bytes(engineCheckedWorld->engineRequestContext);
    if (engineNow > engineCheckedWorld->engineLiveBytesMark) {
        return 0;
    }
    return (engineCheckedWorld->engineLiveBytesMark - engineNow) >=
                   (uint64_t)engineAtLeast
               ? 1
               : 0;
}
