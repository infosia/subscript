#include "interop.h"
#include "host-buffer-completion.h"
#include <stddef.h>
#include <string.h>
#include <assert.h>
typedef struct subscript_rt_context subscript_rt_context;
extern subscript_rt_context *subCompletionDeviceContext(SubDevice device);
extern int32_t subscript_rt_complete_string(subscript_rt_context *, subscript_rt_completion, const char *, size_t);
extern int32_t subscript_rt_complete_bytes(subscript_rt_context *, subscript_rt_completion, const uint8_t *, size_t);
extern int32_t subscript_rt_complete_error(subscript_rt_context *, subscript_rt_completion, const char *, size_t);
void subCompletionText(SubDevice device, int32_t empty, subscript_rt_completion endpoint) {
    char bytes[] = "hello";
    int32_t status = subscript_rt_complete_string(subCompletionDeviceContext(device), endpoint, empty ? NULL : bytes, empty ? 0 : 5);
    assert(status == 0);
    (void)status;
    memset(bytes, 'x', 5);
}
void subCompletionBytes(SubDevice device, int32_t empty, subscript_rt_completion endpoint) {
    uint8_t bytes[] = {0, 128, 255};
    int32_t status = subscript_rt_complete_bytes(subCompletionDeviceContext(device), endpoint, empty ? NULL : bytes, empty ? 0 : 3);
    assert(status == 0);
    (void)status;
    memset(bytes, 0, 3);
}
void subCompletionTextError(SubDevice device, subscript_rt_completion endpoint) {
    char message[] = "buffer failure";
    int32_t status = subscript_rt_complete_error(subCompletionDeviceContext(device), endpoint, message, 14);
    assert(status == 0);
    (void)status;
    memset(message, 'x', 14);
}
