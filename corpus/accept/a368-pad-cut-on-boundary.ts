// corpus: accept/a368-pad-cut-on-boundary
// purpose: A padding whose cut is on a UTF-8 boundary gives the cyclic pad bytes.
// exercises: padStart, padEnd, utf-8
// questions: compiler.md §191 rule 2, Q5, Q21
// tsc: accepts; js-comparable: no Q5: the target counts UTF-8 bytes, and JavaScript counts UTF-16 units.
export function main(): void {
  // A mixed pad: 7 fill bytes are "あxあ", so the partial cycle ends after "あ".
  const mixedStart: string = "A".padStart(8, "あx");
  const mixedEnd: string = "A".padEnd(8, "あx");
  print(`${mixedStart} ${mixedStart.length}`);
  print(`${mixedEnd} ${mixedEnd.length}`);
  // A whole copy of the mixed pad.
  print("A".padStart(5, "あx"));
  // A supplementary character pad: 4 bytes each, whole copies only.
  print("A".padStart(5, "𠮷"));
  print("A".padEnd(9, "𠮷"));
  // ASCII pads cut at any byte.
  print("ab".padStart(5, "xy"));
  print("ab".padEnd(5, "xy"));
  print("7".padStart(3));
  // An empty pad and a receiver that is already long enough do not change.
  print("あ".padStart(9, ""));
  print("あい".padStart(2, "x"));
  print("あい".padEnd(-1, "x"));
}
