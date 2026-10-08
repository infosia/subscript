#ifndef SUBSCRIPT_HOST_COMPLETION_H
#define SUBSCRIPT_HOST_COMPLETION_H
#include <stdint.h>
/* @subscript-external SubDevice */
#ifndef SUBSCRIPT_RUNTIME_H
typedef struct subscript_rt_completion {
    uint64_t context_id;
    uint64_t operation_id;
} subscript_rt_completion;
#endif
typedef struct SubCompletionValue { int32_t x; int32_t y; } SubCompletionValue;
void subCompletionI32(SubDevice device, int32_t request, subscript_rt_completion endpoint);
void subCompletionStruct(SubDevice device, int32_t request, subscript_rt_completion endpoint);
void subCompletionVoid(SubDevice device, int32_t request, subscript_rt_completion endpoint);
void subCompletionImmediate(SubDevice device, int32_t value, subscript_rt_completion endpoint);
void subCompletionSeven(int32_t a, int32_t b, int32_t c, int32_t d, int32_t e, int32_t f, int32_t g, subscript_rt_completion endpoint);
int32_t subCompletionPump(SubDevice device, int32_t request, int32_t fail);
#endif
