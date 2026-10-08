// corpus: accept/a348-boundary-values-have-c-form
// interpreter: no — calls the synthetic native interop library
// purpose: Boundary values preserve native bits, signed arguments, and C bool storage.
// exercises: boundary-value, signed-narrow, half-scalar, half-hfa, hfa-return, bool-storage
// questions: compiler.md §179, C8, Q13
// tsc: accepts
// js-comparable: no C8 Q13: Native values and C layout have no JavaScript shim.

@ValueType
class ScriptBools {
  a: boolean; b: boolean;
  constructor(a: boolean, b: boolean) { this.a = a; this.b = b; }
}
export function main(): void {
  print(`${subBoundaryNarrow(-31)}`);
  print(`${subBoundaryHalfCheck(1.5)} ${subBoundaryHalfCheck(subBoundaryHalfReturn())}`);
  print(`${subBoundaryHalf2Check(new SubBoundaryHalf2(1.5, 2.5))} ${subBoundaryHalf2Check(subBoundaryHalf2Return())}`);
  const float = subBoundaryFloat2Return();
  print(`${float.x} ${float.y}`);
  const bools = subBoundaryBoolReturn();
  print(`${bools.a}:${bools.b}:${bools.c}`);
  const storage = new ScriptBools(false, true);
  print(`${subBoundaryScriptSize(Context.bytesOf<ScriptBools>(storage))}`);
}

// pin: 864eb5af
// pin-dev-jit: Rejected: foreign homogeneous floating-point aggregate return is unsupported; no stdout.
// pin-c-aot: Exit 0; stdout: 1; 0 0; 0 0; 1.5 2.5; true:37:true; 8. Golden differs.
// pin-interpreter: no — calls the synthetic native interop library.
// tsc-version: TypeScript 5.9.2; exit 0.
