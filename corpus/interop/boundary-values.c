#include "boundary-values.h"
#include <string.h>
static uint16_t half_bits(_Float16 value) { uint16_t bits; memcpy(&bits, &value, sizeof bits); return bits; }
int32_t subBoundaryNarrow(int8_t value) { return value == -31; }
int32_t subBoundaryHalfCheck(_Float16 value) { return half_bits(value) == 0x3e00; }
_Float16 subBoundaryHalfReturn(void) { return 1.5; }
int32_t subBoundaryHalf2Check(SubBoundaryHalf2 value) { return half_bits(value.x) == 0x3e00 && half_bits(value.y) == 0x4100; }
SubBoundaryHalf2 subBoundaryHalf2Return(void) { return (SubBoundaryHalf2){1.5, 2.5}; }
SubBoundaryFloat2 subBoundaryFloat2Return(void) { return (SubBoundaryFloat2){1.5, 2.5}; }
SubBoundaryBoolPadded subBoundaryBoolReturn(void) { SubBoundaryBoolPadded value; memset(&value, 0xa5, sizeof value); value.a = false; value.b = 37; value.c = false; return value; }
uint64_t subBoundaryScriptSize(size_t dataCount, const uint8_t *data) { (void)data; return dataCount; }
