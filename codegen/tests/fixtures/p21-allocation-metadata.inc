
/* Allocation attribution tables. Generated from checked HIR and the
* exact pos_id sequence above; consume through the generated
* allocation metadata header. */
typedef struct { uint32_t class_id; const char *name; } sub_alloc_class_info;
typedef struct { const char *file; uint32_t line; uint32_t column; } sub_alloc_position_info;

const sub_alloc_class_info ss_alloc_classes[] = {
    { 4294967041u, "string" },
    { 4294967042u, "Array" },
    { 4294967043u, "ArrayData" },
    { 4294967044u, "GeneratorFrame" },
    { 4294967045u, "Map" },
    { 4294967046u, "Set" },
    { 4294967047u, "MapData" },
    { 4294967048u, "MapIndex" },
    { 0u, "Counter" },
};
const uint64_t ss_alloc_class_count = 9u;

const sub_alloc_position_info ss_alloc_positions[] = {
    { "a15-manual-lifetime.ts", 19u, 28u },
    { "a15-manual-lifetime.ts", 21u, 20u },
    { "a15-manual-lifetime.ts", 22u, 3u },
};
const uint64_t ss_alloc_position_count = 3u;
