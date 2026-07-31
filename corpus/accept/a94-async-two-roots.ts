// corpus: accept/a94-async-two-roots
// purpose: Pins standard-runner async-export kick and poll ordering.
// exercises: sync-main, two-async-roots, kick-order, pump-to-quiescence
// questions: Q34, C8

export function main(): void {
  print("main:sync");
}

export async function first(): Promise<void> {
  print("first:kick");
  await Context.suspend();
  print("first:step-1");
  await Context.suspend();
  print("first:done");
}

export async function second(): Promise<void> {
  print("second:kick");
  await Context.suspend();
  print("second:done");
}
