// corpus: accept/a160-module-initializer-order
// purpose: Runs module initializers and later entry calls that read initialized data bindings.
// exercises: module-initializer, declaration-order, direct-global-read, transitive-global-read
// questions: §67
// tsc: accepts; js-comparable: yes

const earlier: i32 = 4;
const direct: i32 = earlier;
const throughFunction: i32 = readEarlier();

function readEarlier(): i32 {
  return earlier;
}

function readLater(): i32 {
  return later;
}

const later: i32 = 9;

export function main(): void {
  print(`${direct}`);
  print(`${throughFunction}`);
  print(`${later}`);
  print(`${readLater()}`);
}
