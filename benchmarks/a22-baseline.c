/* Hand-written C baseline for the P4 performance gate
 * (specs/blocks/compiler.md sections 3 and 9).
 *
 * It mirrors the corpus entry `accept/a22-matrix-propagation`
 * (corpus/accept/a22-matrix-propagation.ts) statement for statement:
 * same N, same iteration count, same LCG seed and sequence, same f32
 * arithmetic, same checksum. Its stdout must equal
 * corpus/accept/a22-matrix-propagation.expected byte for byte; a
 * mismatch means this file is wrong, never the frozen golden.
 *
 * Build: cc -O2 -ffp-contract=off. The `-O2` is the criterion's
 * (section 3); `-ffp-contract=off` keeps the baseline's f32 arithmetic
 * identical to the language's, which never contracts a multiply-add
 * into an FMA. It is not a handicap: the contracting build of this
 * workload is the slower of the two, and the harness verifies the
 * printed bytes either way.
 *
 * stdout: the checksum, formatted by shortest round-trip, plus '\n'.
 * stderr: `warmup <iterations> <ns>`, one `sample <index> <ns>` line
 * per timed run, then `checksum-stable <0|1>`.
 *
 * Timed span: the workload only -- array construction, the 100
 * propagation iterations, and the checksum -- measured inside the
 * process with CLOCK_MONOTONIC. Seeding the LCG, releasing memory,
 * and formatting/printing the result are outside the span, matching
 * what the script tiers time (the script's global initializer and its
 * Context teardown are likewise outside their span).
 */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#if defined(_WIN32)
#include <io.h>
#include <fcntl.h>
#include <windows.h>
#endif

enum { NODE_COUNT = 10000 };
enum { ITERATION_COUNT = 100 };

#define LCG_SEED 0x12345678u

/* The one module-level variable of the corpus entry. */
static uint32_t lcg_state = LCG_SEED;

/* `@CStruct class Matrix4 { elements: FixedArray<f32, 16>; }` */
typedef struct {
    float elements[16];
} Matrix4;

static uint32_t next_u32(void) {
    lcg_state = lcg_state * 1664525u + 1013904223u;
    return lcg_state;
}

static float next_f32(void) {
    return (float)next_u32() / 4294967296.0f;
}

static Matrix4 identity_with_translation(float x, float y, float z) {
    Matrix4 m;
    m.elements[0] = 1.0f;  m.elements[1] = 0.0f;  m.elements[2] = 0.0f;  m.elements[3] = 0.0f;
    m.elements[4] = 0.0f;  m.elements[5] = 1.0f;  m.elements[6] = 0.0f;  m.elements[7] = 0.0f;
    m.elements[8] = 0.0f;  m.elements[9] = 0.0f;  m.elements[10] = 1.0f; m.elements[11] = 0.0f;
    m.elements[12] = x;    m.elements[13] = y;    m.elements[14] = z;    m.elements[15] = 1.0f;
    return m;
}

static Matrix4 make_local_matrix(void) {
    const float x = (next_f32() - 0.5f) * 0.01f;
    const float y = (next_f32() - 0.5f) * 0.01f;
    const float z = (next_f32() - 0.5f) * 0.01f;
    return identity_with_translation(x, y, z);
}

static Matrix4 multiply(const Matrix4 *left, const Matrix4 *right) {
    float result[16];
    for (int32_t i = 0; i < 16; i += 1) {
        result[i] = 0.0f;
    }
    for (int32_t row = 0; row < 4; row += 1) {
        for (int32_t column = 0; column < 4; column += 1) {
            float cell = 0.0f;
            for (int32_t inner = 0; inner < 4; inner += 1) {
                cell += left->elements[row * 4 + inner] * right->elements[inner * 4 + column];
            }
            result[row * 4 + column] = cell;
        }
    }
    Matrix4 out;
    memcpy(out.elements, result, sizeof result);
    return out;
}

static void perturb_locals(Matrix4 *local, int32_t count, int32_t iteration) {
    for (int32_t index = 0; index < count; index += 1) {
        Matrix4 matrix = local[index];
        const int32_t phase = (iteration + index) % 17;
        const float delta = ((float)phase - 8.0f) * 0.000001f;
        matrix.elements[12] += delta;
        matrix.elements[13] -= delta * 0.5f;
        local[index] = matrix;
    }
}

static void propagate(const Matrix4 *local, Matrix4 *world, const int32_t *parent, int32_t count) {
    world[0] = local[0];
    for (int32_t index = 1; index < count; index += 1) {
        world[index] = multiply(&world[parent[index]], &local[index]);
    }
}

static float checksum(const Matrix4 *matrices, int32_t count) {
    float total = 0.0f;
    for (int32_t matrix_index = 0; matrix_index < count; matrix_index += 1) {
        for (int32_t element_index = 0; element_index < 16; element_index += 1) {
            total += matrices[matrix_index].elements[element_index];
        }
    }
    return total;
}

