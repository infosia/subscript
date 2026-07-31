// corpus: reject/r97-promise-combinator
// purpose: Rejects the Promise object combinator surface.
// exercises: Promise.then, async-call-value
// questions: Q34, C8
// expected-error: S013 at the `.then` call

async function leaf(): Promise<i32> {
  return 1;
}

export function main(): void {
  leaf().then((value) => print(`${value}`));
}
