#ifndef SUBSCRIPT_EXTERNAL_DEVICE_H
#define SUBSCRIPT_EXTERNAL_DEVICE_H

#include <stdint.h>

/* This header consumes the opaque handle owned by interop.h. Bindgen keeps
 * the shared spelling as a reference; the program supplies the mirror that
 * declares it. */
/* @subscript-external SubDevice */

SubDevice subExternalDeviceIdentity(SubDevice device);
uint32_t subExternalDeviceTag(SubDevice device, uint32_t tag);

#endif /* SUBSCRIPT_EXTERNAL_DEVICE_H */
