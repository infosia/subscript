// corpus: accept/a16-explicit-collect
// purpose: Drops the last allocation reference before an explicit collection request.
// exercises: reference-class, allocation, last-reference-drop, explicit-collection
// questions: Q1, Q2, Q7, Q8, Q12
// tsc: accepts
class Token {
  id: i32;

  constructor(id: i32) {
    this.id = id;
  }
}

export function main(): void {
  let token: Token | null = new Token(17);
  if (token !== null) {
    print(`${token.id}`);
  }
  token = null;
  Context.collect();
  print("collected");
}
