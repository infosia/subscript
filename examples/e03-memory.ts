// example: e03-memory
// teaches: Allocate reference classes in a Context, free explicitly, and request collection explicitly.
// differs-from-typescript: the host controls each collection pause, and no collection moves an object; Context.free ends an allocation at once.
// see: corpus/accept/a15-manual-lifetime.ts, corpus/accept/a16-explicit-collect.ts, CLAUDE.md invariant 2, collisions.md Q6-Q7, compiler.md §18.2d and §342, examples.md §5

// The program shows the two ways an allocation ends, and the explicit call
// that reclaims what is left at a point the script selects.
// C2 and invariant 2: a plain class is Context allocated. When its last
// script reference leaves scope, the memory stays until a collection.
class Token {
  active: boolean;

  constructor(active: boolean) {
    this.active = active;
  }
}

// Way one: the script ends the allocation at a point it selects.
function releaseManually(): void {
  const token: Token = new Token(true);
  print(`manual=${token.active}`);
  // Q6: Context.free ends this allocation immediately.
  Context.free(token);
}

// Way two: the script drops the last reference. The bytes stay with the
// Context until a collection or the Context release.
function leaveUnreachable(): void {
  const token: Token = new Token(false);
  print(`unreachable=${token.active}`);
  // Invariant 2: returning drops the last script reference. The next
  // collection reclaims the token; the return does not collect.
}

// Q12: this zero-argument void export is a host-callable script entry.
export function main(): void {
  // manual=true and unreachable=false come from the two allocations. Only the
  // second one leaves memory for the call below.
  releaseManually();
  leaveUnreachable();

  // Q7: this call is the explicit collection; it runs here. This program
  // allocates much less than the minimum headroom, so no automatic collection
  // runs (compiler.md §342 rules 6 and 17). Omitting the call is correct.
  Context.collect();

  // Scripts have no memory-counter observable for the call above. The
  // examples.md §5 capstone reads subscript_rt_ctx_live_allocations from the
  // host around an explicit collection, as compiler.md §18.2d specifies.
}
