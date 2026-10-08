#ifndef SUBSCRIPT_BOUNDARY_VALUES_H
#define SUBSCRIPT_BOUNDARY_VALUES_H
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
typedef struct SubBoundaryHalf2 { _Float16 x; _Float16 y; } SubBoundaryHalf2;
typedef struct SubBoundaryFloat2 { float x; float y; } SubBoundaryFloat2;
typedef struct SubBoundaryBoolPadded { bool a; int32_t b; bool c; } SubBoundaryBoolPadded;
int32_t subBoundaryNarrow(int8_t value);
int32_t subBoundaryHalfCheck(_Float16 value);
_Float16 subBoundaryHalfReturn(void);
int32_t subBoundaryHalf2Check(SubBoundaryHalf2 value);
SubBoundaryHalf2 subBoundaryHalf2Return(void);
SubBoundaryFloat2 subBoundaryFloat2Return(void);
SubBoundaryBoolPadded subBoundaryBoolReturn(void);
uint64_t subBoundaryScriptSize(size_t dataCount, const uint8_t *data);
#endif
