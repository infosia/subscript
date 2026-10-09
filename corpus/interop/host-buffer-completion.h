#ifndef SUBSCRIPT_HOST_BUFFER_COMPLETION_H
#define SUBSCRIPT_HOST_BUFFER_COMPLETION_H
#include <stdint.h>
/* @subscript-external SubDevice */
#ifndef SUBSCRIPT_RUNTIME_H
typedef struct subscript_rt_completion {
    uint64_t context_id;
    uint64_t operation_id;
} subscript_rt_completion;
#endif
void subCompletionText(SubDevice device, int32_t empty, subscript_rt_completion endpoint);
void subCompletionBytes(SubDevice device, int32_t empty, subscript_rt_completion endpoint);
void subCompletionTextError(SubDevice device, subscript_rt_completion endpoint);
#endif
