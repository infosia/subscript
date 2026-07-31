/*
 * interop.h — synthetic C interop fixture for the P5 layout proof.
 *
 * This header is invented for this project's own interop vertical slice
 * (specs/blocks/compiler.md §12). It names and depends on no external
 * project, library, or platform API; every identifier uses a synthetic
 * `Sub` prefix. It contains only C structs, enums, opaque-handle
 * typedefs, and function-pointer typedefs — no unions and no bitfields
 * (§12.1): the C-ABI layout-identity guarantee (design invariant 1) is
 * about C structs / enums / pointers / function-pointers /
 * opaque-handles only.
 *
 * It exercises, one construct per plan §4 interop pattern:
 *   1. an intrusive extension chain (embedded header + two extensions);
 *   2. a (pointer, count) array-pair descriptor;
 *   3. a length-carrying string view (no NUL assumption);
 *   4. a callback-info struct (function pointer + userdata);
 *   5. an opaque handle with create / retain / release.
 *
 * The `offsetof` proof (§12.3) mirrors every struct here that is
 * expressible as a language `@CStruct class` and asserts byte-identical
 * layout against the platform C compiler. Function declarations carry no
 * layout and exist only so P5.2/P5.3 can build the mirror generator and
 * the linked callee on top of this fixture.
 */

#ifndef SUBSCRIPT_INTEROP_H
#define SUBSCRIPT_INTEROP_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* Binary16 storage spelling. The gate compiler is clang (§11), where
 * `_Float16` has an unambiguous IEEE binary16 representation. Do not
 * substitute an integer fallback: bindgen must fail loud on targets
 * without a real half-width float instead of emitting a wrong mirror.
 * No fixture code performs arithmetic on this type. */
typedef _Float16 SubFloat16;

/* ---- Pattern 1: intrusive extension chain -------------------------- */

/* Type tag identifying the concrete struct behind a chain header. */
typedef enum SubChainKind {
    SUB_CHAIN_KIND_BASE = 0,
    SUB_CHAIN_KIND_EXT_A = 1,
    SUB_CHAIN_KIND_EXT_B = 2
} SubChainKind;

/* Common header embedded as the first field of every chainable struct.
 * A reader walks `next`, switching on `sType`. */
typedef struct SubChainHeader {
    SubChainKind sType;
    struct SubChainHeader *next;
} SubChainHeader;

/* Extension whose first field is the common header (structurally an
 * "is-a" chain node), followed by extra scalar payload. */
typedef struct SubChainExtA {
    SubChainHeader header;
    float intensity;
    uint32_t flags;
} SubChainExtA;

/* A second, differently sized extension: proves the chain admits more
 * than one concrete follower. */
typedef struct SubChainExtB {
    SubChainHeader header;
    double scale;
    int32_t level;
} SubChainExtB;

/* Walks a chain and folds the payload scalars of tagged ExtA and ExtB
 * nodes in field order. A tagged node must address the matching
 * extension's embedded header; unsigned accumulation defines wrapping,
 * and casting each float to int32_t before folding makes the payload
 * read-back an exact observable. */
int32_t subChainPayloadValue(SubChainHeader *chain);

/* ---- Pattern 2: (pointer, count) array-pair descriptor ------------- */

/* Borrowed view over a contiguous run of elements; the callee reads
 * `count` items starting at `items` and does not take ownership. */
typedef struct SubBufferView {
    const uint32_t *items;
    size_t count;
} SubBufferView;

/* ---- Pattern 3: length-carrying string view ------------------------ */

/* UTF-8 bytes plus an explicit length; no terminating NUL is assumed. */
typedef struct SubStringView {
    const char *data;
    size_t len;
} SubStringView;

/* ---- Pattern 4: callback info (function pointer + userdata) --------- */

/* Callback receiving a string view by value plus TWO opaque userdata
 * pointers supplied at registration time (§14.4): the common
 * two-userdata callback shape a production async C API uses. */
typedef void (*SubLogCallback)(SubStringView message, void *userdata1, void *userdata2);

/* Registration record: the callback plus two independent userdata
 * slots (a primary sink and an auxiliary context). */
typedef struct SubCallbackInfo {
    SubLogCallback callback;
    void *userdata;
    void *userparam;
} SubCallbackInfo;

/* ---- Alignment-exercising payload structs -------------------------- */
/* These are not one of the five patterns; they exist so the mirrored
 * set includes a fixed C array and non-trivial interior padding, making
 * the offsetof proof discriminating (§12.1, §12.3). */

/* Fixed C array (mirrors FixedArray<f32, 16>) followed by mixed scalars
 * that force interior and trailing padding. */
