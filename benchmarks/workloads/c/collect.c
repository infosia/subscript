/* benchmark: collect (C baseline)
 * Build six graphs of 20000 48-byte nodes from the fixed LCG. Each node
 * owns four unique string payloads whose lengths are 9/41/105/233 bytes, so
 * the analogous subscript requests (8-byte length + bytes) are deliberately
 * one byte past the 16/48/112/240-byte size-class payload capacities.
 *
 * Nodes with (state & 3) != 0 survive (exactly 15000 per round); the other
 * chain and the previous round's survivor chain are explicitly freed at the
 * reclaim point. This is C's honest manual-lifetime analogue to collection.
 * Checksum, over each reverse-built survivor chain:
 *   checksum = checksum * 31 + state + 9 + 41 + 105 + 233 (i32 wrap).
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

enum {
    COUNT = 20000,
    ROUNDS = 6,
    LEN_9 = 9,
    LEN_41 = 41,
    LEN_105 = 105,
    LEN_233 = 233
};

typedef struct Node {
    int32_t value;
    unsigned char *s9;
    unsigned char *s41;
    unsigned char *s105;
    unsigned char *s233;
    struct Node *next;
} Node;

_Static_assert(sizeof(Node) == 48, "collect node layout must stay pinned at 48 bytes");

static unsigned char *make_string(int32_t uid, size_t len, unsigned char pad) {
    char suffix[32];
    int written = snprintf(suffix, sizeof(suffix), "%d", uid);
    if (written < 0 || (size_t)written > len) {
        return NULL;
    }
    unsigned char *payload = (unsigned char *)malloc(8 + len);
    if (payload == NULL) {
        return NULL;
    }
    uint64_t length = (uint64_t)len;
    memcpy(payload, &length, sizeof(length));
    memset(payload + 8, pad, len - (size_t)written);
    memcpy(payload + 8 + len - (size_t)written, suffix, (size_t)written);
    return payload;
}

static void free_node(Node *node) {
    free(node->s233);
    free(node->s105);
    free(node->s41);
    free(node->s9);
    free(node);
}

static void free_chain(Node *node) {
    while (node != NULL) {
        Node *next = node->next;
        free_node(node);
        node = next;
    }
}

static Node *make_node(int32_t uid, int32_t value, Node *next) {
    Node *node = (Node *)malloc(sizeof(Node));
    if (node == NULL) {
        return NULL;
    }
    node->value = value;
    node->s9 = make_string(uid, LEN_9, (unsigned char)'a');
    node->s41 = make_string(uid, LEN_41, (unsigned char)'b');
    node->s105 = make_string(uid, LEN_105, (unsigned char)'c');
    node->s233 = make_string(uid, LEN_233, (unsigned char)'d');
    node->next = next;
    if (node->s9 == NULL || node->s41 == NULL || node->s105 == NULL ||
        node->s233 == NULL) {
        free(node->s233);
        free(node->s105);
        free(node->s41);
        free(node->s9);
        free(node);
        return NULL;
    }
    return node;
}

static int32_t string_length(const unsigned char *payload) {
    uint64_t len = 0;
    memcpy(&len, payload, sizeof(len));
    return (int32_t)len;
}

static int32_t workload(void) {
    int32_t state = (int32_t)0x12345678u;
    int32_t checksum = 0;
    Node *keep = NULL;

    for (int32_t round = 0; round < ROUNDS; round++) {
        Node *previous = keep;
        Node *dropped = NULL;
        keep = NULL;

        for (int32_t i = 0; i < COUNT; i++) {
            state = state * 1664525 + 1013904223;
            int32_t uid = round * COUNT + i;
            Node **head = ((state & 3) != 0) ? &keep : &dropped;
            Node *node = make_node(uid, state, *head);
            if (node == NULL) {
                free_chain(previous);
                free_chain(dropped);
                free_chain(keep);
                return 0;
            }
            *head = node;
        }

        /* The explicit-free analogue of dropping both unreachable roots and
         * forcing collection. The current keep chain remains live. */
        free_chain(previous);
        free_chain(dropped);

        for (const Node *node = keep; node != NULL; node = node->next) {
            checksum = checksum * 31 + node->value;
            checksum = checksum + string_length(node->s9);
            checksum = checksum + string_length(node->s41);
            checksum = checksum + string_length(node->s105);
            checksum = checksum + string_length(node->s233);
        }
    }

    /* Mirrors dropping the final survivor root and collecting once more so
     * repeated samples begin with an empty subject heap. */
    free_chain(keep);
    return checksum;
}

#if defined(_WIN32)
#include <windows.h>
static double now_seconds(void) {
    LARGE_INTEGER freq, counter;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&counter);
    return (double)counter.QuadPart / (double)freq.QuadPart;
}
#else
static double now_seconds(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec + (double)ts.tv_nsec * 1e-9;
}
#endif

static int cmp_double(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return (x > y) - (x < y);
}

int main(int argc, char **argv) {
    int warmup = 3, timed = 11;
    if (argc >= 3) {
        warmup = atoi(argv[1]);
        timed = atoi(argv[2]);
    }
    if (warmup < 0 || timed < 1) {
        fprintf(stderr, "usage: %s <warmup> <timed>\n", argv[0]);
        return 2;
    }
    double *times = (double *)malloc((size_t)timed * sizeof(double));
    if (times == NULL) {
        return 2;
    }
    int32_t checksum = 0;
    int warmup_iterations = 0;
    double warmup_seconds = 0.0;
    while (warmup_iterations < warmup || warmup_iterations < 3 ||
           warmup_seconds < 0.200) {
        double t0 = now_seconds();
        checksum = workload();
        double t1 = now_seconds();
        warmup_seconds += t1 - t0;
        warmup_iterations++;
    }
    /* This volatile sink makes the warm-up result observable so the optimizer
     * cannot delete the contractually required warm-up as dead code. */
    volatile int32_t warmup_sink = checksum;
    (void)warmup_sink;
    for (int i = 0; i < timed; i++) {
        double t0 = now_seconds();
        checksum = workload();
        double t1 = now_seconds();
        times[i] = t1 - t0;
    }
    fprintf(stderr, "warmup %d %.9f\n", warmup_iterations, warmup_seconds);
    for (int i = 0; i < timed; i++) {
        fprintf(stderr, "sample %d %.9f\n", i, times[i]);
    }
    qsort(times, (size_t)timed, sizeof(double), cmp_double);
    int mid = timed / 2;
    double median =
        (timed % 2 == 1) ? times[mid] : (times[mid - 1] + times[mid]) / 2.0;
    printf("%d %.9f\n", checksum, median);
    free(times);
    return 0;
}
