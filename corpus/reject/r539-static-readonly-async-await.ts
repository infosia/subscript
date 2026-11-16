// corpus: reject/r539-static-readonly-async-await
// purpose: An async immediate callee drops the static readonly field fact (await).
// exercises: static-readonly, null-narrowing, async-lambda
// questions: compiler.md §322 rule 1
// tsc: rejects TS18047
// expected-error: S011 at line 13
class A { x: i32 = 1; }
function mk(): A | null { return new A(); }
class K { static readonly R: A | null = mk(); }
async function pause(): Promise<void> {}
export async function main(): Promise<void> {
  if (K.R !== null) {
    await (async (): Promise<void> => { await pause(); print(`${K.R.x}`); })();
  }
}

// tsc-version: TypeScript 5.9.2; TS18047 at line 13, measured.
