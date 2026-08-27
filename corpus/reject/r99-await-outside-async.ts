// corpus: reject/r99-await-outside-async
// purpose: Rejects await in a synchronous function.
// exercises: await, sync-function
// questions: Q34, C8
// tsc: accepts
// expected-error: S013 at `await`
await Context.suspend();

export function main(): void {}
