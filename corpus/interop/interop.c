/*
 * interop.c — minimal, deterministic, headless implementation of the
 * synthetic interop header (corpus/interop/interop.h), for the P5.2b
 * foreign-call slice and the P5.3 goldens.
 *
 * It names and depends on no external project or platform API beyond the
 * C standard library. One implementation serves both execution tiers: it
 * is compiled and linked into the dev-JIT test binary (its symbols are
 * registered with the JIT by address) and into the ship-C link, so a
 * foreign call resolves to exactly these definitions under both tiers.
 *
 * Observable behaviour (this defines the P5.3 goldens):
 *   - subDeviceCreate walks the chain and records its depth (number of
 *     nodes reachable through `next`); this exercises the `Struct | null`
 *     pointer marshaling.
 *   - subChainPayloadValue switches on each chain tag and folds the
 *     matching extension payload, making embedded-header address
 *     marshaling observable.
 *   - subDeviceSetLabel stores the label bytes (a (ptr,len) string view).
 *   - subDeviceSetLogger stores the callback + userdata and immediately
 *     invokes the callback once with the stored label as the message, so
 *     "setLogger invokes the callback" is observable.
 *   - subDeviceSubmit sums the (ptr,count) command view, then invokes the
 *     callback with a message whose length is (sum + chain depth), so the
 *     array sum and the chain depth are both observable through the
 *     callback's `message.length`.
 *
 * The callback is the single observability channel: a language callback
 * sees only its `message` (whose length encodes the effect) and its
 * `userdata`, so a test program surfaces effects by accumulating
 * `message.length` into a userdata sink and printing it. Everything here
 * is single-threaded and deterministic.
 */

#define SUBSCRIPT_INTEROP_HOST 1
#include "interop.h"

#include <stdlib.h>
#include <string.h>

/* The generated AOT entry declares these fixture adapters with the same
 * incomplete runtime Context type. The adapters are host code and do not
 * enter or exit script mode. */
typedef struct subscript_rt_context subscript_rt_context;

/* Concrete layout behind the opaque SubDevice handle. Callers never see
 * it; they hold the pointer only. */
struct SubDevice_T {
    int refs;
    int chain_depth;
    long long acc;
    SubLogCallback cb;
    void *cb_userdata;
    void *cb_userparam;
    size_t label_len;
    char label[128];
    /* Deferred completion callback: STORED by subDeviceOnComplete, FIRED
     * later by subDevicePump (P6.3 async model). Separate slots from the
     * synchronous logger above so a device can carry both. */
    SubLogCallback completion_cb;
    void *completion_userdata;
    /* Composed async capstone (P7.2): STORED by subDeviceKickAsync, FIRED
     * later by subDeviceWait, carrying both userdata slots. */
    SubLogCallback async_cb;
    void *async_ud1;
    void *async_ud2;
};

/* Deterministic scratch used to synthesize a callback message of a given
 * length. Single-threaded; refilled on each use. */
static char subscript_msgbuf[256];

/* R21 host-owned state. The script receives only a borrowed opaque handle;
 * the host adapters below retain the owning pointer for the whole run. */
struct SubHostOwnedState_T {
    int32_t counter;
};

static SubHostOwnedState subscript_host_owned_state;

SubHostOwnedState subHostOwnedStateCreate(void) {
    SubHostOwnedState state =
        (SubHostOwnedState)calloc(1, sizeof(struct SubHostOwnedState_T));
    if (state != NULL) {
        state->counter = 40;
    }
    return state;
}

void subHostOwnedStateDestroy(SubHostOwnedState state) {
    free(state);
}

SubHostOwnedState subHostOwnedStateBorrow(void) {
    return subscript_host_owned_state;
}

int32_t subHostOwnedStateAdvance(SubHostOwnedState state) {
    if (state == NULL) {
        return -1;
    }
    return state->counter++;
}

void subHostOwnedStatePreEntry(subscript_rt_context *ctx) {
    (void)ctx;
    subscript_host_owned_state = subHostOwnedStateCreate();
}

void subHostOwnedStatePostRun(subscript_rt_context *ctx) {
    (void)ctx;
    subHostOwnedStateDestroy(subscript_host_owned_state);
    subscript_host_owned_state = NULL;
}

int32_t subDevicePoll(int32_t attempt) {
    return attempt >= 2 ? 1 : 0;
}

int32_t subChainPayloadValue(SubChainHeader *chain) {
    uint32_t h = 0u;
    for (SubChainHeader *n = chain; n != NULL; n = n->next) {
        switch (n->sType) {
            case SUB_CHAIN_KIND_EXT_A: {
                const SubChainExtA *ext = (const SubChainExtA *)(const void *)n;
                h = h * 31u + (uint32_t)(int32_t)ext->intensity;
                h = h * 31u + ext->flags;
                break;
            }
            case SUB_CHAIN_KIND_EXT_B: {
                const SubChainExtB *ext = (const SubChainExtB *)(const void *)n;
                h = h * 31u + (uint32_t)(int32_t)ext->scale;
                h = h * 31u + (uint32_t)ext->level;
                break;
            }
            case SUB_CHAIN_KIND_BASE:
                break;
        }
    }
    return (int32_t)h;
}

SubDevice subDeviceCreate(SubChainHeader *chain) {
    struct SubDevice_T *d = (struct SubDevice_T *)calloc(1, sizeof(*d));
    if (d == NULL) {
        return NULL;
    }
    d->refs = 1;
    int depth = 0;
    for (SubChainHeader *n = chain; n != NULL; n = n->next) {
        depth++;
    }
    d->chain_depth = depth;
    return d;
}

void subDeviceRetain(SubDevice device) {
    if (device != NULL) {
        device->refs++;
    }
}

void subDeviceRelease(SubDevice device) {
    if (device != NULL && --device->refs == 0) {
        free(device);
    }
}

void subDeviceSetLabel(SubDevice device, SubStringView label) {
    if (device == NULL) {
        return;
    }
    size_t n = label.len;
    if (n > sizeof(device->label)) {
        n = sizeof(device->label);
    }
    if (n > 0 && label.data != NULL) {
        memcpy(device->label, label.data, n);
    }
    device->label_len = n;
}

