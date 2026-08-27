// corpus: accept/a143-async-generic
// purpose: Proves async generic functions and generic-class async methods at two instantiations.
// exercises: generic-async-function, generic-class-async-method, monomorphization, async-export-instance
// questions: R36, Q34, R13
// tsc: accepts; js-comparable: no C2 C8: The CStruct decorator has no JavaScript shim.
@CStruct
class Vec2 {
  x: f32;
  y: f32;

  constructor(x: f32, y: f32) {
    this.x = x;
    this.y = y;
  }
}

class Box<T> {
  value: T;

  constructor(value: T) {
    this.value = value;
  }

  async read(): Promise<T> {
    await Context.suspend();
    return this.value;
  }
}

async function first<T>(items: T[]): Promise<T> {
  await Context.suspend();
  return items[0];
}

export async function tick<T>(): Promise<void> {
  print("tick");
}

function printVec2(value: Vec2): void {
  print(`${value.x},${value.y}`);
}

export async function main(): Promise<void> {
  const numberBox: Box<u32> = new Box<u32>(7);
  const numberValue: u32 = await numberBox.read();
  print(`${numberValue}`);

  const numbers: u32[] = [11, 12];
  const firstNumber: u32 = await first<u32>(numbers);
  print(`${firstNumber}`);

  const vectorBox: Box<Vec2> = new Box<Vec2>(new Vec2(1.5, 2.5));
  printVec2(await vectorBox.read());

  const vectors: Vec2[] = [new Vec2(3.5, 4.5), new Vec2(5.5, 6.5)];
  printVec2(await first<Vec2>(vectors));

  await tick<u32>();
}
