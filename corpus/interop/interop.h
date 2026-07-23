/*
 * interop.h — synthetic C interop fixture for the P5 layout proof.
 *
 * This header is invented for this project's own interop vertical slice
 * (specs/blocks/compiler.md §12). It names and depends on no external
 * project, library, or platform API; every identifier uses a synthetic
 * `Sub` prefix. It contains only C structs, enums, opaque-handle
 * typedefs, and function-pointer typedefs — no unions and no bitfields
 * (§12.1): the C-ABI layout-identity guarantee (design invariant 1) is
 * about C structs / enums / pointers / function-pointers /
 * opaque-handles only.
 *
 * It exercises, one construct per plan §4 interop pattern:
 *   1. an intrusive extension chain (embedded header + two extensions);
 *   2. a (pointer, count) array-pair descriptor;
 *   3. a length-carrying string view (no NUL assumption);
 *   4. a callback-info struct (function pointer + userdata);
 *   5. an opaque handle with create / retain / release.
 *
 * The `offsetof` proof (§12.3) mirrors every struct here that is
 * expressible as a language `@value class` and asserts byte-identical
 * layout against the platform C compiler. Function declarations carry no
 * layout and exist only so P5.2/P5.3 can build the mirror generator and
 * the linked callee on top of this fixture.
 */

#ifndef SUBSCRIPT_INTEROP_H
#define SUBSCRIPT_INTEROP_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* ---- Pattern 1: intrusive extension chain -------------------------- */

/* Type tag identifying the concrete struct behind a chain header. */
typedef enum SubChainKind {
    SUB_CHAIN_KIND_BASE = 0,
    SUB_CHAIN_KIND_EXT_A = 1,
    SUB_CHAIN_KIND_EXT_B = 2
} SubChainKind;

/* Common header embedded as the first field of every chainable struct.
 * A reader walks `next`, switching on `sType`. */
typedef struct SubChainHeader {
    SubChainKind sType;
    struct SubChainHeader *next;
} SubChainHeader;

/* Extension whose first field is the common header (structurally an
 * "is-a" chain node), followed by extra scalar payload. */
typedef struct SubChainExtA {
    SubChainHeader header;
    float intensity;
    uint32_t flags;
} SubChainExtA;

/* A second, differently sized extension: proves the chain admits more
 * than one concrete follower. */
typedef struct SubChainExtB {
    SubChainHeader header;
    double scale;
    int32_t level;
} SubChainExtB;

/* ---- Pattern 2: (pointer, count) array-pair descriptor ------------- */

/* Borrowed view over a contiguous run of elements; the callee reads
 * `count` items starting at `items` and does not take ownership. */
typedef struct SubBufferView {
    const uint32_t *items;
    size_t count;
} SubBufferView;

/* ---- Pattern 3: length-carrying string view ------------------------ */

/* UTF-8 bytes plus an explicit length; no terminating NUL is assumed. */
typedef struct SubStringView {
    const char *data;
    size_t len;
} SubStringView;

/* ---- Pattern 4: callback info (function pointer + userdata) --------- */

/* Callback receiving a string view by value plus an opaque userdata
 * pointer supplied at registration time. */
typedef void (*SubLogCallback)(SubStringView message, void *userdata);

/* Registration record: the callback plus two independent userdata
 * slots (a primary sink and an auxiliary context). */
typedef struct SubCallbackInfo {
    SubLogCallback callback;
    void *userdata;
    void *userparam;
} SubCallbackInfo;

/* ---- Alignment-exercising payload structs -------------------------- */
/* These are not one of the five patterns; they exist so the mirrored
 * set includes a fixed C array and non-trivial interior padding, making
 * the offsetof proof discriminating (§12.1, §12.3). */

/* Fixed C array (mirrors FixedArray<f32, 16>) followed by mixed scalars
 * that force interior and trailing padding. */
typedef struct SubTransform {
    float basis[16];
    int32_t bone;
    double weight;
    bool visible;
} SubTransform;

/* A minimal mixed-alignment struct: bool then f64 forces 7 bytes of
 * padding, exercising the worst-case leading gap. */
typedef struct SubSample {
    bool a;
    double b;
    int32_t c;
    float d;
} SubSample;

/* ---- Pattern 5: opaque handle with create / retain / release ------- */

/* Handle to an incomplete type: callers hold the pointer, never the
 * layout. Not mirrorable as a value class (no visible fields); it lowers
 * to a pointer-sized handle at the language boundary (P5.2). */
typedef struct SubDevice_T *SubDevice;

/* Lifecycle declarations only; a minimal implementation is P5.3's
 * concern and is not needed for the layout proof. */
SubDevice subDeviceCreate(SubChainHeader *chain);
void subDeviceRetain(SubDevice device);
void subDeviceRelease(SubDevice device);

/* Representative uses of the pattern structs, so the mirror generator
 * (P5.2) has foreign functions to bind. Declarations only. */
void subDeviceSubmit(SubDevice device, SubBufferView commands);
void subDeviceSetLogger(SubDevice device, SubCallbackInfo logger);
void subDeviceSetLabel(SubDevice device, SubStringView label);

#endif /* SUBSCRIPT_INTEROP_H */
