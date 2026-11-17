/* Embedded external-mirror slots follow compiler.md §325 rule 4. */
#ifndef SUBSCRIPT_EXTERNAL_USERDATA_H
#define SUBSCRIPT_EXTERNAL_USERDATA_H
#include "interop.h"
#include "external-device.h"
/* @subscript-external SubDescReadUd */
typedef struct Sub325ExternalUd { int32_t tag; SubDescReadUd inner; } Sub325ExternalUd;
void sub325ExternalKeep(Sub325ExternalUd *out);
void sub325ExternalReplace(Sub325ExternalUd *out);
#endif