typedef struct SubTransform {
    float basis[16];
    int32_t bone;
    double weight;
    bool visible;
} SubTransform;

/* A minimal mixed-alignment struct: bool then f64 forces 7 bytes of
 * padding, exercising the worst-case leading gap. */
typedef struct SubSample {
    bool a;
    double b;
    int32_t c;
    float d;
} SubSample;

/* ---- Pattern 5: opaque handle with create / retain / release ------- */

/* Handle to an incomplete type: callers hold the pointer, never the
 * layout. Not mirrorable as a value class (no visible fields); it lowers
 * to a pointer-sized handle at the language boundary (P5.2). */
typedef struct SubDevice_T *SubDevice;

/* Lifecycle declarations only; a minimal implementation is P5.3's
 * concern and is not needed for the layout proof. */
SubDevice subDeviceCreate(SubChainHeader *chain);
void subDeviceRetain(SubDevice device);
void subDeviceRelease(SubDevice device);

/* Representative uses of the pattern structs, so the mirror generator
 * (P5.2) has foreign functions to bind. Declarations only. */
void subDeviceSubmit(SubDevice device, SubBufferView commands);
void subDeviceSetLogger(SubDevice device, SubCallbackInfo logger);
void subDeviceSetLabel(SubDevice device, SubStringView label);

/* Q34 deterministic foreign poll: returns ready after two pending
 * attempts. The caller supplies its attempt number, so repeated test runs
 * share no hidden fixture state. */
int32_t subDevicePoll(int32_t attempt);

/* ---- Pattern 2 (cont.): typed slice descriptors, multiple element ----
 *      types.
 *
 * The (pointer, count) descriptor generalizes across primitive element
 * types: each `SubSlice*` borrows a contiguous run of one element type,
 * and the matching `subSliceChecksum*` reads all `count` elements from
 * `items` — the borrow is zero-copy, the callee reading the caller's own
 * array storage directly. This is the generic "typed descriptor →
 * (array, count) converted host-side" facade that lets a caller hand a
 * primitive array to a C API with no copy, for any element type, not
 * just the u32 case (SubBufferView above; no specific external API is
 * named). Each checksum is an order-sensitive, i32-wrapping rolling hash
 * `h = h*31 + (int32_t)items[i]`, computed in unsigned arithmetic so the
 * wrap is well-defined; float elements are cast to int32_t first, so the
 * result is exact and independent of floating-point format. Deterministic
 * and headless. */

typedef struct SubSliceF32 {
    const float *items;
    size_t count;
} SubSliceF32;

typedef struct SubSliceI32 {
    const int32_t *items;
    size_t count;
} SubSliceI32;

typedef struct SubSliceF64 {
    const double *items;
    size_t count;
} SubSliceF64;

typedef struct SubSliceI64 {
    const int64_t *items;
    size_t count;
} SubSliceI64;

typedef struct SubSliceU8 {
    const uint8_t *items;
    size_t count;
} SubSliceU8;

typedef struct SubSliceI8 {
    const int8_t *items;
    size_t count;
} SubSliceI8;

typedef struct SubSliceU16 {
    const uint16_t *items;
    size_t count;
} SubSliceU16;

typedef struct SubSliceI16 {
    const int16_t *items;
    size_t count;
} SubSliceI16;

typedef struct SubSliceF16 {
    const SubFloat16 *items;
    size_t count;
} SubSliceF16;

int32_t subSliceChecksumF32(SubSliceF32 data);
int32_t subSliceChecksumI32(SubSliceI32 data);
int32_t subSliceChecksumF64(SubSliceF64 data);
int32_t subSliceChecksumI64(SubSliceI64 data);
int32_t subSliceChecksumU8(SubSliceU8 data);
int32_t subSliceChecksumI8(SubSliceI8 data);
int32_t subSliceChecksumU16(SubSliceU16 data);
int32_t subSliceChecksumI16(SubSliceI16 data);
int32_t subSliceChecksumF16(SubSliceF16 data);

/* ==== P6.2 production-C binding shapes (compiler.md §13.2) ============= */

/* ---- Flag typedef end-to-end ---------------------------------------- *
 *
 * A `uintN` alias plus `static const` members combinable with `|` (Q18).
 * The mirror maps the alias to a `u64` type alias and folds each member's
 * value into a `declare const` (bindgen reads the value from the C
 * `static const`). subAccessMatches reports whether every bit in
 * `required` is set in `mask` — the observable bit test. */

