// example: e03-memory
// teaches: Allocate reference classes in a Context, free explicitly, and request collection explicitly.
// differs-from-typescript: invariant 2 forbids implicit collection; omitted collection is correct and retains more memory.
// see: corpus/accept/a15-manual-lifetime.ts, corpus/accept/a16-explicit-collect.ts, CLAUDE.md invariant 2, collisions.md Q6-Q7, compiler.md §18.2d, examples.md §5

// Nothing in this program collects on its own. It shows the two ways an
// allocation ends, and the one call that reclaims what is left.
// C2 and invariant 2: a plain class is Context allocated, and no collector
// runs merely because its last script reference leaves scope.
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
  // Q6 and invariant 2: Context.free ends this allocation immediately.
  Context.free(token);
}

// Way two: the script drops the last reference and keeps the memory. The
// bytes stay with the Context until a collect or the Context release.
function leaveUnreachable(): void {
  const token: Token = new Token(false);
  print(`unreachable=${token.active}`);
  // Invariant 2: returning drops the last script reference but does not
  // invoke collection.
}

// Q12: this zero-argument void export is a host-callable script entry.
export function main(): void {
  // manual=true and unreachable=false come from the two allocations. Only the
  // second one leaves memory for the call below.
  releaseManually();
  leaveUnreachable();

  // Q7 and invariant 2: this call is the only reason collection runs.
  // Omitting it remains correct and retains more Context memory until release.
  Context.collect();

  // Invariant 2: scripts have no memory-counter observable for the call
  // above. The examples.md §5 capstone reads subscript_rt_ctx_live_bytes from the
  // host around explicit collection, as compiler.md §18.2d specifies.
}
