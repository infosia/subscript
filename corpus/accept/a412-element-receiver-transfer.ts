// corpus: accept/a412-element-receiver-transfer
// purpose: A transfer between two elements with the same index loses no write, because this and the element name one object.
// exercises: value-struct, dynamic-array, method-receiver
// questions: compiler.md §221.1 rule 8
// tsc: accepts
// js-comparable: no C2: The ValueType decorator has no JavaScript shim.
@ValueType
class Account {
  balance: i32;

  constructor(balance: i32) {
    this.balance = balance;
  }

  transfer(accs: Account[], j: i32, amount: i32): void {
    this.balance -= amount;
    accs[j].balance += amount;
  }
}

export function main(): void {
  const accs: Account[] = [new Account(100), new Account(50)];
  accs[0].transfer(accs, 0, 10);
  print(`${accs[0].balance}`);
}
