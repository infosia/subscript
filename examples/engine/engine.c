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
#define ENG_THREAD_LOCAL __declspec(thread)
#else
#define ENG_THREAD_LOCAL _Thread_local
#endif

enum {
    ENG_WORLD_ENTITY_CAPACITY = 32,
    ENG_WORLD_NAME_CAPACITY = 64,
    ENG_WORLD_OPTION_LIMIT = 32
};

/* The concrete opaque-world layout owns every per-world mutable byte:
 * lifecycle, simulation state, bounded entities, copied name bytes, and
 * deferred sink. */
struct EngWorld_T {
    uint32_t engReferenceCount;
    uint32_t engTicksPerFrame;
    uint32_t engSimulationTicks;
    uint64_t engSimulationStepIndex;
    size_t engEntityCapacity;
    size_t engEntityCount;
    EngEntityState engEntities[ENG_WORLD_ENTITY_CAPACITY];
    char engName[ENG_WORLD_NAME_CAPACITY];
    size_t engNameLength;
    float engSimulationStepValue;
    EngEventSink engEventSink;
    EngEventKind engPendingEvent;
    EngEventKind engLastEvent;
    bool engEventPending;
};

/* The frame record is separate from simulation state; thread-local storage
 * gives each host-loop thread its own current world, dt, and index. */
typedef struct EngFrameRecord {
    EngWorld engWorld;
    float engFixedStep;
    uint64_t engFrameIndex;
} EngFrameRecord;

/* Static thread-local initialization supplies the documented zeroed record
 * before the calling thread's first engFrameBegin. */
static ENG_THREAD_LOCAL EngFrameRecord engFrameRecord;

/* Handle checking maps NULL to a no-op sentinel; a released non-NULL handle
 * is outside the C API contract and is not read for attempted validation. */
static struct EngWorld_T *engWorldChecked(EngWorld engWorld) {
    if (engWorld == NULL) {
        return NULL;
    }
    return engWorld;
}

/* A transform copy assigns every semantic field rather than copying caller
 * padding; destination padding therefore remains the deterministic zero
 * written when its enclosing state was cleared. */
static void engTransformCopy(
    EngTransform *engDestination,
    const EngTransform *engSource) {
    engDestination->engInheritScale = engSource->engInheritScale;
    engDestination->engX = engSource->engX;
    engDestination->engY = engSource->engY;
    engDestination->engRotation = engSource->engRotation;
    engDestination->engLayer = engSource->engLayer;
}

/* An entity-state copy composes the field-wise transform copy with exact
 * integer assignments, so neither source padding nor evaluation order
 * affects stored or returned bytes. */
static void engEntityStateCopy(
    EngEntityState *engDestination,
    const EngEntityState *engSource) {
    engDestination->engId = engSource->engId;
    engTransformCopy(
        &engDestination->engTransform,
        &engSource->engTransform);
    engDestination->engFlags = engSource->engFlags;
}

/* Option traversal consumes at most the bounded node count in pointer order.
 * Reaching the limit ignores the remainder, unknown tags are ignored, and
 * entity capacity is clamped to fixed storage. */
static void engWorldApplyOptions(
    struct EngWorld_T *engWorld,
    const EngWorldOption *engOptions) {
    const EngWorldOption *engOption = engOptions;
    uint32_t engOptionCount = 0u;
    while (engOption != NULL) {
        if (engOptionCount == ENG_WORLD_OPTION_LIMIT) {
            break;
        }
        engOptionCount += 1u;
        switch (engOption->engKind) {
            case ENG_WORLD_OPTION_TICK: {
                const EngTickOption *engTickOption =
                    (const EngTickOption *)(const void *)engOption;
                engWorld->engTicksPerFrame =
                    engTickOption->engTicksPerFrame;
                break;
            }
            case ENG_WORLD_OPTION_ENTITY_LIMIT: {
                const EngEntityLimitOption *engLimitOption =
                    (const EngEntityLimitOption *)(const void *)engOption;
                size_t engMaximumEntities =
                    (size_t)engLimitOption->engMaximumEntities;
                if (engMaximumEntities > ENG_WORLD_ENTITY_CAPACITY) {
                    engMaximumEntities = ENG_WORLD_ENTITY_CAPACITY;
                }
                engWorld->engEntityCapacity = engMaximumEntities;
                break;
            }
            default:
                break;
        }
        engOption = engOption->engNext;
    }
}

