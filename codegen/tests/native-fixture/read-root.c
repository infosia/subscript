#include "read-root.h"

struct RrDevice_ { int32_t id; };

static struct RrDevice_ rr_devices[2] = {{7}, {42}};
static RrPoint rr_point = {77};
static int rr_marker;

static RrView rr_view(const char *data, size_t len) {
    RrView view;
    view.data = data;
    view.len = len;
    return view;
}

void subReadRootHolderFill(RrHolder *out) {
    out->k = 5;
    out->p = &rr_point;
}

int32_t subReadRootHolderSum(const RrHolder *holder) {
    return holder->k + (holder->p ? holder->p->x : 0);
}

void subReadRootOuterTouch(RrOuter outer) {
    if (outer.inner) {
        outer.inner->x = 9;
        if (outer.inner->q) outer.inner->q->y = 11;
    }
}

int32_t subReadRootOuterSum(RrOuterConst outer) {
    int32_t sum = outer.k;
    if (outer.inner) {
        sum += outer.inner->x;
        if (outer.inner->q) sum += outer.inner->q->y;
    }
    return sum;
}

void subReadRootNamedFill(RrNamed *out) {
    out->name = rr_view("aa", 2);
    out->ud = &rr_marker;
}

void subReadRootDeviceNamedFill(RrDeviceNamed *out) {
    out->name = rr_view("bb", 2);
    out->device = &rr_devices[1];
}

RrDevice subReadRootDevice(int32_t id) {
    return id == 42 ? &rr_devices[1] : &rr_devices[0];
}

int32_t subReadRootDeviceId(RrDevice device) {
    return device ? device->id : -1;
}

void subReadRootCmdRun(const RrCmd *cmd) {
    if (cmd->counter) cmd->counter->n += (int32_t)cmd->name.len;
}

void subReadRootOuterBump(const RrOuter *outer) {
    if (!outer) return;
    if (outer->inner) {
        outer->inner->x += 10;
        if (outer->inner->q) outer->inner->q->y += 10;
    }
}

void subReadRootItemListFill(RrItemList *out) {
    out->tag = 5;
    if (out->itemsCount > 0 && out->items[0].inner) {
        out->items[0].inner->x = 9;
        if (out->items[0].inner->q) out->items[0].inner->q->y = 11;
    }
}

void subReadRootItemSpanTouch(RrItemSpan span) {
    if (span.count > 0) {
        span.items[0].k = 9;
        if (span.items[0].inner) span.items[0].inner->x = 7;
    }
}

int32_t subReadRootLinkedUse(const RrLinked *linked) {
    if (!linked->link) return linked->k * 1000 - 1;
    return linked->k * 1000 + (int32_t)linked->link->name.len * 10 + linked->link->kind;
}

int32_t subReadRootExtUse(const RrExt *ext) {
    return ext->extra * 1000 + (int32_t)ext->base.name.len * 10 + ext->base.kind;
}

void subReadRootNamedUdFill(RrNamedUd *out) {
    out->name = rr_view("aa", 2);
    out->inner.ud = &rr_marker;
    out->inner.k = 9;
}

void subReadRootUdFill(RrUd *out) {
    out->ud = &rr_marker;
    out->k = 9;
}

int32_t subReadRootRender(const RrScene *scene) {
    if (scene->mesh) {
        scene->mesh->count += 1;
        if (scene->mesh->mat) scene->mesh->mat->r *= 2.0f;
    }
    return scene->id;
}

/* Writes through the `const` pointer of a struct that the script owns: the
 * call does not write a `const` target back (compiler.md §187 rule 7). */
int32_t subReadRootRenderConst(const RrScene *scene) {
    ((RrScene *)scene)->id = 50;
    if (scene->mesh) scene->mesh->count += 1;
    return scene->id;
}

void subReadRootJobRun(const RrJob *job) {
    job->callback(rr_view("hi", 2), job->userdata, job->userparam);
}

void subReadRootJobSet(const RrJob *job) {
    job->callback(rr_view("hi", 2), job->userdata, job->userparam);
    if (job->count) job->count->n = 5;
}

void subReadRootNodeBump(RrNode *node) {
    for (RrNode *n = node; n; n = n->next) n->v += 1;
}

