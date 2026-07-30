/*
 * engine.h — neutral C facade for a small host-owned engine loop.
 *
 * The declarations are invented and use only Eng / eng / ENGINE_ names.
 * The facade contains the mappable C shapes required by
 * specs/blocks/examples.md §4: an intrusive option chain, a padded
 * transform passed by value, const and mutable descriptors over the same
 * entity-state element, a length-carrying string view, a two-userdata
 * event sink, an opaque world handle, combinable flags, and an embedded
 * count-first array. No union or bitfield participates in the boundary.
 */

#ifndef ENGINE_ENGINE_H
#define ENGINE_ENGINE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* The option tag identifies each concrete follower in the intrusive
 * extension-chain pattern; a walker uses the tag, so mixed options remain
 * type-directed. */
typedef enum EngineWorldOptionKind {
    ENGINE_WORLD_OPTION_TICK = 1,
    ENGINE_WORLD_OPTION_ENTITY_LIMIT = 2
} EngineWorldOptionKind;

/* The common first-field header supplies the tag and next pointer for the
 * intrusive extension-chain pattern. A node tagged TICK must address an
 * EngineTickOption's embedded engineHeader, and a node tagged ENTITY_LIMIT must
 * address an EngineEntityLimitOption's embedded engineHeader; the walker may
 * then recover and read the matching payload. */
typedef struct EngineWorldOption {
    EngineWorldOptionKind engineKind;
    const struct EngineWorldOption *engineNext;
} EngineWorldOption;

/* The tick option is one concrete intrusive-chain follower; its payload
 * controls the unsigned simulation-tick increment applied by each fixed
 * step. */
typedef struct EngineTickOption {
    EngineWorldOption engineHeader;
    uint32_t engineTicksPerFrame;
} EngineTickOption;

/* The entity-limit option is the second concrete intrusive-chain follower;
 * its payload narrows the fixed per-world capacity, and values above the
 * implementation bound are clamped rather than causing allocation growth. */
typedef struct EngineEntityLimitOption {
    EngineWorldOption engineHeader;
    uint32_t engineMaximumEntities;
} EngineEntityLimitOption;

/* The transform is passed by value to engineWorldSetTransform; bool before
 * float creates interior padding and uint16_t creates trailing padding, so
 * the declaration exercises real C aggregate layout. */
typedef struct EngineTransform {
    bool engineInheritScale;
    float engineX;
    float engineY;
    float engineRotation;
    uint16_t engineLayer;
} EngineTransform;

/* The entity flag typedef is a uint64_t alias; its static members combine
 * with `|`, and the alias width matches the mirror's u64 type for folded
 * ambient integer members, so no implicit narrowing is required. */
typedef uint64_t EngineEntityFlags;

/* The empty entity flag member is a folded flag constant; value zero
 * preserves the absence of all independently combinable bits. */
static const EngineEntityFlags ENGINE_ENTITY_FLAG_NONE = 0x0;

/* The active entity flag member is a folded flag constant; value one
 * occupies an independent bit and can be combined with visible. */
static const EngineEntityFlags ENGINE_ENTITY_FLAG_ACTIVE = 0x1;

/* The visible entity flag member is a folded flag constant; value two
 * occupies an independent bit and can be combined with active. */
static const EngineEntityFlags ENGINE_ENTITY_FLAG_VISIBLE = 0x2;

/* Entity state is the shared element type of the const slice and mutable
 * out-array patterns; the same layout is therefore read and written at the
 * boundary with mutability as the only descriptor distinction. */
typedef struct EngineEntityState {
    uint32_t engineId;
    EngineTransform engineTransform;
    EngineEntityFlags engineFlags;
} EngineEntityState;

/* The const entity-state descriptor is a borrowed pointer/count slice; the
 * callee reads engineCount elements and does not take ownership. */
typedef struct EngineEntityStateView {
    const EngineEntityState *engineItems;
    size_t engineCount;
} EngineEntityStateView;

/* The mutable entity-state descriptor is an out-array over the same
 * EngineEntityState element; the callee writes engineCount caller-owned elements,
 * and only pointer mutability distinguishes it from EngineEntityStateView. */
typedef struct EngineEntityStateOut {
    EngineEntityState *engineItems;
    size_t engineCount;
} EngineEntityStateOut;

/* The string view carries bytes and an explicit length; the boundary maps
 * it to string, and no terminating NUL is read or required. */
typedef struct EngineStringView {
    const char *engineData;
    size_t engineLen;
} EngineStringView;

/* The event kind identifies work observed by the deferred callback pattern;
 * a pump reports registration, entity mutation, or a completed fixed step. */
typedef enum EngineEventKind {
    ENGINE_EVENT_WORLD_READY = 0,
    ENGINE_EVENT_ENTITY_CHANGED = 1,
    ENGINE_EVENT_FRAME_STEPPED = 2
} EngineEventKind;

/* The event callback is the supported string-view and two-userdata
 * function-pointer pattern; the event kind is read separately through
 * engineWorldLastEvent, and both userdata slots are delivered unchanged. */
typedef void (*EngineEventCallback)(
    EngineStringView engineMessage,
    void *engineUserdata1,
    void *engineUserdata2);

/* The event sink is the callback-registration struct; storing the function
 * and both userdata slots makes their deferred lifetime explicit. */
typedef struct EngineEventSink {
    EngineEventCallback engineCallback;
    void *engineUserdata1;
    void *engineUserdata2;
} EngineEventSink;

/* The world is an opaque handle; callers retain the pointer-sized identity
 * and use create/retain/release without observing its fixed storage layout. */