void subDeviceSetLogger(SubDevice device, SubCallbackInfo logger) {
    if (device == NULL) {
        return;
    }
    device->cb = logger.callback;
    device->cb_userdata = logger.userdata;
    device->cb_userparam = logger.userparam;
    if (device->cb != NULL) {
        SubStringView msg;
        msg.data = device->label;
        msg.len = device->label_len;
        device->cb(msg, device->cb_userdata, device->cb_userparam);
    }
}

void subDeviceSubmit(SubDevice device, SubBufferView commands) {
    if (device == NULL) {
        return;
    }
    long long sum = 0;
    for (size_t i = 0; i < commands.count; i++) {
        sum += (long long)commands.items[i];
    }
    device->acc += sum;
    if (device->cb != NULL) {
        long long n = sum + (long long)device->chain_depth;
        if (n < 0) {
            n = 0;
        }
        if (n > (long long)sizeof(subscript_msgbuf)) {
            n = (long long)sizeof(subscript_msgbuf);
        }
        memset(subscript_msgbuf, 'x', (size_t)n);
        SubStringView msg;
        msg.data = subscript_msgbuf;
        msg.len = (size_t)n;
        device->cb(msg, device->cb_userdata, device->cb_userparam);
    }
}

/* Typed slice facades: each reads all `data.count` elements straight from
 * the borrowed `data.items` — the caller's own array storage (zero-copy)
 * — and returns an order-sensitive, i32-wrapping rolling hash. The
 * accumulation is unsigned so the wrap is well-defined; float elements
 * are cast to int32_t first so the result is exact and float-format
 * independent. The returned checksum depends on every element value,
 * which is what makes the zero-copy read observable. */

int32_t subSliceChecksumF32(SubSliceF32 data) {
    uint32_t h = 0u;
    for (size_t i = 0; i < data.count; i++) {
        h = h * 31u + (uint32_t)(int32_t)data.items[i];
    }
    return (int32_t)h;
}

int32_t subSliceChecksumI32(SubSliceI32 data) {
    uint32_t h = 0u;
    for (size_t i = 0; i < data.count; i++) {
        h = h * 31u + (uint32_t)data.items[i];
    }
    return (int32_t)h;
}

int32_t subSliceChecksumF64(SubSliceF64 data) {
    uint32_t h = 0u;
    for (size_t i = 0; i < data.count; i++) {
        h = h * 31u + (uint32_t)(int32_t)data.items[i];
    }
    return (int32_t)h;
}

int32_t subSliceChecksumI64(SubSliceI64 data) {
    uint32_t h = 0u;
    for (size_t i = 0; i < data.count; i++) {
        h = h * 31u + (uint32_t)(int32_t)data.items[i];
    }
    return (int32_t)h;
}

int32_t subSliceChecksumU8(SubSliceU8 data) {
    uint32_t h = 0u;
    for (size_t i = 0; i < data.count; i++) {
        h = h * 31u + (uint32_t)data.items[i];
    }
    return (int32_t)h;
}

int32_t subSliceChecksumI8(SubSliceI8 data) {
    uint32_t h = 0u;
    for (size_t i = 0; i < data.count; i++) {
        h = h * 31u + (uint32_t)(int32_t)data.items[i];
    }
    return (int32_t)h;
}

int32_t subSliceChecksumU16(SubSliceU16 data) {
    uint32_t h = 0u;
    for (size_t i = 0; i < data.count; i++) {
        h = h * 31u + (uint32_t)data.items[i];
    }
    return (int32_t)h;
}

int32_t subSliceChecksumI16(SubSliceI16 data) {
    uint32_t h = 0u;
    for (size_t i = 0; i < data.count; i++) {
        h = h * 31u + (uint32_t)(int32_t)data.items[i];
    }
    return (int32_t)h;
}

int32_t subSliceChecksumF16(SubSliceF16 data) {
    uint32_t h = 0u;
    for (size_t i = 0; i < data.count; i++) {
        uint16_t bits = 0u;
        memcpy(&bits, &data.items[i], sizeof(bits));
        h = h * 31u + (uint32_t)bits;
    }
    return (int32_t)h;
}

/* ==== P6.2 production-C binding shapes (compiler.md §13.2) ============= */

/* Flag bit test: 1 when every bit of `required` is set in `mask`, else 0.
 * The observable result of combining flag members with `|` and passing the
 * combined u64 across the boundary. */
int32_t subAccessMatches(SubAccess mask, SubAccess required) {
    return (mask & required) == required ? 1 : 0;
}

/* Descriptor-embedded (count, pointer) array: sum `layer` plus every draw.
 * Reads the borrowed `draws` run zero-copy (the caller's own array
 * storage), so the returned total depends on every element. */
int32_t subDrawListTotal(SubDrawList list) {
    long long sum = (long long)list.layer;
    for (size_t i = 0; i < list.drawsCount; i++) {
        sum += (long long)list.draws[i];
    }
    return (int32_t)sum;
}

/* Untyped bulk-data API: a raw byte range. Records the byte size in an
 * order-sensitive, i32-wrapping rolling checksum over the bytes, seeded by
 * the byte size, so the returned value witnesses both the size and the
 * bytes. */
int32_t subBulkConsume(const void *data, size_t size) {
    const unsigned char *b = (const unsigned char *)data;
    uint32_t h = (uint32_t)size;
    for (size_t i = 0; i < size; i++) {
        h = h * 31u + (uint32_t)b[i];
    }
    return (int32_t)h;
}

/* Typed facade over the untyped API: computes the byte size from the typed
 * f32 slice (count * sizeof(float)) and forwards the borrowed run zero-copy.
 * The documented path for `void*`+byte-size APIs. */
int32_t subBulkConsumeF32(SubSliceF32 data) {
    return subBulkConsume(data.items, data.count * sizeof(float));
}

/* ==== P6.3 async callback model (compiler.md §13.3) =================== */

/* Registration: STORE the callback + userdata; return WITHOUT firing. This
 * is the deferred model — unlike subDeviceSetLogger, which fires inside the
 * registering call. The stored userdata is the runtime callback binding; it
 * is subscript_rt_context-held and outlives this call (the Q13 lifetime rule), so a later
 * pump can read it back. */
