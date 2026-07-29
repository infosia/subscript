// corpus: warn/w01-loop-allocation-unreleased
// warning: W001
// purpose: Identifies a reference-class allocation retained once per loop iteration.

class Token {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }
}

export function main(): void {
  for (let i: i32 = 0; i < 3; i += 1) {
    const token: Token = new Token(i);
    print(`${token.value}`);
  }
}
