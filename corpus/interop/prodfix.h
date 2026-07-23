/*
 * prodfix.h — neutral production-C fixture for the P6.1 libclang frontend
 * (specs/blocks/compiler.md §13.1).
 *
 * It carries the real-C features the P5 synthetic fixture (interop.h)
 * lacked, so the libclang frontend can be shown to ingest them:
 * object-like and function-like #define macros; an attribute macro
 * (visibility) applied to a declaration; a nullability attribute macro
 * that expands to nothing; Doxygen-style doc comments; static const
 * integer constants; a flag typedef (uint64_t alias plus static const
 * members); nested structs; a function-pointer typedef; and an intrusive
 * chain struct.
 *
 * It names and depends on no external project, library, or platform API;
 * every identifier uses a synthetic `Sub` prefix. P6.1 proves the parser
 * ingests these shapes; mapping the new shapes to the boundary mirror
 * (embedded arrays, flags) is P6.2, so no committed golden mirror is
 * produced for this header yet.
 */

#ifndef SUBSCRIPT_PRODFIX_H
#define SUBSCRIPT_PRODFIX_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* Object-like and function-like macros. */
#define SUB_VERSION 5
#define SUB_MAKE_VERSION(major, minor) (((major) << 16) | (minor))

/* Attribute macros. SUB_EXPORT expands to a visibility attribute applied
 * to a declaration; SUB_NULLABLE expands to nothing (a nullability
 * annotation the frontend ignores after preprocessing). */
#define SUB_EXPORT __attribute__((visibility("default")))
#define SUB_NULLABLE

/* File-scope `static const` integer constants. */
static const int SUB_MAX_ATTACHMENTS = 8;
static const uint32_t SUB_DEFAULT_MASK = 0xFFu;

/* Flag typedef: a uint64 alias plus `static const` members combinable
 * with `|` (the P6.2 flag shape; here only parsed). */
typedef uint64_t SubFlags;
static const SubFlags SUB_FLAG_NONE = 0x0;
static const SubFlags SUB_FLAG_READ = 0x1;
static const SubFlags SUB_FLAG_WRITE = 0x2;
static const SubFlags SUB_FLAG_EXEC = 0x4;

/* An enum with running values. */
typedef enum SubStatus {
    SUB_STATUS_OK = 0,
    SUB_STATUS_RETRY = 1,
    SUB_STATUS_FATAL = 2
} SubStatus;

/* Nested struct: SubExtent is embedded by value inside SubImageInfo. */
typedef struct SubExtent {
    uint32_t width;
    uint32_t height;
    uint32_t depth;
} SubExtent;

typedef struct SubImageInfo {
    SubExtent extent;
    uint32_t mipLevels;
    SubFlags usage;
} SubImageInfo;

/* Intrusive-chain struct: a common header carrying a tag plus a self
 * pointer to the next node. */
typedef struct SubNodeHeader {
    SubStatus kind;
    struct SubNodeHeader *next;
} SubNodeHeader;

/* Function-pointer typedef. */
typedef void (*SubAllocCallback)(size_t size, void *userdata);

/** Creates an image from a descriptor and reports a status. The out
 *  parameter may be null, in which case only validation is performed. */
SUB_EXPORT SubStatus subImageCreate(const SubImageInfo *info,
                                    SubImageInfo *SUB_NULLABLE out);

/* Plain declarations, one attribute-tagged and one by-value parameter. */
SUB_EXPORT void subImageDestroy(SubImageInfo *info);
uint32_t subExtentVolume(SubExtent extent);

#endif /* SUBSCRIPT_PRODFIX_H */
