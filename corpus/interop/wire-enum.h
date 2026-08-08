#ifndef SUBSCRIPT_WIRE_ENUM_H
#define SUBSCRIPT_WIRE_ENUM_H

#include <stdint.h>

typedef int32_t SubWireModeC;
/* @subscript-cenum SubWireModeC SubWireMode */

typedef enum SubBindToneC {
    SUB_BIND_TONE_QUIET = -3,
    SUB_BIND_TONE_STEADY = 9,
    SUB_BIND_TONE_BRIGHT = 0x2a,
} SubBindToneC;
/* @subscript-cenum SubBindToneC SubBindTone */

SubWireModeC subWireModeNext(void);
int32_t subWireModeEcho(SubWireModeC value);
SubWireModeC subWireModeUnknown(void);

SubBindToneC subBindToneNext(void);
int32_t subBindToneEcho(SubBindToneC value);

#endif
