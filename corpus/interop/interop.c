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

#include "interop.h"

#include <stdlib.h>
#include <string.h>

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
