#include "interop.h"
#include "external-device.h"

SubDevice subExternalDeviceIdentity(SubDevice device) {
    return device;
}

uint32_t subExternalDeviceTag(SubDevice device, uint32_t tag) {
    return device == NULL ? 0 : tag;
}
