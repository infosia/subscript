/*
 * engine.h — neutral C facade for a small host-owned engine loop.
 *
 * The declarations are invented and use only Eng / eng / ENG_ names.
 * The facade contains the mappable C shapes required by
 * specs/blocks/examples.md §4: an intrusive option chain, a padded
 * transform passed by value, const and mutable descriptors over the same
 * entity-state element, a length-carrying string view, a two-userdata
 * event sink, an opaque world handle, combinable flags, and an embedded
 * count-first array. No union or bitfield participates in the boundary.
 */

#ifndef ENG_ENGINE_H
#define ENG_ENGINE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* The option tag identifies each concrete follower in the intrusive
 * extension-chain pattern; a walker uses the tag, so mixed options remain
 * type-directed. */
typedef enum EngWorldOptionKind {
    ENG_WORLD_OPTION_TICK = 1,
    ENG_WORLD_OPTION_ENTITY_LIMIT = 2
} EngWorldOptionKind;

/* The common first-field header supplies the tag and next pointer for the
 * intrusive extension-chain pattern. A node tagged TICK must address an
 * EngTickOption's embedded engHeader, and a node tagged ENTITY_LIMIT must
 * address an EngEntityLimitOption's embedded engHeader; the walker may
 * then recover and read the matching payload. */
typedef struct EngWorldOption {
    EngWorldOptionKind engKind;
    const struct EngWorldOption *engNext;
} EngWorldOption;

/* The tick option is one concrete intrusive-chain follower; its payload
 * controls the unsigned simulation-tick increment applied by each fixed
 * step. */
typedef struct EngTickOption {
    EngWorldOption engHeader;
    uint32_t engTicksPerFrame;
} EngTickOption;

/* The entity-limit option is the second concrete intrusive-chain follower;
 * its payload narrows the fixed per-world capacity, and values above the
 * implementation bound are clamped rather than causing allocation growth. */
typedef struct EngEntityLimitOption {
    EngWorldOption engHeader;
    uint32_t engMaximumEntities;
} EngEntityLimitOption;

/* The transform is passed by value to engWorldSetTransform; bool before
 * float creates interior padding and uint16_t creates trailing padding, so
 * the declaration exercises real C aggregate layout. */
typedef struct EngTransform {
    bool engInheritScale;
    float engX;
    float engY;
    float engRotation;
    uint16_t engLayer;
} EngTransform;

/* The entity flag typedef is a uint64_t alias; its static members combine
 * with `|`, and the alias width matches the mirror's u64 type for folded
 * ambient integer members, so no implicit narrowing is required. */
typedef uint64_t EngEntityFlags;

/* The empty entity flag member is a folded flag constant; value zero
 * preserves the absence of all independently combinable bits. */
static const EngEntityFlags ENG_ENTITY_FLAG_NONE = 0x0;

/* The active entity flag member is a folded flag constant; value one
 * occupies an independent bit and can be combined with visible. */
static const EngEntityFlags ENG_ENTITY_FLAG_ACTIVE = 0x1;

/* The visible entity flag member is a folded flag constant; value two
 * occupies an independent bit and can be combined with active. */
static const EngEntityFlags ENG_ENTITY_FLAG_VISIBLE = 0x2;

/* Entity state is the shared element type of the const slice and mutable
 * out-array patterns; the same layout is therefore read and written at the
 * boundary with mutability as the only descriptor distinction. */
typedef struct EngEntityState {
    uint32_t engId;
    EngTransform engTransform;
    EngEntityFlags engFlags;
} EngEntityState;

/* The const entity-state descriptor is a borrowed pointer/count slice; the
 * callee reads engCount elements and does not take ownership. */
typedef struct EngEntityStateView {
    const EngEntityState *engItems;
    size_t engCount;
} EngEntityStateView;

/* The mutable entity-state descriptor is an out-array over the same
 * EngEntityState element; the callee writes engCount caller-owned elements,
 * and only pointer mutability distinguishes it from EngEntityStateView. */
typedef struct EngEntityStateOut {
    EngEntityState *engItems;
    size_t engCount;
} EngEntityStateOut;

/* The string view carries bytes and an explicit length; the boundary maps
 * it to string, and no terminating NUL is read or required. */
typedef struct EngStringView {
    const char *engData;
    size_t engLen;
} EngStringView;

/* The event kind identifies work observed by the deferred callback pattern;
 * a pump reports registration, entity mutation, or a completed fixed step. */
typedef enum EngEventKind {
    ENG_EVENT_WORLD_READY = 0,
    ENG_EVENT_ENTITY_CHANGED = 1,
    ENG_EVENT_FRAME_STEPPED = 2
} EngEventKind;

/* The event callback is the supported string-view and two-userdata
 * function-pointer pattern; the event kind is read separately through
 * engWorldLastEvent, and both userdata slots are delivered unchanged. */
typedef void (*EngEventCallback)(
    EngStringView engMessage,
    void *engUserdata1,
    void *engUserdata2);

/* The event sink is the callback-registration struct; storing the function
 * and both userdata slots makes their deferred lifetime explicit. */
typedef struct EngEventSink {
    EngEventCallback engCallback;
    void *engUserdata1;
    void *engUserdata2;
} EngEventSink;

/* The world is an opaque handle; callers retain the pointer-sized identity
 * and use create/retain/release without observing its fixed storage layout. */