/* Nanoseconds since an unspecified monotonic epoch. The MSVC UCRT has no
 * clock_gettime/CLOCK_MONOTONIC, so on Windows the same monotonic span is
 * read from QueryPerformanceCounter, converted to nanoseconds by exact
 * integer arithmetic (no precision loss for the reported timed span). The
 * POSIX path is unchanged on every other platform. */
#if defined(_WIN32)
static uint64_t monotonic_ns(void) {
    LARGE_INTEGER freq;
    LARGE_INTEGER counter;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&counter);
    const uint64_t f = (uint64_t)freq.QuadPart;
    const uint64_t c = (uint64_t)counter.QuadPart;
    return (c / f) * 1000000000ull + (c % f) * 1000000000ull / f;
}
#else
static uint64_t monotonic_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}
#endif

/* One workload run: the body of the corpus entry's `main` up to (but
 * not including) the `print`. Returns the checksum and writes the
 * elapsed nanoseconds of the timed span to `ns`. */
static float run_once(uint64_t *ns) {
    lcg_state = LCG_SEED;

    const uint64_t start = monotonic_ns();

    Matrix4 *local = (Matrix4 *)malloc((size_t)NODE_COUNT * sizeof(Matrix4));
    Matrix4 *world = (Matrix4 *)malloc((size_t)NODE_COUNT * sizeof(Matrix4));
    int32_t *parent = (int32_t *)malloc((size_t)NODE_COUNT * sizeof(int32_t));
    if (local == NULL || world == NULL || parent == NULL) {
        free(local);
        free(world);
        free(parent);
        *ns = 0;
        return 0.0f / 0.0f;
    }

    for (int32_t index = 0; index < NODE_COUNT; index += 1) {
        local[index] = make_local_matrix();
        world[index] = identity_with_translation(0.0f, 0.0f, 0.0f);
        if (index == 0) {
            parent[index] = -1;
        } else {
            parent[index] = (int32_t)(next_u32() % (uint32_t)index);
        }
    }

    for (int32_t iteration = 0; iteration < ITERATION_COUNT; iteration += 1) {
        perturb_locals(local, NODE_COUNT, iteration);
        propagate(local, world, parent, NODE_COUNT);
    }

    const float result = checksum(world, NODE_COUNT);

    const uint64_t end = monotonic_ns();
    *ns = end - start;

    free(local);
    free(world);
    free(parent);
    return result;
}

/* Shortest round-tripping decimal spelling of an f32, the rule the
 * language's template-literal formatting uses (collisions.md Q14). */
static void format_f32(float value, char *buffer, size_t size) {
    for (int precision = 1; precision <= 9; precision += 1) {
        snprintf(buffer, size, "%.*g", precision, (double)value);
        if (strtof(buffer, NULL) == value) {
            return;
        }
    }
    snprintf(buffer, size, "%.9g", (double)value);
}

int main(int argc, char **argv) {
#if defined(_WIN32)
    /* stdout is compared byte-for-byte against the LF golden; the MSVCRT
     * opens stdout in text mode and would translate '\n' to '\r\n'.
     * Binary mode writes the bytes through unchanged. No-op off Windows. */
    _setmode(_fileno(stdout), _O_BINARY);
#endif
    int warmup = 3;
    int timed = 11;
    uint64_t warmup_floor_ns = 0;
    int report_warmup = 0;
    if (argc >= 3) {
        warmup = atoi(argv[1]);
        timed = atoi(argv[2]);
    }
    if (argc >= 4) {
        warmup_floor_ns = (uint64_t)strtoull(argv[3], NULL, 10);
        report_warmup = 1;
    }
    if (warmup < 0 || timed < 1) {
        fprintf(stderr, "usage: a22-baseline <warmup-runs> <timed-runs> [warmup-floor-ns]\n");
        return 2;
    }

    uint64_t ns = 0;
    float first = 0.0f;
    int stable = 1;
    int warmup_iterations = 0;
    uint64_t warmup_elapsed_ns = 0;

    while (warmup_iterations < warmup || warmup_elapsed_ns < warmup_floor_ns) {
        const float v = run_once(&ns);
        if (warmup_iterations == 0) {
            first = v;
        } else if (v != first) {
            stable = 0;
        }
        warmup_elapsed_ns += ns;
        warmup_iterations += 1;
    }
    if (report_warmup) {
        fprintf(
            stderr,
            "warmup %d %llu\n",
            warmup_iterations,
            (unsigned long long)warmup_elapsed_ns
        );
    }
    for (int i = 0; i < timed; i += 1) {
        const float v = run_once(&ns);
        if (warmup_iterations == 0 && i == 0) {
            first = v;
        } else if (v != first) {
            stable = 0;
        }
        fprintf(stderr, "sample %d %llu\n", i, (unsigned long long)ns);
    }
    fprintf(stderr, "checksum-stable %d\n", stable);

    char text[64];
    format_f32(first, text, sizeof text);
    printf("%s\n", text);
    fflush(stdout);
    return 0;
}