typedef uint64_t SubAccess;
static const SubAccess SUB_ACCESS_NONE = 0x0;
static const SubAccess SUB_ACCESS_READ = 0x1;
static const SubAccess SUB_ACCESS_WRITE = 0x2;
static const SubAccess SUB_ACCESS_EXEC = 0x4;

int32_t subAccessMatches(SubAccess mask, SubAccess required);

/* ---- Descriptor-embedded (count, pointer) array --------------------- *
 *
 * Production headers place an array as adjacent count+pointer fields
 * inside a larger struct, not as a standalone descriptor. The mirror
 * recognizes the `size_t <n>Count; const T* <n>;` pair by name, maps the
 * pointer field to `T[]`, and elides the count; the lowering reconstructs
 * the pair count-first from the one array (zero-copy). `layer` makes this
 * a genuinely larger struct (>16 bytes, exercising the by-reference ABI
 * path). subDrawListTotal sums `layer + every draw`. */

typedef struct SubDrawList {
    uint32_t layer;
    size_t drawsCount;
    const uint32_t *draws;
} SubDrawList;

int32_t subDrawListTotal(SubDrawList list);

/* ---- Untyped bulk-data facade --------------------------------------- *
 *
 * A `const void* data, size_t size` (byte-size) API, plus a thin typed C
 * facade taking a typed slice descriptor (as SubSliceF32) that computes
 * `size = count * sizeof(T)` and forwards to the untyped API zero-copy.
 * The subscript program passes an `f32[]` to the facade (bound as `T[]`);
 * the untyped API records the byte size in its checksum. The documented
 * path for `void*`+byte-size APIs. */

int32_t subBulkConsume(const void *data, size_t size);
int32_t subBulkConsumeF32(SubSliceF32 data);

/* ==== P6.3 async callback model (compiler.md §13.3) =================== *
 *
 * Production callbacks register a callback-info NOW and fire it LATER,
 * host-driven, unlike P5's synchronous single fire (a28 / subDeviceSetLogger,
 * which fires inside the registering call). subDeviceOnComplete STORES the
 * callback + userdata in the device and returns without firing;
 * subDevicePump is the host driver that fires the stored callback AFTER the
 * registering call returned. The userdata-lifetime rule (Q13: userdata must
 * outlive the registration) is what makes the deferred fire correct — the
 * runtime's callback binding is Context-held (lives for the whole run), so
 * the binding stored at registration is still valid when a later pump reads
 * it back. The message pump fires carries a length derived from the device's
 * accumulated work (subDeviceSubmit's running sum plus chain depth), so the
 * callback observes work that happened BETWEEN registration and fire. */

typedef struct SubCompletionInfo {
    SubLogCallback callback;
    void *userdata;
} SubCompletionInfo;

void subDeviceOnComplete(SubDevice device, SubCompletionInfo info);
void subDevicePump(SubDevice device);

/* ==== P6.3 production-scale layout fixture (compiler.md §13.4) ======== *
 *
 * The P5 offsetof proof (§12.3) covered ~8 structs. These carry it to
 * production scale/complexity — dozens of structs mixing varied
 * scalar/padding layouts, nested by-value structs, flag-typedef fields,
 * intrusive chains, and descriptor-embedded arrays — so the offsetof suite
 * proves the C-ABI layout identity (invariant 1) holds at that scale. All
 * neutral and `Sub`-prefixed; no external API is named. Only C
 * structs/enums/pointers here (no unions, no bitfields — §12.1). The
 * two-field `{ const T*; size_t }` slice descriptors above (SubSlice*) are
 * also layout-proven; the offsetof suite mirrors them by their raw pair
 * layout. */

/* Varied scalar / padding layouts. */
typedef struct SubVec2 {
    float x;
    float y;
} SubVec2;

typedef struct SubVec3 {
    float x;
    float y;
    float z;
} SubVec3;

typedef struct SubVec4 {
    float x;
    float y;
    float z;
    float w;
} SubVec4;

typedef struct SubRect {
    int32_t x;
    int32_t y;
    uint32_t width;
    uint32_t height;
} SubRect;

typedef struct SubRange {
    uint64_t offset;
    uint64_t size;
} SubRange;

typedef struct SubColor {
    float r;
    float g;
    float b;
    float a;
} SubColor;

/* f64/f64/i32 forces trailing padding to an 8-byte size multiple. */
typedef struct SubTimings {
    double cpu;
    double gpu;
    int32_t frame;
} SubTimings;

/* bool/i64/bool/f32 forces two interior leading gaps. */
typedef struct SubMixed {
    bool enabled;
    int64_t id;
    bool visible;
    float ratio;
} SubMixed;