void subDeviceOnComplete(SubDevice device, SubCompletionInfo info) {
    if (device == NULL) {
        return;
    }
    device->completion_cb = info.callback;
    device->completion_userdata = info.userdata;
}

/* Host driver: fire the stored completion callback AFTER the registering
 * call returned. The message length reports the device's accumulated work
 * (submit sum + chain depth), so the callback observes work performed
 * between registration and this fire. A no-op when nothing is registered. */
void subDevicePump(SubDevice device) {
    if (device == NULL || device->completion_cb == NULL) {
        return;
    }
    long long n = device->acc + (long long)device->chain_depth;
    if (n < 0) {
        n = 0;
    }
    if (n > (long long)sizeof(subscript_msgbuf)) {
        n = (long long)sizeof(subscript_msgbuf);
    }
    memset(subscript_msgbuf, 'x', (size_t)n);
    SubStringView msg;
    msg.data = subscript_msgbuf;
    msg.len = (size_t)n;
    device->completion_cb(msg, device->completion_userdata, NULL);
}

/* Descriptor-embedded (count, pointer) array at production scale: sum
 * `queue` plus every command, reading the borrowed run zero-copy (as
 * subDrawListTotal does for SubDrawList). */
int32_t subCommandBufferTotal(SubCommandBuffer buf) {
    long long sum = (long long)buf.queue;
    for (size_t i = 0; i < buf.commandsCount; i++) {
        sum += (long long)buf.commands[i];
    }
    return (int32_t)sum;
}

/* ==== P7.1 incremental async/Future interop shapes (compiler.md §14) === */

/* §14.1 chained flag alias: 1 when every bit of `required` is set in
 * `mask`, else 0 — the same observable bit test as subAccessMatches, over
 * the two-level SubStageFlags alias. */
int32_t subStageMatches(SubStageFlags mask, SubStageFlags required) {
    return (mask & required) == required ? 1 : 0;
}

/* §14.2 by-value struct return. Both are deterministic and host-state-free
 * so the corpus golden depends only on the argument, isolating the struct-
 * return marshaling. SubFuture is 8 bytes (register return); SubStats is 24
 * bytes (sret). */
SubFuture subFutureMake(uint32_t request) {
    SubFuture f;
    f.id = (uint64_t)request * 3u + 1u;
    return f;
}

SubStats subStatsMake(uint32_t base) {
    SubStats s;
    s.submitted = (uint64_t)base;
    s.completed = (uint64_t)base * 2u;
    s.pending = (uint64_t)base * 3u;
    return s;
}

/* §14.3 out field: WRITE the caller-provided status record. The pointer is
 * the caller's own storage (layout-identical), so the writes are observed by
 * the script after this returns with no copy-back. future encodes the
 * request plus the device chain depth; completed is set to 1. */
void subDeviceQuery(SubDevice device, uint32_t request, SubQueryStatus *status) {
    if (status == NULL) {
        return;
    }
    int depth = (device != NULL) ? device->chain_depth : 0;
    status->future = (uint64_t)request * 10u + (uint64_t)depth;
    status->completed = 1;
}

/* ==== P7.2 composed Future-shape async capstone (compiler.md §14.4/§14.5) == */

/* Register the two-userdata callback-info (STORE, do not fire — the a35
 * deferred model) and return a future BY VALUE (§14.2). Both userdata slots
 * are stored so a later subDeviceWait fires the callback with both. */
SubFuture subDeviceKickAsync(SubDevice device, uint32_t request, SubCallbackInfo info) {
    SubFuture f;
    f.id = (uint64_t)request * 3u + 1u;
    if (device != NULL) {
        device->async_cb = info.callback;
        device->async_ud1 = info.userdata;
        device->async_ud2 = info.userparam;
    }
    return f;
}

/* Host driver: WRITE each wait entry's `completed` flag through the caller's
 * own array storage (§14.3 out-array; no copy-back), then fire the stored
 * async callback on THIS thread (§14.6) delivering both userdata. The fired
 * message length is (entries completed + chain depth), so the callback
 * observes the wait. */
void subDeviceWait(SubDevice device, SubWaitList waits) {
    if (device == NULL) {
        return;
    }
    long long done = 0;
    for (size_t i = 0; i < waits.count; i++) {
        waits.entries[i].completed = 1;
        done++;
    }
    if (device->async_cb != NULL) {
        long long n = done + (long long)device->chain_depth;
        if (n < 0) {
            n = 0;
        }
        if (n > (long long)sizeof(subscript_msgbuf)) {
            n = (long long)sizeof(subscript_msgbuf);
        }
        memset(subscript_msgbuf, 'x', (size_t)n);
        SubStringView msg;
        msg.data = subscript_msgbuf;
        msg.len = (size_t)n;
        device->async_cb(msg, device->async_ud1, device->async_ud2);
    }
}

/* ==== R5 scalar array-pairs at parameter position (compiler.md §27) ===== */

uint32_t subDeviceSumBytes(size_t dataCount, const uint8_t *data) {
    uint32_t sum = 0u;
    for (size_t i = 0; i < dataCount; i++) {
        sum += (uint32_t)data[i];
    }
    return sum;
}

void subDeviceFillBytes(size_t dataCount, uint8_t *data) {
    for (size_t i = 0; i < dataCount; i++) {
        data[i] = (uint8_t)(3u + (uint32_t)i * 17u);
    }
}

void subDeviceFillShorts(size_t dataCount, uint16_t *data) {
    for (size_t i = 0; i < dataCount; i++) {
        data[i] = (uint16_t)(1000u + (uint32_t)i * 257u);
    }
}

/* ==== R6 string-view fields in pointer-passed boundary structs (§28) ==== */

