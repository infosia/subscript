// corpus: accept/a146-scoped-locals
// purpose: Pins source scopes and derived type names in emitted C.
// exercises: managed-local-shadowing, coroutine-scope, storage-mask, lambda-capture-name-table, coroutine-frame-type, for-of-body-scope, generator-driven-for-of, switch-case-shadow, lambda-function-state, function-body-kinds, using-scope
// questions: §66
// tsc: accepts; js-comparable: no C8 C11: The coroutine API has no JavaScript shim.
class Box {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }

  async x(): Promise<i32> {
    await Context.suspend();
    return 103;
  }
}

class ScopeBodies {
  value: i32;

  constructor(value: i32) {
    const constructorLocal: i32 = value + 1;
    print(`constructor-body:${constructorLocal}`);
    this.value = value;
  }

  method(): void {
    const methodLocal: i32 = this.value + 2;
    print(`method-body:${methodLocal}`);
  }

  get accessorValue(): i32 {
    const accessorLocal: i32 = this.value + 3;
    print(`accessor-body:${accessorLocal}`);
    return accessorLocal;
  }
}

class ScopedResource {
  label: string;

  constructor(label: string) {
    this.label = label;
  }

  [Symbol.dispose](): void {
    print(`dispose:${this.label}`);
  }
}

function synchronousScopes(): void {
  const text: string = "string:outer";
  {
    const text: string = "string:inner";
    print(text);
  }
  print(text);

  const box: Box = new Box(1);
  {
    const box: Box = new Box(2);
    print(`box:${box.value}`);
  }
  print(`box:${box.value}`);

  const loopText: string = "loop:outer";
  let i: i32 = 0;
  while (i < 2) {
    const loopText: string = `loop:${i}`;
    print(loopText);
    i += 1;
  }
  print(loopText);

  const forText: string = "for:outer";
  for (
    let forText: string = "for:inner";
    forText === "for:inner";
  ) {
    print(forText);
    break;
  }
  print(forText);

  const branchText: string = "branch:outer";
  if (i === 2) {
    const branchText: string = "branch:inner";
    print(branchText);
  }
  print(branchText);

  const switchText: string = "switch:outer";
  switch (1) {
    case 1:
      const switchText: string = "switch:inner";
      print(switchText);
      break;
  }
  print(switchText);

  const a$b: i32 = 12;
  const a_dollar_b: i32 = 34;
  const captured: () => i32 = (): i32 => a$b + a_dollar_b;
  print(`lambda:${captured()}`);

  const storage: string = "storage:outer-managed";
  {
    const storage: i32 = 5;
    print(`storage:inner-unmanaged:${storage}`);
  }
  print(storage);

  const reverseStorage: i32 = 7;
  {
    const reverseStorage: string = "storage:inner-managed";
    print(reverseStorage);
  }
  print(`storage:outer-unmanaged:${reverseStorage}`);

  const values: i32[] = [1, 2, 3];
  for (const value of values) {
    const value: i32 = 100;
    print(`for-of-body:${value}`);
  }

  for (const generatedValue of generatorScopes()) {
    const generatedValue: i32 = 100;
    print(`gen-for-of-body:${generatedValue}`);
  }

  for (let bodyValue: i32 = 0; bodyValue < 1; bodyValue += 1) {
    const bodyValue: i32 = 200;
    print(`for-body:${bodyValue}`);
  }

  const localLambda: () => i32 = (): i32 => {
    const bonus: i32 = 9;
    return bonus;
  };
  print(`lambda-body:${localLambda()}`);

  const map: Map<string, i32> = new Map<string, i32>();
  map.set("a", 1);
  for (const key of map.keys()) {
    const pick: () => i32 = (): i32 => {
      const bonus: i32 = 10;
      return bonus;
    };
    print(`map-lambda:${key}${pick()}`);
  }

  const bodies: ScopeBodies = new ScopeBodies(10);
  bodies.method();
  print(`accessor-result:${bodies.accessorValue}`);

  const usingText: string = "using:outer";
  {
    using resource = new ScopedResource("using");
    const usingText: string = "using:inner";
    print(usingText);
  }
  print(usingText);
}

function* generatorScopes(): Generator<i32> {
  const blockText: string = "generator-block:outer";
  {
    const blockText: string = "generator-block:inner";
    print(blockText);
  }
  print(blockText);

  const switchText: string = "generator-switch:outer";
  switch (1) {
    case 1:
      const switchText: string = "generator-switch:inner";
      print(switchText);
      break;
  }
  print(switchText);

  const forOfText: string = "generator-for-of:outer";
  for (const value of [1]) {
    {
      const forOfText: string = `generator-for-of:inner:${value}`;
      print(forOfText);
    }
    print(forOfText);
  }
  print(forOfText);

  const forText: string = "generator-for:outer";
  for (
    let forText: string = "generator-for:inner";
    forText === "generator-for:inner";
  ) {
    print(forText);
    break;
  }
  print(forText);
  yield 1;
  yield 2;
}

async function asyncScopes(): Promise<void> {
  const text: string = "async:outer";
  {
    const text: string = "async:inner";
    print(text);
  }
  await Context.suspend();
  print(text);
}

async function m0_x(): Promise<i32> {
  await Context.suspend();
  return 200;
}

function runGeneratorScopes(): void {
  const generator: Generator<i32> = generatorScopes();
  generator.next();
}

export async function main(): Promise<void> {
  synchronousScopes();
  runGeneratorScopes();
  await asyncScopes();
}