int32_t subReadRootCycleUse(const RrCycleA *a) {
    return a->k + (a->b ? a->b->k : 0);
}

int32_t subReadRootFtSum(RrFt t) {
    return (int32_t)(t.m[0] + t.m[3]) + t.k;
}

int32_t subReadRootFtSumP(const RrFt *t) {
    return (int32_t)(t->m[0] + t->m[3]) + t->k;
}

void subReadRootSceneDeform(const RrDScene *scene) {
    if (!scene->mesh) return;
    for (size_t i = 0; i < scene->mesh->vertsCount; i++) scene->mesh->verts[i] *= 2.0f;
}

/* Adds 100 and the first leaf to each middle element (compiler.md §187
 * rule 11): C writes a pair of struct elements reached through a pair. */
void subReadRootNWalk(const RrNTop *top) {
    for (size_t i = 0; i < top->midsCount; i++)
        top->mids[i].m += 100 + top->mids[i].leaves[0].k;
}

/* Reads a `const` pair of struct elements (compiler.md §187 rule 11). */
int32_t subReadRootNSum(const RrNView *view) {
    int32_t sum = 0;
    for (size_t i = 0; i < view->midsCount; i++)
        sum += view->mids[i].m + view->mids[i].leaves[0].k;
    return sum;
}

/* A `const` pair whose elements link a mesh that C can write (compiler.md
 * §187 rule 11): adds 1 to each material and sums the ids and the counts. */
int32_t subReadRootRenderScene(const RrScScene *s) {
    int32_t sum = 0;
    for (size_t i = 0; i < s->entitiesCount; i++) {
        const RrScEntity *e = &s->entities[i];
        sum += e->id;
        if (e->mesh) {
            sum += e->mesh->n;
            if (e->mesh->material) e->mesh->material->r += 1.0f;
        }
    }
    return sum;
}

/* The scalar twin of `subReadRootNWalk`. */
void subReadRootSWalk(const RrSTop *top) {
    for (size_t i = 0; i < top->midsCount; i++)
        top->mids[i].m += 100 + top->mids[i].leaves[0];
}

static RrFp rr_fp = {42};

/* A result with a fixed array and a pointer member (compiler.md §187
 * rule 12). */
RrFr subReadRootFrGet(void) {
    RrFr r = {{1.0f, 2.0f, 3.0f, 4.0f}, &rr_fp};
    return r;
}

RrFb subReadRootFbGet(void) {
    RrFb r = {{1.0f, 2.0f, 3.0f, 4.0f}};
    return r;
}

/* A scalar fixed array in a scratch struct (compiler.md §187 rule 13):
 * C reads `bounds` and writes `bounds[1]`. */
int32_t subReadRootBoundsRender(const RrBScene *scene) {
    if (!scene->mesh) return -1;
    scene->mesh->bounds[1] = 20.0f;
    if (scene->mesh->mat) scene->mesh->mat->r *= 2.0f;
    return (int32_t)(scene->mesh->bounds[0] + scene->mesh->bounds[3]);
}

int32_t subReadRootTrSum(const RrTr *t) {
    return (int32_t)(t->m[0] + t->m[3]) + (t->p ? t->p->x : 0);
}

int32_t subReadRootTrVal(RrTr t) {
    return (int32_t)(t.m[0] + t.m[3]) + (t.p ? t.p->x : 0);
}

void subReadRootTrFill(RrTr *t) {
    t->m[2] = 30.0f;
}

/* A C-writable pair of `CEnum` elements (compiler.md §187 rule 11). */
void subReadRootRecFill(RrRec *r) {
    r->tag = 1;
    if (r->modesCount) r->modes[0] = 12345;
}

void subReadRootRecTouch(RrRec r) {
    if (r.modesCount) r.modes[0] = 777;
}

int32_t subReadRootRecCGet(const RrRecC *r) {
    return r->modesCount > 1 ? r->modes[1] : -1;
}

/* A link to an embedded header whose extension holds a string view
 * (compiler.md §187 rule 14). */
int32_t subReadRootHUse(const RrHHolder *h) {
    if (!h->link) return -1;
    if (h->link->kind == 2) return 10000 + (int32_t)((const RrHExt *)h->link)->name.len;
    return h->link->kind;
}

