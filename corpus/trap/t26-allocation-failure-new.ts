// corpus: trap/t26-allocation-failure-new
// purpose: Injects allocation failure at a reference-class `new`.
// exercises: allocation-failure, reference-class, new
// questions: none
// tier-policy: both tiers must report the same trap tuple and pre-fault stdout at the same object-allocation count

class Box {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }
}

export function main(): void {
  print("before");
  const box: Box = new Box(7);
  print(`${box.value}`);
}
