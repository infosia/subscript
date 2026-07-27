// corpus: reject/r77-pass-keys-view
// purpose: Rejects passing a subject-only fused keys() view.
// exercises: for-of-subject-restriction, escaping-iterator-temporary
// questions: Q30
// expected-error: keys() may not outlive the call that creates it

function consume(values: i32[]): void {
  print(`${values.length}`);
}

export function main(): void {
  const map: Map<i32, string> = new Map<i32, string>();
  consume(map.keys());
}
