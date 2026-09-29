/* Static dev fixtures expose library functions only. Ship hosts read the
 * current program's entry declarations and membership flags. */
#ifndef SUBSCRIPT_INTEROP_LIBRARY_ONLY
#include "program.h"
#endif

#include "wire-enum.h"

typedef struct subscript_rt_context subscript_rt_context;

#ifdef SUBSCRIPT_HOST_ENTRY_configure
void subWireEntryDrive(subscript_rt_context *ctx) {
    subscript_export_configure(ctx, 23, 5);
}

void subWireEntryDriveUnknown(subscript_rt_context *ctx) {
    subscript_export_configure(ctx, 12345, 5);
}

#endif

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

int32_t subWireModeRecordEchoMode(const SubWireModeRecord *value) {
    return value->mode;
}

int32_t subWireModeRecordEchoTone(SubWireModeRecord value) {
    return value.tone;
}

int32_t subWireModeRecordEchoElement(const SubWireModeRecord *value, uint32_t index) {
    if (index >= value->modesCount) {
        return 0;
    }
    return value->modes[index];
}

void subWireModeRecordFill(SubWireModeRecord *value) {
    value->tag = 88;
    value->mode = 23;
    value->tone = SUB_BIND_TONE_STEADY;
    value->modesCount = 0;
    value->modes = 0;
    value->serial = 900;
}

void subWireModeRecordFillUnknown(SubWireModeRecord *value) {
    value->tag = 99;
    value->mode = 12345;
    value->tone = SUB_BIND_TONE_QUIET;
    value->modesCount = 0;
    value->modes = 0;
    value->serial = 901;
}
