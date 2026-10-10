#ifndef SUBSCRIPT_EXTERNAL_DEVICE_H
#define SUBSCRIPT_EXTERNAL_DEVICE_H

#include <stdint.h>

/* This header consumes the opaque handle owned by interop.h. Bindgen keeps
 * the shared spelling as a reference; the program supplies the mirror that
 * declares it. */
/* @subscript-external SubDevice */

SubDevice subExternalDeviceIdentity(SubDevice device);
uint32_t subExternalDeviceTag(SubDevice device, uint32_t tag);

/* A struct that copies its bytes and holds a userdata slot. Where the
 * script reads C's bytes, the slot has no read lowering (compiler.md §187
 * rule 8). No function here passes it; the reject corpus embeds it in a
 * second mirror (§48). */
typedef struct SubDescReadUd {
    int32_t k;
    void *ud;
} SubDescReadUd;

#endif /* SUBSCRIPT_EXTERNAL_DEVICE_H */
