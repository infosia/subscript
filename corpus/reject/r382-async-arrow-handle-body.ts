// corpus: reject/r382-async-arrow-handle-body
// purpose: An async expression body returns a fulfilled value without handle adoption.
// exercises: async-arrow, async-return, held-handle
// questions: compiler.md §167 rule 3
// tsc: accepts
// expected-error: S100, the return value expects i32, got Promise<i32>

export function main(): void {
  const job = async (h: Promise<i32>): Promise<i32> => h;
}
