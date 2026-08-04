// corpus: accept/a128-host-owned-state
// purpose: Proves that host-owned state spans two separate script entry calls.
// exercises: ship-host-hooks, borrowed-opaque-handle, sync-and-async-entries
// questions: R21, Q1, C8

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