uint64_t subBoundaryStringCheck(const SubBoundaryStringRecord *record, uint32_t selector) {
    if (record == NULL) {
        return UINT64_MAX;
    }
    switch (selector) {
        case 0: {
            uint64_t sum = 0u;
            if (record->label.data != NULL) {
                for (size_t i = 0; i < record->label.len; i++) {
                    sum += (uint64_t)(unsigned char)record->label.data[i];
                }
            }
            return sum;
        }
        case 1:
            return record->handle;
        case 2:
            return record->enabled ? 1u : 0u;
        case 3:
            return record->serial;
        default:
            return UINT64_MAX - 1u;
    }
}

void subBoundaryStringFill(SubBoundaryStringRecord *record, bool emptyLabel) {
    static const char label[] = "filled-by-c";
    if (record == NULL) {
        return;
    }
    memset(record, 0, sizeof(*record));
    if (!emptyLabel) {
        record->label.data = label;
        record->label.len = sizeof(label) - 1u;
    }
    record->handle = 77u;
    record->enabled = true;
    record->serial = 1234u;
    record->generation = 5678u;
}

/* ==== R7 nested aggregates + struct enum pairs (compiler.md §30) ===== */

uint64_t subProbeTextureDescriptorCheck(
    const SGPUProbeTextureDescriptor *descriptor,
    uint32_t selector) {
    if (descriptor == NULL) {
        return UINT64_MAX;
    }
    switch (selector) {
        case 0: {
            uint64_t sum = 0u;
            if (descriptor->label.data != NULL) {
                for (size_t i = 0; i < descriptor->label.len; i++) {
                    sum += (uint64_t)(unsigned char)descriptor->label.data[i];
                }
            }
            return sum;
        }
        case 1:
            return (uint64_t)descriptor->label.len;
        case 2:
            return (uint64_t)descriptor->extent.width;
        case 3:
            return (uint64_t)descriptor->extent.height;
        case 4:
            return (uint64_t)descriptor->extent.depthOrArrayLayers;
        case 5:
            return (uint64_t)descriptor->viewFormatsCount;
        case 6:
        case 7:
        case 8: {
            size_t index = (size_t)(selector - 6u);
            if (descriptor->viewFormats == NULL || index >= descriptor->viewFormatsCount) {
                return UINT64_MAX;
            }
            return (uint64_t)descriptor->viewFormats[index];
        }
        case 9:
            return (uint64_t)descriptor->format;
        case 10:
            return (uint64_t)descriptor->mipLevelCount;
        case 11:
            return (uint64_t)descriptor->sampleCount;
        case 12:
            return (uint64_t)descriptor->dimension;
        case 13:
            return (uint64_t)descriptor->usage;
        default:
            return UINT64_MAX - 1u;
    }
}

void subProbeTextureDescriptorFill(SGPUProbeTextureDescriptor *descriptor) {
    static const char label[] = "filled-r7";
    if (descriptor == NULL) {
        return;
    }
    descriptor->label.data = label;
    descriptor->label.len = sizeof(label) - 1u;
    descriptor->extent.width = 101u;
    descriptor->extent.height = 202u;
    descriptor->extent.depthOrArrayLayers = 303u;
    descriptor->format = SGPU_PROBE_FORMAT_DEPTH24;
    descriptor->mipLevelCount = 8u;
    descriptor->sampleCount = 4u;
    descriptor->dimension = 3u;
    descriptor->usage = 165u;
}

/* ==== R8 opaque handles in aggregate positions (compiler.md §31) ===== */

uint64_t subProbePipelineLayoutCheck(
    const SubProbePipelineLayoutDescriptor *descriptor,
    uint32_t selector) {
    if (descriptor == NULL) {
        return UINT64_MAX;
    }
    if (selector == 0u) {
        uint64_t sum = 0u;
        if (descriptor->label.data != NULL) {
            for (size_t i = 0; i < descriptor->label.len; i++) {
                sum += (uint64_t)(unsigned char)descriptor->label.data[i];
            }
        }
        return sum;
    }
    if (selector == 1u) {
        return (uint64_t)descriptor->label.len;
    }
    if (selector == 2u) {
        return (uint64_t)descriptor->bindGroupLayoutsCount;
    }
    size_t index = (size_t)(selector - 3u);
    if (descriptor->bindGroupLayouts == NULL
        || index >= descriptor->bindGroupLayoutsCount
        || descriptor->bindGroupLayouts[index] == NULL) {
        return UINT64_MAX;
    }
    for (size_t first = 0; first <= index; first++) {
        if (descriptor->bindGroupLayouts[first]
            == descriptor->bindGroupLayouts[index]) {
            return (uint64_t)(first + 1u);
        }
    }
    return UINT64_MAX - 1u;
}

uint32_t subProbeBindGroupEntryCheck(const SubProbeBindGroupEntry *entry) {
    if (entry == NULL) {
        return UINT32_MAX;
    }
    uint32_t mask = 0u;
    if (entry->buffer != NULL) {
        mask |= 1u;
    }
    if (entry->sampler != NULL) {
        mask |= 2u;
    }
    if (entry->textureView != NULL) {
        mask |= 4u;
    }
    return mask;
}

void subProbeBindGroupEntryFill(
    SubProbeBindGroupEntry *entry,
    uint32_t selected,
    SubDevice handle) {
    if (entry == NULL) {
        return;
    }
    entry->binding = 900u + selected;
    entry->buffer = NULL;
    entry->sampler = NULL;
    entry->textureView = NULL;
    if (selected == 0u) {
        entry->buffer = handle;
    } else if (selected == 1u) {
        entry->sampler = handle;
    } else if (selected == 2u) {
        entry->textureView = handle;
    }
}

/* ==== R9 recursive lowering at embedded positions (compiler.md §32) === */

static uint64_t subProbeViewSum(SGPUStringView view) {
    uint64_t sum = 0u;
    if (view.data != NULL) {
        for (size_t i = 0; i < view.len; i++) {
            sum += (uint64_t)(unsigned char)view.data[i];
        }
    }
    return sum;
}

