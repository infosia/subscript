/* benchmark: tree (C baseline)
 * Build, traverse, and free 30 full binary trees of depth 16 (131071 nodes
 * each) with malloc/free — the manual-lifetime counterpart of subscript's
 * reference class + unsafeDelete. Checksum: total node-visit count (int64) =
 * 30 * (2^17 - 1) = 3932130.
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <time.h>

enum { DEPTH = 16, COUNT = 30 };

typedef struct Node {
    struct Node *left;
    struct Node *right;
} Node;

static Node *build(int depth) {
    Node *n = (Node *)malloc(sizeof(Node));
    if (depth == 0) {
        n->left = NULL;
        n->right = NULL;
    } else {
        n->left = build(depth - 1);
        n->right = build(depth - 1);
    }
    return n;
}

static int32_t check(const Node *node) {
    if (node->left == NULL) {
        return 1;
    }
    if (node->right == NULL) {
        return 1;
    }
    return 1 + check(node->left) + check(node->right);
}

static void free_tree(Node *node) {
    if (node->left != NULL) {
        free_tree(node->left);
    }
    if (node->right != NULL) {
        free_tree(node->right);
    }
    free(node);
}

static int64_t workload(void) {
    int64_t checksum = 0;
    for (int i = 0; i < COUNT; i++) {
        Node *root = build(DEPTH);
        checksum += (int64_t)check(root);
        free_tree(root);
    }
    return checksum;
}

static double now_seconds(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec + (double)ts.tv_nsec * 1e-9;
}

static int cmp_double(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return (x > y) - (x < y);
}

int main(int argc, char **argv) {
    /* Warm-up and timed counts come from argv, defaulting to the 3/11 floor, so
     * the runner drives this baseline with the same counts as every other
     * subject. Only the workload call is timed. */
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
    int64_t checksum = 0;
    for (int i = 0; i < warmup; i++) {
        checksum = workload();
    }
    for (int i = 0; i < timed; i++) {
        double t0 = now_seconds();
        checksum = workload();
        double t1 = now_seconds();
        times[i] = t1 - t0;
    }
    qsort(times, (size_t)timed, sizeof(double), cmp_double);
    int mid = timed / 2;
    double median = (timed % 2 == 1) ? times[mid] : (times[mid - 1] + times[mid]) / 2.0;
    printf("%lld %.9f\n", (long long)checksum, median);
    free(times);
    return 0;
}