/* i32/bool/f64: a bool between a 4- and an 8-aligned field. */
typedef struct SubPadB {
    int32_t head;
    bool mid;
    double tail;
} SubPadB;

/* Production-shaped narrow payload: byte/short/binary16 fields mixed
 * with established 32- and 64-bit scalars. This is the P14 bindgen and
 * offsetof proof fixture. */
typedef struct SubNarrowPacket {
    uint8_t kind;
    int16_t delta;
    SubFloat16 weight;
    uint64_t serial;
    int8_t bias;
    uint16_t count;
    float scale;
} SubNarrowPacket;

/* Nested by-value structs. */
typedef struct SubExtent {
    uint32_t width;
    uint32_t height;
    uint32_t depth;
} SubExtent;

/* Nested struct + a flag-typedef field (SubAccess is the §13.2 uint64 flag
 * alias declared above). The 12-byte extent then a u32 then an 8-aligned
 * flag field exercises interior padding around a nested aggregate. */
typedef struct SubImageInfo {
    SubExtent extent;
    uint32_t mipLevels;
    SubAccess usage;
} SubImageInfo;

typedef struct SubBounds {
    SubVec3 min;
    SubVec3 max;
} SubBounds;

typedef struct SubViewport {
    SubRect rect;
    SubRange depth;
} SubViewport;

/* Two levels of nesting: SubNodeInfo -> SubBounds -> SubVec3. */
typedef struct SubNodeInfo {
    SubBounds bounds;
    uint32_t id;
    SubColor tint;
} SubNodeInfo;

/* Intrusive-chain extensions: the common SubChainHeader embedded first,
 * followed by differently sized/aligned payloads (adds to SubChainExtA/B). */
typedef struct SubChainExtC {
    SubChainHeader header;
    SubVec3 offset;
    uint32_t flags;
} SubChainExtC;

typedef struct SubChainExtD {
    SubChainHeader header;
    double scale;
    int64_t level;
    bool active;
} SubChainExtD;

/* A second, independent intrusive chain with its own header type. */
typedef struct SubEventHeader {
    int32_t kind;
    struct SubEventHeader *next;
} SubEventHeader;

typedef struct SubEventKey {
    SubEventHeader header;
    uint32_t code;
    bool pressed;
} SubEventKey;

typedef struct SubEventMove {
    SubEventHeader header;
    float dx;
    float dy;
} SubEventMove;

/* Flag-typedef fields leading a struct (SubAccess is 8-aligned). */
typedef struct SubPassInfo {
    SubAccess access;
    uint32_t width;
    uint32_t height;
} SubPassInfo;

typedef struct SubResourceDesc {
    SubAccess usage;
    SubRange range;
    uint32_t count;
} SubResourceDesc;

/* Descriptor-embedded (count, pointer) array inside a larger struct: the
 * mirror collapses `commandsCount`+`commands` to `u32[]`, but the raw C
 * layout (three fields) is what both tiers marshal, so the offsetof suite
 * proves that raw layout is what the language computes (as it does for
 * SubDrawList above). */
typedef struct SubCommandBuffer {
    uint32_t queue;
    size_t commandsCount;
    const uint32_t *commands;
} SubCommandBuffer;

int32_t subCommandBufferTotal(SubCommandBuffer buf);

/* ==== P7.1 incremental async/Future interop shapes (compiler.md §14) === */

/* ---- §14.1 Chained (two-level) flag alias --------------------------- *
 *
 * A production flag typedef spelled as TWO typedef levels: SubStageFlags is
 * a typedef of SubStageBits, itself a typedef of uint64_t. The mirror
 * follows the chain to the underlying integer (`type SubStageFlags = u64`),
 * emits the `static const` members as folded `declare const`s, and they
 * combine with `|` (Q18) and cross to a foreign bit test exactly as the
 * one-level SubAccess flag (a33) does. Two levels over uint64_t (not
 * uint32_t) so the folded members — typed u64 by the ambient-const rule —
 * match the alias width and the foreign parameter width. subStageMatches
 * reports whether every bit of `required` is set in `mask`. */

typedef uint64_t SubStageBits;
typedef SubStageBits SubStageFlags;
static const SubStageFlags SUB_STAGE_NONE = 0x0;
static const SubStageFlags SUB_STAGE_VERTEX = 0x1;
static const SubStageFlags SUB_STAGE_FRAGMENT = 0x2;
static const SubStageFlags SUB_STAGE_COMPUTE = 0x4;

int32_t subStageMatches(SubStageFlags mask, SubStageFlags required);

