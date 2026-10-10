// corpus: accept/a363-interop-unreadable-field-input
// interpreter: no — calls the synthetic native interop library
// purpose: Passes structs that hold a count-first pair and a string view to C as input parameters.
// exercises: struct-scalar-pair, string-view-field, by-value-struct-parameter, view-copy-in
// questions: compiler.md §187, Q13, C4
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// compiler.md §187 rule 5. These members have no read lowering, so no
// foreign result holds them (r405–r410). The write direction stays: the
// script builds each struct and C reads it.

export function main(): void {
  const lay: SubDescReadLay = new SubDescReadLay(1, [10, 20]);
  print(`${subDescReadLayTotal(lay)}`);
  print(`${subDescReadLayTotal(new SubDescReadLay(5, []))}`);
  const label: SubDescReadStr = new SubDescReadStr("abc", 13);
  print(`${subDescReadStrTotal(label)}`);
  print(`${subDescReadStrTotal(null)}`);
}

// pin: ce01c7c7
// pin-dev-jit: Exit 0; output matches the golden.
// pin-c-aot: Exit 0; output matches the golden.
