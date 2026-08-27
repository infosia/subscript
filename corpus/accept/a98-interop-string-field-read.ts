// corpus: accept/a98-interop-string-field-read
// purpose: Materializes language strings from C-filled string views inside pointer-passed boundary structs.
// exercises: string-view-field, boundary-struct-pointer, c-layout-scratch, view-copy-in, all-zero-view
// questions: Q13, C4
// tsc: accepts
// compiler.md §28.2. The first fill writes a stable C view and scalar
// fields; copy-back materializes the viewed bytes as a language string. The
// second fill writes an all-zero view, which must read as the empty string.

export function main(): void {
  const filled: SubBoundaryStringRecord = new SubBoundaryStringRecord(
    "before",
    0,
    false,
    0,
    0,
  );
  subBoundaryStringFill(filled, false);
  Context.collect();
  print(filled.label);
  print(`${filled.handle}`);
  print(`${filled.enabled}`);
  print(`${filled.serial}`);
  print(`${filled.generation}`);

  const empty: SubBoundaryStringRecord = new SubBoundaryStringRecord(
    "not-empty",
    0,
    false,
    0,
    0,
  );
  subBoundaryStringFill(empty, true);
  Context.collect();
  print(`${empty.label.length}`);
  print(`[${empty.label}]`);
}