typedef struct EngWorld_T *EngWorld;

/* The entity batch embeds a count-first scalar array inside a larger
 * descriptor; engEntityIdsCount is elided and engEntityIds becomes u32[]
 * in the mirror by the existing <n>Count / <n> naming rule. */
typedef struct EngEntityBatch {
    EngEntityFlags engFlags;
    size_t engEntityIdsCount;
    const uint32_t *engEntityIds;
} EngEntityBatch;

/* World creation allocates one opaque handle and consumes at most 32 nodes
 * of the optional intrusive option chain, which terminates a cycle;
 * allocation failure returns NULL, an over-capacity entity limit is
 * clamped, zero tick values are preserved, and unknown or later nodes are
 * ignored. */
EngWorld engWorldCreate(const EngWorldOption *engOptions);

/* World retain implements the opaque-handle lifecycle pattern; the
 * reference count advances in checked unsigned arithmetic, while a NULL
 * handle or a saturated count is a no-op. */
void engWorldRetain(EngWorld engWorld);

/* World release implements the opaque-handle lifecycle pattern; the last
 * release frees that handle's allocation, after which the handle must not
 * be used, while a NULL handle is a no-op. */
void engWorldRelease(EngWorld engWorld);

/* Setting a name consumes the length-carrying string-view pattern; exactly
 * the bytes that fit are copied, embedded NUL bytes have no special
 * meaning, and a NULL handle or NULL data with nonzero length is a no-op. */
void engWorldSetName(EngWorld engWorld, EngStringView engName);

/* Setting one transform passes EngTransform by value; field-wise storage
 * preserves the C values while keeping aggregate padding deterministic. An
 * absent id creates an entity when capacity remains; a NULL handle or a
 * full entity store with no matching id is a no-op. */
void engWorldSetTransform(
    EngWorld engWorld,
    uint32_t engEntityId,
    EngTransform engTransform);

/* Replacing entities consumes the const pointer/count slice; the callee
 * reads up to the configured capacity and never writes caller-owned
 * storage, so overflow stores what fits. NULL data with nonzero count or a
 * NULL handle is a no-op; an empty view clears the store and queues the
 * entity-change event. */
void engWorldReplaceEntities(
    EngWorld engWorld,
    EngEntityStateView engStates);

/* Reading entities consumes the mutable pointer/count out-array; the callee
 * writes caller-owned EngEntityState storage and returns the written count,
 * while a NULL handle or NULL data with nonzero count returns zero. */
size_t engWorldReadEntities(
    EngWorld engWorld,
    EngEntityStateOut engStates);

/* Applying flags consumes the descriptor-embedded count/pointer array and
 * the combinable flag alias; matching entity ids receive the requested
 * bits, the return value counts matches, and a NULL handle or NULL data
 * with nonzero count returns zero. */
size_t engWorldApplyFlags(
    EngWorld engWorld,
    EngEntityBatch engBatch);

/* Registering an event sink stores the callback and both userdata slots and
 * returns without firing; this is the deferred callback registration step,
 * and a NULL handle is a no-op. */
void engWorldSetEventSink(
    EngWorld engWorld,
    EngEventSink engSink);

/* Pumping is host-driven and fires a stored event sink only after
 * registration returned; the callback runs synchronously on the pumping
 * thread and receives the stored two-userdata shape, while a NULL handle or
 * absent callback is a no-op. */
void engWorldPump(EngWorld engWorld);

/* The last-event accessor returns the kind most recently delivered by
 * engWorldPump, which supplies the callback metadata outside the fixed
 * trampoline shape; a NULL handle returns the zero-valued WORLD_READY
 * member. */
EngEventKind engWorldLastEvent(EngWorld engWorld);

/* A fixed simulation step advances only per-world step/tick state and
 * queues a later pump event; frame access is recorded separately by
 * engFrameBegin, and a NULL handle is a no-op. */
void engWorldStep(EngWorld engWorld, float engFixedStep);

/*
 * Exported script functions are zero-argument and void, so the host records
 * thread-local frame state before invoking them. A host loop calls
 * engFrameBegin, invokes the zero-argument script update, then pumps events;
 * the accessors are therefore the direct boundary for a host-owned loop.
 *
 * Before engFrameBegin is called on a thread, the accessors return NULL,
 * 0.0f, and zero. Using that zeroed record is a host bug, not a supported
 * frame path; passing its NULL handle to this facade remains a no-op.
 */

/* Frame begin explicitly records the world and fixed step for the calling
 * thread and advances that thread's unsigned frame index; it does not step
 * the world, and a NULL world is recorded as the zeroed handle. One host
 * loop owns each thread: a world made current on one thread remains alive
 * until that same thread releases it, because release clears only the
 * calling thread's borrowed frame record. */
void engFrameBegin(EngWorld engWorld, float engFixedStep);

/* The current-world accessor reads the calling thread's host-owned frame
 * handle; before frame begin it returns the zeroed NULL handle. */
EngWorld engFrameWorld(void);

/* The fixed-step accessor reads the calling thread's value recorded by
 * engFrameBegin; before frame begin it returns 0.0f. */
float engFrameFixedStep(void);

/* The frame-index accessor reads the calling thread's unsigned host frame
 * counter; before frame begin it returns zero. */
uint64_t engFrameIndex(void);

#endif
