#include "wire-enum.h"

SubWireModeC subWireModeNext(void) {
    return 23;
}

int32_t subWireModeEcho(SubWireModeC value) {
    return value;
}

SubWireModeC subWireModeUnknown(void) {
    return 12345;
}

SubBindToneC subBindToneNext(void) {
    return SUB_BIND_TONE_STEADY;
}

int32_t subBindToneEcho(SubBindToneC value) {
    return value;
}
