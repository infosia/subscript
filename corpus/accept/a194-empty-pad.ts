// corpus: accept/a194-empty-pad
// purpose: Checks compiler section 95 empty-pad semantics.
// exercises: string-methods, q14-formatting
// questions: Q21
// tsc: accepts; js-comparable: yes
export function main(): void {
  print("ab".padStart(-1, ""));
  print("ab".padStart(1, ""));
  print("ab".padStart(2, ""));
  print("ab".padStart(4, ""));
  print("ab".padEnd(-1, ""));
  print("ab".padEnd(1, ""));
  print("ab".padEnd(2, ""));
  print("ab".padEnd(4, ""));
  print("ab".padStart(4, "x"));
  print("ab".padEnd(4, "x"));
}
