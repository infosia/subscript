// corpus: trap/t61-uncaught-exception
// purpose: An exception that no handler catches leaves the host entry as the uncaught-exception trap with the Error's name, its message, and the throw position.
// exercises: throw, uncaught-exception, host-entry
// questions: Q9
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the `throw` in `validate`
function validate(value: i32): void {
  if (value > 2) {
    throw new TypeError(`value ${value} is out of range`);
  }
  print(`valid ${value}`);
}

export function main(): void {
  validate(1);
  validate(2);
  validate(3);
  print("unreached");
}
