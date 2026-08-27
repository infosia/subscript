// corpus: reject/r76-return-keys-view
// purpose: Rejects returning a subject-only fused keys() view.
// exercises: for-of-subject-restriction, escaping-iterator-temporary
// questions: Q30
// tsc: rejects TS2740
// expected-error: keys() may not outlive the call that creates it
function leak(map: Map<i32, string>): i32[] {
  return map.keys();
}

export function main(): void {
  const map: Map<i32, string> = new Map<i32, string>();
  print(`${leak(map).length}`);
}