/* A link to an embedded header whose family copies its bytes. */
int32_t subReadRootGUse(const RrGHolder *h) {
    if (!h->link) return -1;
    if (h->link->kind == 2) return ((const RrGExt *)h->link)->extra * 100 + h->k;
    return h->link->kind;
}

/* A scalar fixed array beside a pair (compiler.md §187 rule 15). */
int32_t subReadRootPDraw(const RrPMesh *m) {
    return (int32_t)(m->bounds[0] + m->bounds[3]) * 100 + (int32_t)m->idsCount * 10 +
           (int32_t)m->ids[1];
}

int32_t subReadRootPDrawV(RrPMesh m) {
    return (int32_t)(m.bounds[0] + m.bounds[3]) * 100 + (int32_t)m.idsCount * 10 +
           (int32_t)m.ids[1];
}

int32_t subReadRootPNamedUse(const RrPNamed *n) {
    return (int32_t)n->name.len * 100 + (int32_t)n->m[3];
}

/* C writes a callback field's userdata and a scalar of a scratch struct
 * (compiler.md §187 rule 8): the scalar is written back, the userdata is
 * not. */
void subReadRootPSubmit(RrPJob *job) {
    job->prio = 9;
    job->userdata = &rr_marker;
}

void subReadRootPStep(RrPWorld *w) {
    if (w->player) {
        w->player->x += 1.0f;
        w->player->user = &rr_marker;
    }
}

/* Sums a `const` list of structs that copy their bytes (compiler.md §187
 * rule 9). */
int32_t subReadRootCSum(const RrCNode *n) {
    int32_t sum = 0;
    for (const RrCNode *node = n; node; node = node->next) sum += node->v;
    return sum;
}

/* Links to embedded headers that an extension embeds (compiler.md §187
 * rule 14): each link passes as its declared type. */
void subReadRootCross(const RrVec3 *a, const RrVec3 *b, RrVec3 *out) {
    RrVec3 r;
    r.x = a->y * b->z - a->z * b->y;
    r.y = a->z * b->x - a->x * b->z;
    r.z = a->x * b->y - a->y * b->x;
    *out = r;
}

void subReadRootMousePos(RrVec2 *out) {
    out->x = 3.0f;
    out->y = 4.0f;
}

int32_t subReadRootLabelLen(const RrLabel *label) {
    return (int32_t)label->text.len;
}

int32_t subReadRootDrawMeshInstanced(RrRMesh mesh, const RrMatrix *transforms, int32_t instances) {
    return mesh.vertexCount * 100 + (int32_t)transforms[0].m12 * 10 + instances;
}

int32_t subReadRootLinkedExt(const RrLinked *linked) {
    if (!linked->link) return -1;
    if (linked->link->kind == 2) {
        const RrExt *e = (const RrExt *)linked->link;
        return e->extra * 100 + (int32_t)e->base.name.len;
    }
    return linked->link->kind * 1000 + (int32_t)linked->link->name.len;
}

void subReadRootWTouch(RrWHolder *h) {
    /* `k` names the class that `link` points into. */
    if (h->link && h->k == 2) ((RrWExt *)h->link)->extra += 1000;
}

int32_t subReadRootWRead(const RrWHolder *h) {
    if (!h->link) return -1;
    if (h->k == 2) return ((const RrWExt *)h->link)->extra;
    return (int32_t)h->link->x;
}

/* C memory that a script reference names (compiler.md §187 rule 14): the
 * link passes it as it is and reads no class id. */
static RrIV2 rr_grid[8];

RrIV2 *subReadRootIvAt(int32_t i) {
    return &rr_grid[i];
}

void subReadRootIvSet(int32_t i, int32_t x, int32_t y) {
    rr_grid[i].x = x;
    rr_grid[i].y = y;
}

int32_t subReadRootIvSum(const RrIV2 *p) {
    return p ? p->x * 100 + p->y : -1;
}

void subReadRootIvBump(RrIV2 *p) {
    if (p) p->x += 1;
}

int32_t subReadRootLabLen(const RrLab *l) {
    return (int32_t)l->text.len;
}
