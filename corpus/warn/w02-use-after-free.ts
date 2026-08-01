// corpus: warn/w02-use-after-free
// warning: W002
// purpose: Identifies a straight-line local use after explicit release.
// exercises: Context.free, use-after-free, local-dataflow
// questions: Q6

class Token {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }
}

export function main(): void {
  const token: Token = new Token(7);
  Context.free(token);
  print(`${token.value}`);
}
