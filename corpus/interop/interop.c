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
    size_t label_len;
    char label[128];
};

/* Deterministic scratch used to synthesize a callback message of a given
 * length. Single-threaded; refilled on each use. */
static char sub_msgbuf[256];

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
    if (device->cb != NULL) {
        SubStringView msg;
        msg.data = device->label;
        msg.len = device->label_len;
        device->cb(msg, device->cb_userdata);
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
        if (n > (long long)sizeof(sub_msgbuf)) {
            n = (long long)sizeof(sub_msgbuf);
        }
        memset(sub_msgbuf, 'x', (size_t)n);
        SubStringView msg;
        msg.data = sub_msgbuf;
        msg.len = (size_t)n;
        device->cb(msg, device->cb_userdata);
    }
}
