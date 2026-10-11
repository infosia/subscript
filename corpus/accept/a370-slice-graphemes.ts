// corpus: accept/a370-slice-graphemes
// purpose: sliceGraphemes slices by cluster positions with the clamping and negative-position rules of slice.
// exercises: text-module, grapheme, slice, utf-8
// questions: compiler.md §193 rule 2, Q5, Q21
// tsc: accepts; js-comparable: no C26: node cannot load the subscript:text module.
import { graphemeLength, sliceGraphemes } from "subscript:text";

export function main(): void {
  // Clusters: "か\u3099", "き", "𠮷", a ZWJ family, a flag, "!".
  const text: string = "か\u3099き𠮷\u{1F469}\u200D\u{1F467}\u{1F1EF}\u{1F1F5}!";
  const n: i32 = graphemeLength(text);
  print(`${n} clusters, ${text.length} bytes`);
  for (let i: i32 = 0; i < n; i++) {
    const cluster: string = sliceGraphemes(text, i, i + 1);
    print(`[${i}] ${cluster} (${cluster.length} bytes)`);
  }
  print(sliceGraphemes(text, 2));
  print(sliceGraphemes(text, 0, 2));
  print(sliceGraphemes(text, -2));
  print(sliceGraphemes(text, -4, -2));
  print(sliceGraphemes(text, -100, 1));
  print(sliceGraphemes(text, 4, 100));
  print(`reversed: "${sliceGraphemes(text, 3, 1)}"`);
  print(`past the end: "${sliceGraphemes(text, 10, 20)}"`);
  // The result is the original bytes: no normalization.
  const first: string = sliceGraphemes(text, 0, 1);
  print(`${first === "か\u3099"} ${first === "が"}`);
  // A user-visible limit: keep the first 3 characters of a name.
  const name: string = "山田\u{1F468}\u200D\u{1F373}太郎";
  print(`${sliceGraphemes(name, 0, 3)}…`);
}
