// corpus: accept/a08-string-view
// purpose: Uses string length and slicing without a NUL terminator assumption.
// exercises: string-view, string-length, string-slice
// questions: Q1, Q5, Q12

function middle(value: string, start: i32, end: i32): string {
  return value.slice(start, end);
}

export function main(): void {
  const label: string = "alpha-beta";
  const part: string = middle(label, 6, label.length as i32);
  print(`${label.length}:${part}`);
}
