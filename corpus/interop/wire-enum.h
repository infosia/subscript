#ifndef SUBSCRIPT_WIRE_ENUM_H
#define SUBSCRIPT_WIRE_ENUM_H

#include <stddef.h>
#include <stdint.h>

typedef int32_t SubWireModeC;
/* @subscript-cenum SubWireModeC SubWireMode */

typedef enum SubBindToneC {
    SUB_BIND_TONE_QUIET = -3,
    SUB_BIND_TONE_STEADY = 9,
    SUB_BIND_TONE_BRIGHT = 0x2a,
} SubBindToneC;
/* @subscript-cenum SubBindToneC SubBindTone */

typedef struct SubWireModeRecord {
    int32_t tag;
    SubWireModeC mode;
    SubBindToneC tone;
    size_t modesCount;
    const SubWireModeC *modes;
    uint32_t serial;
} SubWireModeRecord;

SubWireModeC subWireModeNext(void);
int32_t subWireModeEcho(SubWireModeC value);
SubWireModeC subWireModeUnknown(void);

SubBindToneC subBindToneNext(void);
int32_t subBindToneEcho(SubBindToneC value);

int32_t subWireModeRecordEchoMode(const SubWireModeRecord *value);
int32_t subWireModeRecordEchoTone(SubWireModeRecord value);
int32_t subWireModeRecordEchoElement(const SubWireModeRecord *value, uint32_t index);
void subWireModeRecordFill(SubWireModeRecord *value);
void subWireModeRecordFillUnknown(SubWireModeRecord *value);

#endif
