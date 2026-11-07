// corpus: reject/r492-nullable-function-value-to-function-type
// purpose: A nullable function value does not fit a function type, also when its result fits the result of that type.
// exercises: function-value, nullable-reference
// questions: compiler.md §235.1 rules 2 and 5b
// tsc: rejects TS2322
// expected-error: S100 at line 18, type mismatch: the initializer expects

class C {
  v: i32 = 1;
}

function maybeId(b: boolean): ((x: C) => C) | null {
  return b ? (x: C): C => x : null;
}

export function main(): void {
  const m: ((x: C) => C) | null = maybeId(false);
  const f: (x: C) => C | null = m;
  print(`${f(new C()) === null}`);
}