/* Creation allocates one independent zeroed world, applies the optional
 * intrusive chain, and returns one opaque-handle reference or NULL. */
EngWorld engWorldCreate(const EngWorldOption *engOptions) {
    struct EngWorld_T *engWorld =
        (struct EngWorld_T *)calloc(1u, sizeof(*engWorld));
    if (engWorld == NULL) {
        return NULL;
    }
    engWorld->engReferenceCount = 1u;
    engWorld->engTicksPerFrame = 1u;
    engWorld->engEntityCapacity = ENG_WORLD_ENTITY_CAPACITY;
    engWorld->engEventSink.engCallback = NULL;
    engWorld->engEventSink.engUserdata1 = NULL;
    engWorld->engEventSink.engUserdata2 = NULL;
    engWorldApplyOptions(engWorld, engOptions);
    return engWorld;
}

/* Retain advances the opaque handle's checked unsigned reference count;
 * NULL and saturated handles leave state unchanged. */
void engWorldRetain(EngWorld engWorld) {
    struct EngWorld_T *engCheckedWorld = engWorldChecked(engWorld);
    if (engCheckedWorld == NULL ||
        engCheckedWorld->engReferenceCount == UINT32_MAX) {
        return;
    }
    engCheckedWorld->engReferenceCount += 1u;
}

/* Release frees the per-handle allocation after its last reference; a NULL
 * handle leaves state unchanged. */
void engWorldRelease(EngWorld engWorld) {
    struct EngWorld_T *engCheckedWorld = engWorldChecked(engWorld);
    if (engCheckedWorld == NULL) {
        return;
    }
    engCheckedWorld->engReferenceCount -= 1u;
    if (engCheckedWorld->engReferenceCount == 0u) {
        if (engFrameRecord.engWorld == engWorld) {
            engFrameRecord.engWorld = NULL;
            engFrameRecord.engFixedStep = 0.0f;
        }
        free(engCheckedWorld);
    }
}

/* Name storage copies the length-carrying view without reading or adding a
 * terminator; missing data is a no-op and the fixed bound truncates. */
void engWorldSetName(EngWorld engWorld, EngStringView engName) {
    struct EngWorld_T *engCheckedWorld = engWorldChecked(engWorld);
    if (engCheckedWorld == NULL ||
        (engName.engLen != 0u && engName.engData == NULL)) {
        return;
    }
    size_t engStoredLength = engName.engLen;
    if (engStoredLength > ENG_WORLD_NAME_CAPACITY) {
        engStoredLength = ENG_WORLD_NAME_CAPACITY;
    }
    memset(
        engCheckedWorld->engName,
        0,
        sizeof(engCheckedWorld->engName));
    if (engStoredLength != 0u) {
        memcpy(
            engCheckedWorld->engName,
            engName.engData,
            engStoredLength);
    }
    engCheckedWorld->engNameLength = engStoredLength;
}

/* The by-value transform updates an existing entity or creates one within
 * the configured bound; semantic fields are copied and padding stays zero. */
void engWorldSetTransform(
    EngWorld engWorld,
    uint32_t engEntityId,
    EngTransform engTransform) {
    struct EngWorld_T *engCheckedWorld = engWorldChecked(engWorld);
    if (engCheckedWorld == NULL) {
        return;
    }
    EngEntityState *engState = NULL;
    for (size_t engIndex = 0u;
         engIndex < engCheckedWorld->engEntityCount;
         engIndex += 1u) {
        if (engCheckedWorld->engEntities[engIndex].engId == engEntityId) {
            engState = &engCheckedWorld->engEntities[engIndex];
            break;
        }
    }
    if (engState == NULL) {
        if (engCheckedWorld->engEntityCount ==
            engCheckedWorld->engEntityCapacity) {
            return;
        }
        engState =
            &engCheckedWorld->engEntities[engCheckedWorld->engEntityCount];
        memset(engState, 0, sizeof(*engState));
        engState->engId = engEntityId;
        engState->engFlags = ENG_ENTITY_FLAG_NONE;
        engCheckedWorld->engEntityCount += 1u;
    }
    engTransformCopy(&engState->engTransform, &engTransform);
    engCheckedWorld->engPendingEvent = ENG_EVENT_ENTITY_CHANGED;
    engCheckedWorld->engEventPending = true;
}

