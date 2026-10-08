#ifndef SUBSCRIPT_ABI_PRESSURE_H
#define SUBSCRIPT_ABI_PRESSURE_H
#include "interop.h"
#include "host-completion.h"
typedef struct SubPressureValue { uint64_t x; uint64_t y; } SubPressureValue;
int32_t subPressureEndpoint0(subscript_rt_completion value, int32_t tail);
int32_t subPressureEndpoint1(int32_t a0, subscript_rt_completion value, int32_t tail);
int32_t subPressureEndpoint2(int32_t a0, int32_t a1, subscript_rt_completion value, int32_t tail);
int32_t subPressureEndpoint3(int32_t a0, int32_t a1, int32_t a2, subscript_rt_completion value, int32_t tail);
int32_t subPressureEndpoint4(int32_t a0, int32_t a1, int32_t a2, int32_t a3, subscript_rt_completion value, int32_t tail);
int32_t subPressureEndpoint5(int32_t a0, int32_t a1, int32_t a2, int32_t a3, int32_t a4, subscript_rt_completion value, int32_t tail);
int32_t subPressureEndpoint6(int32_t a0, int32_t a1, int32_t a2, int32_t a3, int32_t a4, int32_t a5, subscript_rt_completion value, int32_t tail);
int32_t subPressureEndpoint7(int32_t a0, int32_t a1, int32_t a2, int32_t a3, int32_t a4, int32_t a5, int32_t a6, subscript_rt_completion value, int32_t tail);
int32_t subPressureEndpoint8(int32_t a0, int32_t a1, int32_t a2, int32_t a3, int32_t a4, int32_t a5, int32_t a6, int32_t a7, subscript_rt_completion value, int32_t tail);
int32_t subPressureValue0(SubPressureValue value, int32_t tail);
int32_t subPressureValue1(int32_t a0, SubPressureValue value, int32_t tail);
int32_t subPressureValue2(int32_t a0, int32_t a1, SubPressureValue value, int32_t tail);
int32_t subPressureValue3(int32_t a0, int32_t a1, int32_t a2, SubPressureValue value, int32_t tail);
int32_t subPressureValue4(int32_t a0, int32_t a1, int32_t a2, int32_t a3, SubPressureValue value, int32_t tail);
int32_t subPressureValue5(int32_t a0, int32_t a1, int32_t a2, int32_t a3, int32_t a4, SubPressureValue value, int32_t tail);
int32_t subPressureValue6(int32_t a0, int32_t a1, int32_t a2, int32_t a3, int32_t a4, int32_t a5, SubPressureValue value, int32_t tail);
int32_t subPressureValue7(int32_t a0, int32_t a1, int32_t a2, int32_t a3, int32_t a4, int32_t a5, int32_t a6, SubPressureValue value, int32_t tail);
int32_t subPressureValue8(int32_t a0, int32_t a1, int32_t a2, int32_t a3, int32_t a4, int32_t a5, int32_t a6, int32_t a7, SubPressureValue value, int32_t tail);

typedef struct SubPressureNarrow { int32_t x; int32_t y; int32_t z; } SubPressureNarrow;
int32_t subPressureNarrow8(int32_t a0, int32_t a1, int32_t a2, int32_t a3, int32_t a4, int32_t a5, int32_t a6, int32_t a7, SubPressureNarrow value, int32_t tail);
int32_t subPressureNarrow9(int32_t a0, int32_t a1, int32_t a2, int32_t a3, int32_t a4, int32_t a5, int32_t a6, int32_t a7, int32_t a8, SubPressureNarrow value, int32_t tail);
#endif
