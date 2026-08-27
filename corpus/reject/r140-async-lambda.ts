// corpus: reject/r140-async-lambda
// purpose: Keeps async arrow functions outside the decided surface.
// exercises: async-arrow-function
// questions: R36, Q34
// tsc: accepts
// expected-error: S100 at the async arrow function

export async function main(): Promise<void> {
  const work = async (): Promise<void> => {
    await Context.suspend();
  };
}