uint64_t subProbeComputePipelineCheck(
    const SGPUProbeComputePipelineDescriptor *descriptor,
    uint32_t selector) {
    if (descriptor == NULL) {
        return UINT64_MAX;
    }
    switch (selector) {
        case 0: return subProbeViewSum(descriptor->label);
        case 1: return (uint64_t)descriptor->label.len;
        case 2: return subProbeViewSum(descriptor->compute.entryPoint);
        case 3: return (uint64_t)descriptor->compute.entryPoint.len;
        case 4: return (uint64_t)descriptor->compute.workgroupX;
        case 5: return (uint64_t)descriptor->compute.workgroupY;
        case 6: return descriptor->compute.constantSeed;
        case 7: return (uint64_t)descriptor->flags;
        default: return UINT64_MAX - 1u;
    }
}

uint64_t subProbeRenderPipelineCheck(
    const SGPUProbeRenderPipelineDescriptor *descriptor,
    uint32_t selector) {
    if (descriptor == NULL) {
        return UINT64_MAX;
    }
    if (selector == 0u) return subProbeViewSum(descriptor->label);
    if (selector == 1u) return (uint64_t)descriptor->label.len;
    if (selector == 2u) return (uint64_t)descriptor->vertex.moduleId;
    if (selector == 3u) return (uint64_t)descriptor->vertex.buffersCount;
    if (descriptor->vertex.buffers == NULL || descriptor->vertex.buffersCount < 2u) {
        return UINT64_MAX - 2u;
    }
    const SGPUProbeVertexBufferLayout *first = &descriptor->vertex.buffers[0];
    const SGPUProbeVertexBufferLayout *second = &descriptor->vertex.buffers[1];
    switch (selector) {
        case 4: return first->arrayStride;
        case 5: return (uint64_t)first->stepMode;
        case 6: return (uint64_t)first->attributesCount;
        case 7: return first->attributesCount > 0u && first->attributes != NULL
            ? (uint64_t)first->attributes[0].shaderLocation : UINT64_MAX;
        case 8: return first->attributesCount > 0u && first->attributes != NULL
            ? (uint64_t)first->attributes[0].format : UINT64_MAX;
        case 9: return first->attributesCount > 0u && first->attributes != NULL
            ? first->attributes[0].offset : UINT64_MAX;
        case 10: return second->arrayStride;
        case 11: return (uint64_t)second->stepMode;
        case 12: return (uint64_t)second->attributesCount;
        case 13: return second->attributesCount > 1u && second->attributes != NULL
            ? (uint64_t)second->attributes[1].shaderLocation : UINT64_MAX;
        case 14: return second->attributesCount > 1u && second->attributes != NULL
            ? (uint64_t)second->attributes[1].format : UINT64_MAX;
        case 15: return second->attributesCount > 1u && second->attributes != NULL
            ? second->attributes[1].offset : UINT64_MAX;
        case 16: return (uint64_t)descriptor->primitive;
        default: return UINT64_MAX - 1u;
    }
}

uint64_t subProbeProgrammableStageCheck(
    const SGPUProbeProgrammableStage *stage,
    uint32_t selector) {
    if (stage == NULL) {
        return UINT64_MAX;
    }
    if (selector == 0u) return (uint64_t)stage->constantsCount;
    if (selector == 7u) return (uint64_t)stage->stage;
    if (stage->constants == NULL || stage->constantsCount < 2u) {
        return UINT64_MAX - 2u;
    }
    switch (selector) {
        case 1: return subProbeViewSum(stage->constants[0].key);
        case 2: return (uint64_t)stage->constants[0].key.len;
        case 3: return (uint64_t)stage->constants[0].value;
        case 4: return subProbeViewSum(stage->constants[1].key);
        case 5: return (uint64_t)stage->constants[1].key.len;
        case 6: return (uint64_t)stage->constants[1].value;
        default: return UINT64_MAX - 1u;
    }
}

/* ==== R10 lowering through struct-pointer members (compiler.md §33) == */

uint64_t subProbeFullRenderPipelineCheck(
    const SGPUProbeFullRenderPipelineDescriptor *descriptor,
    uint32_t selector) {
    if (descriptor == NULL) {
        return UINT64_MAX;
    }
    if (selector == 0u) return subProbeViewSum(descriptor->label);
    if (selector == 1u) return (uint64_t)descriptor->label.len;
    if (selector == 2u) return descriptor->fragment == NULL ? 0u : 1u;
    if (descriptor->fragment == NULL) return 7000u + (uint64_t)selector;

    const SGPUProbeFragmentState *fragment = descriptor->fragment;
    switch (selector) {
        case 3: return subProbeViewSum(fragment->entryPoint);
        case 4: return (uint64_t)fragment->entryPoint.len;
        case 5: return (uint64_t)fragment->constantsCount;
        case 12: return (uint64_t)fragment->targetsCount;
        default: break;
    }

    if (selector >= 6u && selector <= 11u) {
        size_t index = selector >= 9u ? 1u : 0u;
        if (fragment->constants == NULL || index >= fragment->constantsCount) {
            return 8000u + (uint64_t)selector;
        }
        const SGPUProbeConstantEntry *constant = &fragment->constants[index];
        uint32_t member = selector - (index == 0u ? 6u : 9u);
        if (member == 0u) return subProbeViewSum(constant->key);
        if (member == 1u) return (uint64_t)constant->key.len;
        return (uint64_t)constant->value;
    }

    size_t target_index = selector >= 18u ? 1u : 0u;
    if (selector < 13u || selector > 22u
        || fragment->targets == NULL
        || target_index >= fragment->targetsCount) {
        return 8000u + (uint64_t)selector;
    }
    const SGPUProbeColorTargetState *target = &fragment->targets[target_index];
    uint32_t member = selector - (target_index == 0u ? 13u : 18u);
    if (member == 0u) return (uint64_t)target->format;
    if (member == 1u) return target->blend == NULL ? 0u : 1u;
    if (member == 2u) {
        return target->blend == NULL
            ? 6000u + (uint64_t)selector
            : (uint64_t)target->blend->colorOperation;
    }
    if (member == 3u) {
        return target->blend == NULL
            ? 6000u + (uint64_t)selector
            : (uint64_t)target->blend->alphaOperation;
    }
    return (uint64_t)target->writeMask;
}

/* ==== OBS-3 handle beside arrays through a nullable member (§44) ==== */

