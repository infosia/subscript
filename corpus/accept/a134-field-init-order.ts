// corpus: accept/a134-field-init-order
// purpose: Orders constructor arguments before field initializers and the constructor body.
// exercises: field-initializer, constructor-argument-order, initializer-side-effect
// questions: §57, R27
// tsc: accepts; js-comparable: yes
function initValue(): i32 {
  print("init runs");
  return 17;
}

function argumentValue(): i32 {
  print("arg runs");
  return 29;
}

class OrderedFields {
  initialized: i32 = initValue();
  argument: i32;

  constructor(argument: i32) {
    this.argument = argument;
  }
}

export function main(): void {
  const fields: OrderedFields = new OrderedFields(argumentValue());
  print(`initialized:${fields.initialized}`);
  print(`argument:${fields.argument}`);
}
