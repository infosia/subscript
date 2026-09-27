// corpus: accept/a270-numeric-container-copy
// purpose: Copy live numeric Map and Set sources as distinct handles.
// exercises: map-copy, set-copy, numeric-layout
// questions: Q24
// tsc: accepts; js-comparable: yes
export function main(): void {
  const map = new Map<i32, i32>();
  map.set(1, 7);
  const mapCopy = new Map(map);
  const set = new Set<i32>();
  set.add(3);
  const setCopy = new Set<i32>(set);
  print(`${map === mapCopy} ${set === setCopy}`);
}