/* Const-slice replacement reads every caller-owned entity in increasing
 * index order and stores only semantic fields into zeroed fixed storage. */
void engWorldReplaceEntities(
    EngWorld engWorld,
    EngEntityStateView engStates) {
    struct EngWorld_T *engCheckedWorld = engWorldChecked(engWorld);
    if (engCheckedWorld == NULL ||
        (engStates.engCount != 0u && engStates.engItems == NULL)) {
        return;
    }
    size_t engStoredCount = engStates.engCount;
    if (engStoredCount > engCheckedWorld->engEntityCapacity) {
        engStoredCount = engCheckedWorld->engEntityCapacity;
    }
    memset(
        engCheckedWorld->engEntities,
        0,
        sizeof(engCheckedWorld->engEntities));
    for (size_t engIndex = 0u;
         engIndex < engStoredCount;
         engIndex += 1u) {
        engEntityStateCopy(
            &engCheckedWorld->engEntities[engIndex],
            &engStates.engItems[engIndex]);
    }
    engCheckedWorld->engEntityCount = engStoredCount;
    engCheckedWorld->engPendingEvent = ENG_EVENT_ENTITY_CHANGED;
    engCheckedWorld->engEventPending = true;
}

/* Mutable out-array access writes the caller's own storage in increasing
 * index order; clearing each destination first makes padding deterministic. */
size_t engWorldReadEntities(
    EngWorld engWorld,
    EngEntityStateOut engStates) {
    struct EngWorld_T *engCheckedWorld = engWorldChecked(engWorld);
    if (engCheckedWorld == NULL ||
        (engStates.engCount != 0u && engStates.engItems == NULL)) {
        return 0u;
    }
    size_t engWritten = engStates.engCount;
    if (engWritten > engCheckedWorld->engEntityCount) {
        engWritten = engCheckedWorld->engEntityCount;
    }
    for (size_t engIndex = 0u;
         engIndex < engWritten;
         engIndex += 1u) {
        memset(
            &engStates.engItems[engIndex],
            0,
            sizeof(engStates.engItems[engIndex]));
        engEntityStateCopy(
            &engStates.engItems[engIndex],
            &engCheckedWorld->engEntities[engIndex]);
    }
    return engWritten;
}

/* The embedded count/pointer batch reads ids in caller order and applies
 * flag bits with `|`; all matching counts advance without signed overflow. */
size_t engWorldApplyFlags(
    EngWorld engWorld,
    EngEntityBatch engBatch) {
    struct EngWorld_T *engCheckedWorld = engWorldChecked(engWorld);
    if (engCheckedWorld == NULL ||
        (engBatch.engEntityIdsCount != 0u &&
         engBatch.engEntityIds == NULL)) {
        return 0u;
    }
    size_t engMatched = 0u;
    for (size_t engIdIndex = 0u;
         engIdIndex < engBatch.engEntityIdsCount;
         engIdIndex += 1u) {
        for (size_t engStateIndex = 0u;
             engStateIndex < engCheckedWorld->engEntityCount;
             engStateIndex += 1u) {
            EngEntityState *engState =
                &engCheckedWorld->engEntities[engStateIndex];
            if (engState->engId == engBatch.engEntityIds[engIdIndex]) {
                engState->engFlags =
                    engState->engFlags | engBatch.engFlags;
                engMatched += 1u;
                break;
            }
        }
    }
    if (engMatched != 0u) {
        engCheckedWorld->engPendingEvent = ENG_EVENT_ENTITY_CHANGED;
        engCheckedWorld->engEventPending = true;
    }
    return engMatched;
}

