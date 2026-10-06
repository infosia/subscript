// corpus: reject/r381-async-arrow-result-annotation
// purpose: An async arrow result annotation must be Promise<T>.
// exercises: async-arrow, result-annotation
// questions: compiler.md §167 rule 4
// tsc: rejects TS1064
// expected-error: S100, the async arrow result annotation is not Promise<T>

export function main(): void {
  const job = async (): i32 => 7;
}