uint64_t subProbeFullRenderPipelineWithHandleCheck(
    const SGPUProbeHandleRenderPipelineDescriptor *descriptor,
    uint32_t selector) {
    if (descriptor == NULL) {
        return UINT64_MAX;
    }
    if (selector == 0u) return subProbeViewSum(descriptor->label);
    if (selector == 1u) return (uint64_t)descriptor->label.len;
    if (selector == 2u) return descriptor->fragment == NULL ? 0u : 1u;
    if (descriptor->fragment == NULL) return 9000u + (uint64_t)selector;

    const SGPUProbeHandleFragmentState *fragment = descriptor->fragment;
    switch (selector) {
        case 3: return fragment->module == NULL ? 0u : 1u;
        case 4: return subProbeViewSum(fragment->entryPoint);
        case 5: return (uint64_t)fragment->entryPoint.len;
        case 6: return (uint64_t)fragment->constantsCount;
        case 7: return (uint64_t)fragment->targetsCount;
        default: break;
    }

    if (selector >= 8u && selector <= 13u) {
        size_t index = selector >= 11u ? 1u : 0u;
        if (fragment->constants == NULL || index >= fragment->constantsCount) {
            return 8000u + (uint64_t)selector;
        }
        const SGPUProbeConstantEntry *constant = &fragment->constants[index];
        uint32_t member = selector - (index == 0u ? 8u : 11u);
        if (member == 0u) return subProbeViewSum(constant->key);
        if (member == 1u) return (uint64_t)constant->key.len;
        return (uint64_t)constant->value;
    }

    size_t target_index = selector >= 19u ? 1u : 0u;
    if (selector < 14u || selector > 23u
        || fragment->targets == NULL
        || target_index >= fragment->targetsCount) {
        return 8000u + (uint64_t)selector;
    }
    const SGPUProbeColorTargetState *target = &fragment->targets[target_index];
    uint32_t member = selector - (target_index == 0u ? 14u : 19u);
    if (member == 0u) return (uint64_t)target->format;
    if (member == 1u) return target->blend == NULL ? 0u : 1u;
    if (member == 2u) {
        return target->blend == NULL
            ? 6000u + (uint64_t)selector
            : (uint64_t)target->blend->colorOperation;
    }
    if (member == 3u) {
        return target->blend == NULL
            ? 6000u + (uint64_t)selector
            : (uint64_t)target->blend->alphaOperation;
    }
    return (uint64_t)target->writeMask;
}

/* ==== OBS-3 nested structs behind an element pointer (§44.5) ======== */

uint64_t subProbeFullRenderPipelineWithNestedBlendCheck(
    const SGPUProbeNestedRenderPipelineDescriptor *descriptor,
    uint32_t selector) {
    if (descriptor == NULL) {
        return UINT64_MAX;
    }
    if (selector == 0u) return subProbeViewSum(descriptor->label);
    if (selector == 1u) return (uint64_t)descriptor->label.len;
    if (selector == 2u) return descriptor->fragment == NULL ? 0u : 1u;
    if (descriptor->fragment == NULL) return 10000u + (uint64_t)selector;

    const SGPUProbeNestedFragmentState *fragment = descriptor->fragment;
    switch (selector) {
        case 3: return fragment->module == NULL ? 0u : 1u;
        case 4: return subProbeViewSum(fragment->entryPoint);
        case 5: return (uint64_t)fragment->entryPoint.len;
        case 6: return (uint64_t)fragment->constantsCount;
        case 7: return (uint64_t)fragment->targetsCount;
        default: break;
    }

    if (selector >= 8u && selector <= 13u) {
        size_t index = selector >= 11u ? 1u : 0u;
        if (fragment->constants == NULL || index >= fragment->constantsCount) {
            return 8000u + (uint64_t)selector;
        }
        const SGPUProbeConstantEntry *constant = &fragment->constants[index];
        uint32_t member = selector - (index == 0u ? 8u : 11u);
        if (member == 0u) return subProbeViewSum(constant->key);
        if (member == 1u) return (uint64_t)constant->key.len;
        return (uint64_t)constant->value;
    }

    size_t target_index = selector >= 23u ? 1u : 0u;
    if (selector < 14u || selector > 31u
        || fragment->targets == NULL
        || target_index >= fragment->targetsCount) {
        return 8000u + (uint64_t)selector;
    }
    const SGPUProbeNestedColorTargetState *target =
        &fragment->targets[target_index];
    uint32_t member = selector - (target_index == 0u ? 14u : 23u);
    if (member == 0u) return (uint64_t)target->format;
    if (member == 1u) return target->blend == NULL ? 0u : 1u;
    if (member >= 2u && member <= 7u) {
        if (target->blend == NULL) return 6000u + (uint64_t)selector;
        const SGPUProbeNestedBlendComponent *component =
            member <= 4u ? &target->blend->color : &target->blend->alpha;
        uint32_t component_member = member <= 4u ? member - 2u : member - 5u;
        if (component_member == 0u) return (uint64_t)component->operation;
        if (component_member == 1u) return (uint64_t)component->srcFactor;
        return (uint64_t)component->dstFactor;
    }
    return (uint64_t)target->writeMask;
}

/* ==== OBS-3 unmarked reach-through pointer (§44.6) ================= */

