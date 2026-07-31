// corpus: reject/r100-floating-async-call
// purpose: Requires every direct async call to appear immediately under await.
// exercises: async-call-statement, no-Promise-values
// questions: Q34, C8
// tsc-clean-standalone: verified with node_modules/.bin/tsc against prelude/lang.d.ts; stock TypeScript permits ignoring an async return value.
// expected-error: S013 at the floating async call

async function work(): Promise<void> {
  await Context.suspend();
}

export function main(): void {
  work();
}