typedef struct EngineWorld_T *EngineWorld;

/* The entity batch embeds a count-first scalar array inside a larger
 * descriptor; engineEntityIdsCount is elided and engineEntityIds becomes u32[]
 * in the mirror by the existing <n>Count / <n> naming rule. */
typedef struct EngineEntityBatch {
    EngineEntityFlags engineFlags;
    size_t engineEntityIdsCount;
    const uint32_t *engineEntityIds;
} EngineEntityBatch;

/* World creation allocates one opaque handle and consumes at most 32 nodes
 * of the optional intrusive option chain, which terminates a cycle;
 * allocation failure returns NULL, an over-capacity entity limit is
 * clamped, zero tick values are preserved, and unknown or later nodes are
 * ignored. */
EngineWorld engineWorldCreate(const EngineWorldOption *engineOptions);

/* World retain implements the opaque-handle lifecycle pattern; the
 * reference count advances in checked unsigned arithmetic, while a NULL
 * handle or a saturated count is a no-op. */
void engineWorldRetain(EngineWorld engineWorld);

/* World release implements the opaque-handle lifecycle pattern; the last
 * release frees that handle's allocation, after which the handle must not
 * be used, while a NULL handle is a no-op. */
void engineWorldRelease(EngineWorld engineWorld);

/* Setting a name consumes the length-carrying string-view pattern; exactly
 * the bytes that fit are copied, embedded NUL bytes have no special
 * meaning, and a NULL handle or NULL data with nonzero length is a no-op. */
void engineWorldSetName(EngineWorld engineWorld, EngineStringView engineName);

/* Setting one transform passes EngineTransform by value; field-wise storage
 * preserves the C values while keeping aggregate padding deterministic. An
 * absent id creates an entity when capacity remains; a NULL handle or a
 * full entity store with no matching id is a no-op. */
void engineWorldSetTransform(
    EngineWorld engineWorld,
    uint32_t engineEntityId,
    EngineTransform engineTransform);

/* Replacing entities consumes the const pointer/count slice; the callee
 * reads up to the configured capacity and never writes caller-owned
 * storage, so overflow stores what fits. NULL data with nonzero count or a
 * NULL handle is a no-op; an empty view clears the store and queues the
 * entity-change event. */
void engineWorldReplaceEntities(
    EngineWorld engineWorld,
    EngineEntityStateView engineStates);

/* Reading entities consumes the mutable pointer/count out-array; the callee
 * writes caller-owned EngineEntityState storage and returns the written count,
 * while a NULL handle or NULL data with nonzero count returns zero. */
size_t engineWorldReadEntities(
    EngineWorld engineWorld,
    EngineEntityStateOut engineStates);

/* Applying flags consumes the descriptor-embedded count/pointer array and
 * the combinable flag alias; matching entity ids receive the requested
 * bits, the return value counts matches, and a NULL handle or NULL data
 * with nonzero count returns zero. */
size_t engineWorldApplyFlags(
    EngineWorld engineWorld,
    EngineEntityBatch engineBatch);

/* Registering an event sink stores the callback and both userdata slots and
 * returns without firing; this is the deferred callback registration step,
 * and a NULL handle is a no-op. */
void engineWorldSetEventSink(
    EngineWorld engineWorld,
    EngineEventSink engineSink);

/* Pumping is host-driven and fires a stored event sink only after
 * registration returned; the callback runs synchronously on the pumping
 * thread and receives the stored two-userdata shape, while a NULL handle or
 * absent callback is a no-op. */
void engineWorldPump(EngineWorld engineWorld);

/* The last-event accessor returns the kind most recently delivered by
 * engineWorldPump, which supplies the callback metadata outside the fixed
 * trampoline shape; a NULL handle returns the zero-valued WORLD_READY
 * member. */
EngineEventKind engineWorldLastEvent(EngineWorld engineWorld);

/* A fixed simulation step advances only per-world step/tick state and
 * queues a later pump event; frame access is recorded separately by
 * engineFrameBegin, and a NULL handle is a no-op. */
void engineWorldStep(EngineWorld engineWorld, float engineFixedStep);

/*
 * Exported script functions are zero-argument and void, so the host records
 * thread-local frame state before invoking them. A host loop calls
 * engineFrameBegin, invokes the zero-argument script update, then pumps events;
 * the accessors are therefore the direct boundary for a host-owned loop.
 *
 * Before engineFrameBegin is called on a thread, the accessors return NULL,
 * 0.0f, and zero. Using that zeroed record is a host bug, not a supported
 * frame path; passing its NULL handle to this facade remains a no-op.
 */

/* Frame begin explicitly records the world and fixed step for the calling
 * thread and advances that thread's unsigned frame index; it does not step
 * the world, and a NULL world is recorded as the zeroed handle. One host
 * loop owns each thread: a world made current on one thread remains alive
 * until that same thread releases it, because release clears only the
 * calling thread's borrowed frame record. */
void engineFrameBegin(EngineWorld engineWorld, float engineFixedStep);

/* The current-world accessor reads the calling thread's host-owned frame
 * handle; before frame begin it returns the zeroed NULL handle. */
EngineWorld engineFrameWorld(void);

/* The fixed-step accessor reads the calling thread's value recorded by
 * engineFrameBegin; before frame begin it returns 0.0f. */
float engineFrameFixedStep(void);

/* The frame-index accessor reads the calling thread's unsigned host frame
 * counter; before frame begin it returns zero. */
uint64_t engineFrameIndex(void);

#endif
