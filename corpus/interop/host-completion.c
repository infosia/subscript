#include "interop.h"
#include "host-completion.h"
#include <stddef.h>
#include <string.h>
typedef struct subscript_rt_context subscript_rt_context;
extern subscript_rt_context *subCompletionDeviceContext(SubDevice device);
extern int32_t subscript_rt_complete_value(subscript_rt_context *, subscript_rt_completion, const void *, size_t);
extern int32_t subscript_rt_complete_void(subscript_rt_context *, subscript_rt_completion);
extern int32_t subscript_rt_complete_error(subscript_rt_context *, subscript_rt_completion, const char *, size_t);
/* Each device owns its pending endpoints. Request ids select the pump action. */
typedef struct SubCompletionSlot {
    SubDevice device;
    subscript_rt_completion endpoint;
    int32_t request;
    int32_t kind;
} SubCompletionSlot;
static _Thread_local SubCompletionSlot slots[32];
static _Thread_local SubDevice pressure_device;
static void store(SubDevice device, int32_t request, int32_t kind, subscript_rt_completion endpoint) {
    int i;
    for (i = 0; i < 32; i++) {
        if (slots[i].device == NULL) {
            slots[i] = (SubCompletionSlot){ device, endpoint, request, kind };
            return;
        }
    }
}
void subCompletionI32(SubDevice device, int32_t request, subscript_rt_completion endpoint) {
    pressure_device = device;
    store(device, request, 0, endpoint);
}
void subCompletionStruct(SubDevice device, int32_t request, subscript_rt_completion endpoint) {
    store(device, request, 1, endpoint);
}
void subCompletionVoid(SubDevice device, int32_t request, subscript_rt_completion endpoint) {
    store(device, request, 2, endpoint);
}
void subCompletionImmediate(SubDevice device, int32_t value, subscript_rt_completion endpoint) {
    subscript_rt_complete_value(subCompletionDeviceContext(device), endpoint, &value, sizeof value);
}
int32_t subCompletionPump(SubDevice device, int32_t request, int32_t fail) {
    int i;
    for (i = 0; i < 32; i++) {
        if (slots[i].device == device && slots[i].request == request) {
            SubCompletionSlot slot = slots[i];
            slots[i].device = NULL;
            subscript_rt_context *ctx = subCompletionDeviceContext(device);
            if (fail != 0) {
                const char *message = "host failure";
                return subscript_rt_complete_error(ctx, slot.endpoint, message, strlen(message));
            }
            if (slot.kind == 2) return subscript_rt_complete_void(ctx, slot.endpoint);
            if (slot.kind == 1) {
                SubCompletionValue value = { request, request + 1 };
                return subscript_rt_complete_value(ctx, slot.endpoint, &value, sizeof value);
            }
            return subscript_rt_complete_value(ctx, slot.endpoint, &request, sizeof request);
        }
    }
    return 1;
}

void subCompletionSeven(int32_t a, int32_t b, int32_t c, int32_t d, int32_t e, int32_t f, int32_t g, subscript_rt_completion endpoint) {
    int32_t value = a + 2*b + 3*c + 4*d + 5*e + 6*f + 7*g;
    subscript_rt_complete_value(subCompletionDeviceContext(pressure_device), endpoint, &value, sizeof value);
}
