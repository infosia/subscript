// corpus: accept/a128-host-owned-state
// interpreter: no — requires host pre-entry and post-run hooks
// purpose: Proves that host-owned state spans two separate script entry calls.
// exercises: ship-host-hooks, borrowed-opaque-handle, sync-and-async-entries
// questions: R21, Q1, C8
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
let borrowed: SubHostOwnedState | null = null;

export function main(): void {
  const current: SubHostOwnedState = subHostOwnedStateBorrow();
  borrowed = current;
  print(`host-state:first=${subHostOwnedStateAdvance(current)}`);
}

export async function secondEntry(): Promise<void> {
  if (borrowed === null) {
    print("host-state:missing");
    return;
  }
  print(`host-state:second=${subHostOwnedStateAdvance(borrowed)}`);
}
