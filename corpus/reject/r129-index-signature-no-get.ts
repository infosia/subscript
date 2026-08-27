// corpus: reject/r129-index-signature-no-get
// purpose: Rejects an index signature without its get accessor.
// exercises: class-index-signature, missing-get-accessor
// questions: §58, R29
// tsc: accepts
// expected-error: an index signature requires `get(index: I): T`
class MissingGet {
  readonly [index: u32]: i32;
}

export function main(): void {
  print("unreachable");
}
