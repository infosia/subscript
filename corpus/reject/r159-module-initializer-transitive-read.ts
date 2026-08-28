// corpus: reject/r159-module-initializer-transitive-read
// purpose: Rejects a module initializer whose called function reads a later data binding.
// exercises: module-initializer, declaration-order, transitive-global-read
// questions: §67
// tsc: accepts
// expected-error: S100 at the calling initializer

class Box {
  value: i32 = 5;
}

const g: Box = f();

function f(): Box {
  return h;
}

const h: Box = new Box();

export function main(): void {
  print(`h=${h.value}`);
  print(`g=${g.value}`);
}