uint64_t subProbeFullRenderPipelineWithUnmarkedBlendCheck(
    const SGPUProbeUnmarkedRenderPipelineDescriptor *descriptor,
    uint32_t selector) {
    if (descriptor == NULL) {
        return UINT64_MAX;
    }
    if (selector == 0u) return subProbeViewSum(descriptor->label);
    if (selector == 1u) return (uint64_t)descriptor->label.len;
    if (selector == 2u) return descriptor->fragment == NULL ? 0u : 1u;
    if (descriptor->fragment == NULL) return 11000u + (uint64_t)selector;

    const SGPUProbeUnmarkedFragmentState *fragment = descriptor->fragment;
    switch (selector) {
        case 3: return fragment->module == NULL ? 0u : 1u;
        case 4: return subProbeViewSum(fragment->entryPoint);
        case 5: return (uint64_t)fragment->entryPoint.len;
        case 6: return (uint64_t)fragment->constantsCount;
        case 7: return (uint64_t)fragment->targetsCount;
        default: break;
    }

    if (selector >= 8u && selector <= 13u) {
        size_t index = selector >= 11u ? 1u : 0u;
        if (fragment->constants == NULL || index >= fragment->constantsCount) {
            return 8000u + (uint64_t)selector;
        }
        const SGPUProbeConstantEntry *constant = &fragment->constants[index];
        uint32_t member = selector - (index == 0u ? 8u : 11u);
        if (member == 0u) return subProbeViewSum(constant->key);
        if (member == 1u) return (uint64_t)constant->key.len;
        return (uint64_t)constant->value;
    }

    size_t target_index = selector >= 19u ? 1u : 0u;
    if (selector < 14u || selector > 23u
        || fragment->targets == NULL
        || target_index >= fragment->targetsCount) {
        return 8000u + (uint64_t)selector;
    }
    const SGPUProbeUnmarkedColorTargetState *target =
        &fragment->targets[target_index];
    uint32_t member = selector - (target_index == 0u ? 14u : 19u);
    if (member == 0u) return (uint64_t)target->format;
    if (member == 1u) return target->blend == NULL ? 0u : 1u;
    if (member == 2u) {
        return target->blend == NULL
            ? 6000u + (uint64_t)selector
            : (uint64_t)target->blend->colorOperation;
    }
    if (member == 3u) {
        return target->blend == NULL
            ? 6000u + (uint64_t)selector
            : (uint64_t)target->blend->alphaOperation;
    }
    return target->writeMask;
}

/* ==== OBS-3 two simultaneously-present pointer members (§44.7) ==== */

uint64_t subProbeBreadthRenderPipelineCheck(
    const SGPUProbeBreadthRenderPipelineDescriptor *descriptor,
    uint32_t selector) {
    if (descriptor == NULL) {
        return UINT64_MAX;
    }
    switch (selector) {
        case 0: return subProbeViewSum(descriptor->label);
        case 1: return (uint64_t)descriptor->label.len;
        case 2: return descriptor->depthStencil == NULL ? 0u : 1u;
        case 3: return (uint64_t)descriptor->primitive.topology;
        case 4: return (uint64_t)descriptor->primitive.stripIndexFormat;
        case 5: return descriptor->fragment == NULL ? 0u : 1u;
        default: break;
    }

    if (selector >= 6u && selector <= 10u) {
        const SGPUProbeBreadthDepthStencilState *depthStencil =
            descriptor->depthStencil;
        if (depthStencil == NULL) return 12000u + (uint64_t)selector;
        if (selector == 6u) return (uint64_t)depthStencil->limits.first;
        if (selector == 7u) return (uint64_t)depthStencil->limits.second;
        if (selector == 8u) return (uint64_t)depthStencil->biasesCount;
        size_t index = (size_t)(selector - 9u);
        if (depthStencil->biases == NULL || index >= depthStencil->biasesCount) {
            return 8000u + (uint64_t)selector;
        }
        return (uint64_t)depthStencil->biases[index];
    }

    if (selector >= 11u && selector <= 15u) {
        const SGPUProbeBreadthFragmentState *fragment = descriptor->fragment;
        if (fragment == NULL) return 13000u + (uint64_t)selector;
        if (selector == 11u) return (uint64_t)fragment->stage.first;
        if (selector == 12u) return (uint64_t)fragment->stage.second;
        if (selector == 13u) return (uint64_t)fragment->constantsCount;
        size_t index = (size_t)(selector - 14u);
        if (fragment->constants == NULL || index >= fragment->constantsCount) {
            return 8000u + (uint64_t)selector;
        }
        return (uint64_t)fragment->constants[index];
    }

    return UINT64_MAX - 1u;
}

/* ==== OBS-3 wide render-pipeline descriptor (§44.8) ================ */

static uint64_t subProbeWidePairEntryCheck(
    const SGPUProbeWidePairEntry *entries,
    size_t count,
    uint32_t slot) {
    size_t index = (size_t)(slot / 5u);
    uint32_t member = slot % 5u;
    if (entries == NULL || index >= count) return UINT64_MAX - 2u;
    const SGPUProbeWidePairEntry *entry = &entries[index];
    if (member == 0u) return subProbeViewSum(entry->key);
    if (member == 1u) return (uint64_t)entry->key.len;
    if (member == 2u) return (uint64_t)entry->valuesCount;
    size_t value_index = (size_t)(member - 3u);
    if (entry->values == NULL || value_index >= entry->valuesCount) {
        return UINT64_MAX - 3u;
    }
    return (uint64_t)entry->values[value_index];
}

static uint64_t subProbeWidePointerElementCheck(
    const SGPUProbeWidePointerElement *elements,
    size_t count,
    uint32_t slot) {
    size_t index = (size_t)(slot / 7u);
    uint32_t member = slot % 7u;
    if (elements == NULL || index >= count) return UINT64_MAX - 2u;
    const SGPUProbeWidePointerElement *element = &elements[index];
    if (member == 0u) return (uint64_t)element->kind;
    if (member == 1u) return element->payload == NULL ? 0u : 1u;
    if (element->payload == NULL) return UINT64_MAX - 3u;
    if (member == 2u) return subProbeViewSum(element->payload->label);
    if (member == 3u) return (uint64_t)element->payload->label.len;
    if (member == 4u) return (uint64_t)element->payload->valuesCount;
    size_t value_index = (size_t)(member - 5u);
    if (element->payload->values == NULL
        || value_index >= element->payload->valuesCount) {
        return UINT64_MAX - 4u;
    }
    return (uint64_t)element->payload->values[value_index];
}

