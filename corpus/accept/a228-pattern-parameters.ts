// corpus: accept/a228-pattern-parameters
// purpose: Binds a pattern parameter in every function form the language declares.
// observable: extraction runs at entry, in parameter order.
// exercises: binding-pattern, parameter, method, lambda, constructor
// questions: Q30, compiler section 107
// tsc: accepts; js-comparable: yes
class Pair {
  left: i32 = 3;
  right: i32 = 4;
}

class Adder {
  base: i32;
  constructor([first, second]: i32[]) {
    this.base = first + second;
  }
  sum([first, second]: i32[]): i32 {
    return this.base + first + second;
  }
  static named({ left, right }: Pair): i32 {
    return left * right;
  }
}

function total([first, second]: i32[], { left }: Pair): i32 {
  return first + second + left;
}

export function main(): void {
  const adder: Adder = new Adder([1, 2]);
  print(`${adder.base}`);
  print(`${adder.sum([10, 20])}`);
  print(`${Adder.named(new Pair())}`);
  print(`${total([5, 6], new Pair())}`);
  const lambda = ([first, second]: i32[]): i32 => first - second;
  print(`${lambda([9, 4])}`);
  const renaming = ({ left: only }: Pair): i32 => only;
  print(`${renaming(new Pair())}`);
}
