// corpus: reject/r395-array-length-store
// purpose: Only a literal zero length store statement clears an array.
// exercises: array-clear, rejection-class, divergence-block
// questions: compiler.md §174
// tsc: accepts
// expected-error: S100: only `xs.length = 0` as a statement is accepted

export function main(): void {
  const xs: i32[] = [1];
  xs.length = 2;
  xs.length -= 1;
  const n = (xs.length = 0);
  print(`${n}`);
}

// pin: 69212b29
// pin-dev-jit: Exit 1; S100 at ArrayUnknownMember with no divergence block.
// pin-c-aot: Exit 1; S100 at ArrayUnknownMember with no divergence block.
// pin-interpreter: Checker rejects with S100 before execution.