/* ---- §14.2 By-value boundary-struct return -------------------------- *
 *
 * A foreign function returns a boundary value class BY VALUE. The dev JIT
 * marshals the C-ABI struct return (small structs in registers, larger via
 * sret), subject to the §12.3a by-value-aggregate arch-gate; the ship tier
 * gets the direct C return. SubFuture is the small (register-returned,
 * 8-byte) async-future shape the production model uses; SubStats is the
 * larger (sret-returned, 24-byte) case, so one corpus entry proves both ABI
 * return paths. The returned value class is an ordinary in-language value
 * afterward (read its fields). */

typedef struct SubFuture {
    uint64_t id;
} SubFuture;

typedef struct SubStats {
    uint64_t submitted;
    uint64_t completed;
    uint64_t pending;
} SubStats;

/* Deterministic, no host state: id = request*3 + 1. */
SubFuture subFutureMake(uint32_t request);
/* Deterministic, no host state: (base, base*2, base*3). */
SubStats subStatsMake(uint32_t base);

/* ---- §14.3 Out field written by the callee -------------------------- *
 *
 * A caller-provided boundary struct passed BY REFERENCE (the `Struct | null`
 * boundary form): the callee WRITES its fields and the script reads them
 * back after the call. There is no copy-back — the callee wrote the caller's
 * own storage, because both tiers pass the address of the language struct's
 * storage (layout-identical to the C struct, invariant 1). This is the
 * out-field spelling of §14.3 ("or writes a struct out-field"); SubQueryStatus
 * is exactly the per-future status record the P7.2 out-array capstone fills
 * many of. subDeviceQuery writes status->future (derived from `request` plus
 * the device chain depth) and status->completed = 1. */

typedef struct SubQueryStatus {
    uint64_t future;
    int32_t completed;
} SubQueryStatus;

void subDeviceQuery(SubDevice device, uint32_t request, SubQueryStatus *status);

/* ==== P7.2 composed Future-shape async capstone (compiler.md §14.4/§14.5) ==
 *
 * The whole common main-thread-driven async model, composed: an async op
 * returns a future BY VALUE (§14.2) while taking a two-userdata callback-
 * info (§14.4) and storing it without firing (the a35 deferred model); a
 * host wait/process-events call takes an OUT-ARRAY of per-future status
 * records the callee writes (§14.3, the out-array generalization of a38's
 * out field) and fires the registered callback on the calling thread
 * (§14.6 main-thread model) delivering BOTH userdata. */

/* Per-future status the wait/process-events call writes: the future plus a
 * callee-written completed flag. The array element the out-array fills
 * many of; the same record shape as a38's SubQueryStatus. */
typedef struct SubWaitEntry {
    SubFuture future;
    int32_t completed;
} SubWaitEntry;

/* Mutable (pointer, count) descriptor over SubWaitEntry: the callee WRITES
 * each entry's `completed` through `entries` — the caller's own array
 * storage, layout-identical (invariant 1), so the writes are observed
 * after the call with no copy-back. The non-const pointer marks the
 * out/mutable array (§14.3), distinct from the const borrow of
 * SubBufferView / SubSlice*. The mirror absorbs it into `SubWaitEntry[]`. */
typedef struct SubWaitList {
    SubWaitEntry *entries;
    size_t count;
} SubWaitList;

/* Async op: registers the two-userdata callback-info, returns a future BY
 * VALUE (§14.2), and does NOT fire (deferred, like subDeviceOnComplete).
 * The future id is deterministic: request*3 + 1 (as subFutureMake). */
SubFuture subDeviceKickAsync(SubDevice device, uint32_t request, SubCallbackInfo info);

/* Host driver: writes each wait entry's `completed` flag, then fires the
 * registered async callback ON THE CALLING THREAD (§14.6) delivering both
 * userdata. The fired message length reports the number of entries
 * completed plus the device chain depth, so the callback observes the
 * wait. A no-op when nothing is registered. */
void subDeviceWait(SubDevice device, SubWaitList waits);

/* ==== R5 scalar array-pairs at parameter position (compiler.md §27) =====
 *
 * These functions use adjacent count-first scalar parameters directly,
 * rather than wrapping them in a descriptor struct. The mirror collapses
 * each pair to one language array. A const pointer reads the caller's bytes;
 * mutable pointers fill exactly the supplied array length in place. */

uint32_t subDeviceSumBytes(size_t dataCount, const uint8_t *data);
void subDeviceFillBytes(size_t dataCount, uint8_t *data);
void subDeviceFillShorts(size_t dataCount, uint16_t *data);

#endif /* SUBSCRIPT_INTEROP_H */