/* Registration stores the callback and both userdata slots, queues a ready
 * event, and returns without invoking the callback. */
void engWorldSetEventSink(
    EngWorld engWorld,
    EngEventSink engSink) {
    struct EngWorld_T *engCheckedWorld = engWorldChecked(engWorld);
    if (engCheckedWorld == NULL) {
        return;
    }
    engCheckedWorld->engEventSink.engCallback = engSink.engCallback;
    engCheckedWorld->engEventSink.engUserdata1 = engSink.engUserdata1;
    engCheckedWorld->engEventSink.engUserdata2 = engSink.engUserdata2;
    engCheckedWorld->engPendingEvent = ENG_EVENT_WORLD_READY;
    engCheckedWorld->engEventPending = engSink.engCallback != NULL;
}

/* The host-driven pump clears the pending marker before invoking the stored
 * sink; a callback may therefore queue later work without losing it. */
void engWorldPump(EngWorld engWorld) {
    struct EngWorld_T *engCheckedWorld = engWorldChecked(engWorld);
    if (engCheckedWorld == NULL ||
        !engCheckedWorld->engEventPending ||
        engCheckedWorld->engEventSink.engCallback == NULL) {
        return;
    }
    EngEventCallback engCallback =
        engCheckedWorld->engEventSink.engCallback;
    void *engUserdata1 =
        engCheckedWorld->engEventSink.engUserdata1;
    void *engUserdata2 =
        engCheckedWorld->engEventSink.engUserdata2;
    EngStringView engMessage;
    engMessage.engData = engCheckedWorld->engName;
    engMessage.engLen = engCheckedWorld->engNameLength;
    engCheckedWorld->engLastEvent =
        engCheckedWorld->engPendingEvent;
    engCheckedWorld->engEventPending = false;
    engCallback(engMessage, engUserdata1, engUserdata2);
}

/* The last delivered event is per-world state set immediately before the
 * deferred callback fires; NULL maps to the enum's zero member. */
EngEventKind engWorldLastEvent(EngWorld engWorld) {
    struct EngWorld_T *engCheckedWorld = engWorldChecked(engWorld);
    if (engCheckedWorld == NULL) {
        return ENG_EVENT_WORLD_READY;
    }
    return engCheckedWorld->engLastEvent;
}

/* A simulation step records its input without floating accumulation,
 * advances per-world counters in unsigned arithmetic, and queues an event. */
void engWorldStep(EngWorld engWorld, float engFixedStep) {
    struct EngWorld_T *engCheckedWorld = engWorldChecked(engWorld);
    if (engCheckedWorld == NULL) {
        return;
    }
    engCheckedWorld->engSimulationStepValue = engFixedStep;
    engCheckedWorld->engSimulationStepIndex += UINT64_C(1);
    engCheckedWorld->engSimulationTicks +=
        engCheckedWorld->engTicksPerFrame;
    engCheckedWorld->engPendingEvent = ENG_EVENT_FRAME_STEPPED;
    engCheckedWorld->engEventPending = true;
}

/* Frame begin updates only the calling thread's record; it does not mutate
 * the world or perform a simulation step. */
void engFrameBegin(EngWorld engWorld, float engFixedStep) {
    engFrameRecord.engWorld = engWorldChecked(engWorld);
    engFrameRecord.engFixedStep = engFixedStep;
    engFrameRecord.engFrameIndex += UINT64_C(1);
}

/* The current frame world is the calling thread's explicit record; static
 * thread-local initialization supplies NULL before the first begin. */
EngWorld engFrameWorld(void) {
    return engFrameRecord.engWorld;
}

/* The current fixed step is exactly the calling thread's begin value, with
 * no clock read or floating accumulation. */
float engFrameFixedStep(void) {
    return engFrameRecord.engFixedStep;
}

/* The current frame index is the calling thread's unsigned counter; no
 * process-global frame state exists. */
uint64_t engFrameIndex(void) {
    return engFrameRecord.engFrameIndex;
}