uint64_t subProbeWideRenderPipelineCheck(
    const SGPUProbeWideRenderPipelineDescriptor *descriptor,
    uint32_t selector) {
    if (descriptor == NULL) return UINT64_MAX;
    switch (selector) {
        case 0: return subProbeViewSum(descriptor->label);
        case 1: return (uint64_t)descriptor->label.len;
        case 2: return descriptor->layout == NULL ? 0u : 1u;
        case 3: return subProbeViewSum(descriptor->vertex.entryPoint);
        case 4: return (uint64_t)descriptor->vertex.entryPoint.len;
        case 5: return (uint64_t)descriptor->vertex.buffersCount;
        case 16: return (uint64_t)descriptor->primitive.topology;
        case 17: return (uint64_t)descriptor->primitive.stripIndexFormat;
        case 18: return descriptor->depthStencil == NULL ? 0u : 1u;
        case 45: return (uint64_t)descriptor->multisample.count;
        case 46: return (uint64_t)descriptor->multisample.mask;
        case 47: return (uint64_t)descriptor->multisample.alphaToCoverage;
        case 48: return descriptor->fragment == NULL ? 0u : 1u;
        default: break;
    }

    if (selector >= 6u && selector <= 15u) {
        return subProbeWidePairEntryCheck(
            descriptor->vertex.buffers,
            descriptor->vertex.buffersCount,
            selector - 6u);
    }

    if (descriptor->depthStencil == NULL && selector >= 19u && selector <= 44u) {
        return 12000u + (uint64_t)selector;
    }
    if (selector == 19u) {
        return (uint64_t)descriptor->depthStencil->constantsCount;
    }
    if (selector == 20u) {
        return (uint64_t)descriptor->depthStencil->elementsCount;
    }
    if (selector >= 21u && selector <= 30u) {
        return subProbeWidePairEntryCheck(
            descriptor->depthStencil->constants,
            descriptor->depthStencil->constantsCount,
            selector - 21u);
    }
    if (selector >= 31u && selector <= 44u) {
        return subProbeWidePointerElementCheck(
            descriptor->depthStencil->elements,
            descriptor->depthStencil->elementsCount,
            selector - 31u);
    }

    if (descriptor->fragment == NULL && selector >= 49u && selector <= 77u) {
        return 13000u + (uint64_t)selector;
    }
    if (selector == 49u) return descriptor->fragment->module == NULL ? 0u : 1u;
    if (selector == 50u) return subProbeViewSum(descriptor->fragment->entryPoint);
    if (selector == 51u) return (uint64_t)descriptor->fragment->entryPoint.len;
    if (selector == 52u) return (uint64_t)descriptor->fragment->constantsCount;
    if (selector == 53u) return (uint64_t)descriptor->fragment->elementsCount;
    if (selector >= 54u && selector <= 63u) {
        return subProbeWidePairEntryCheck(
            descriptor->fragment->constants,
            descriptor->fragment->constantsCount,
            selector - 54u);
    }
    if (selector >= 64u && selector <= 77u) {
        return subProbeWidePointerElementCheck(
            descriptor->fragment->elements,
            descriptor->fragment->elementsCount,
            selector - 64u);
    }
    return UINT64_MAX - 1u;
}

/* ==== R11 handle pairs at parameter position (compiler.md §34) ===== */

uint64_t subProbeQueueSubmitCheck(
    SubDevice queue,
    size_t commandsCount,
    const SubDevice *commands,
    uint32_t selector) {
    if (selector == 0u) {
        return (uint64_t)commandsCount;
    }
    size_t index = (size_t)(selector - 1u);
    if (commands == NULL || index >= commandsCount || commands[index] == NULL) {
        return UINT64_MAX;
    }
    if (commands[index] == queue) {
        return 0u;
    }
    for (size_t first = 0; first <= index; first++) {
        if (commands[first] == commands[index]) {
            return (uint64_t)(first + 1u);
        }
    }
    return UINT64_MAX - 1u;
}

/* ==== R12 nullable handles at parameter position (compiler.md §35) = */

uint32_t subProbeSetBindGroupCheck(
    SubDevice encoder,
    SubDevice _Nullable group) {
    if (group == NULL) {
        return 0u;
    }
    return group == encoder ? 1u : 2u;
}

/* ==== OBS-4 by-value register-image packing (compiler.md §47) ===== */

void subByValueI32OneReport(
    SubByValueI32One *report,
    SubByValueI32One value) {
    if (report == NULL) return;
    report->a = value.a;
}

void subByValueI32PairReport(
    SubByValueI32Pair *report,
    SubByValueI32Pair value) {
    if (report == NULL) return;
    report->x = value.x;
    report->y = value.y;
}

void subByValueI32TripleReport(
    SubByValueI32Triple *report,
    SubByValueI32Triple value) {
    if (report == NULL) return;
    report->a = value.a;
    report->b = value.b;
    report->c = value.c;
}

void subByValueI16I16I32Report(
    SubByValueI16I16I32 *report,
    SubByValueI16I16I32 value) {
    if (report == NULL) return;
    report->a = value.a;
    report->b = value.b;
    report->c = value.c;
}

void subByValueU8FourReport(
    SubByValueU8Four *report,
    SubByValueU8Four value) {
    if (report == NULL) return;
    report->a = value.a;
    report->b = value.b;
    report->c = value.c;
    report->d = value.d;
}

void subByValueI64PairReport(
    SubByValueI64Pair *report,
    SubByValueI64Pair value) {
    if (report == NULL) return;
    report->a = value.a;
    report->b = value.b;
}

void subByValueF32Hfa2Report(
    SubByValueF32Hfa2 *report,
    SubByValueF32Hfa2 value) {
    if (report == NULL) return;
    report->a = value.a;
    report->b = value.b;
}

void subByValueF32Hfa4Report(
    SubByValueF32Hfa4 *report,
    SubByValueF32Hfa4 value) {
    if (report == NULL) return;
    report->a = value.a;
    report->b = value.b;
    report->c = value.c;
    report->d = value.d;
}

void subByValueI32F32Report(
    SubByValueI32F32 *report,
    SubByValueI32F32 value) {
    if (report == NULL) return;
    report->a = value.a;
    report->b = value.b;
}

void subByValueI32I64Report(
    SubByValueI32I64 *report,
    SubByValueI32I64 value) {
    if (report == NULL) return;
    report->a = value.a;
    report->b = value.b;
}

void subByValueI64TripleReport(
    SubByValueI64Triple *report,
    SubByValueI64Triple value) {
    if (report == NULL) return;
    report->a = value.a;
    report->b = value.b;
    report->c = value.c;
}
