// corpus: accept/a164-frame-class-locals
// purpose: Keeps addressable fixed-array locals in coroutine frames across each suspension.
// exercises: local-storage-class, generator, async-await, fixed-array, cstruct, loop
// questions: §68, C2, C8
// tsc: accepts; js-comparable: no C2 C8: The CStruct decorator and coroutine API have no JavaScript shim.

@CStruct
class Cell {
  value: i32;
  constructor(value: i32) { this.value = value; }
}

function* generatorValues(): Generator<i32> {
  const integers: FixedArray<i32, 2> = [1, 2];
  const cells: FixedArray<Cell, 2> = [new Cell(10), new Cell(20)];
  yield integers[0] + cells[0].value;
  let index: i32 = 0;
  while (index < 2) {
    yield integers[index] + cells[index].value;
    index = index + 1;
  }
  yield integers[1] + cells[1].value;
}

export async function asyncValues(): Promise<void> {
  const integers: FixedArray<i32, 2> = [3, 4];
  const cells: FixedArray<Cell, 2> = [new Cell(30), new Cell(40)];
  await Context.suspend();
  print(`async-outside-first=${integers[0] + cells[0].value}`);
  let index: i32 = 0;
  while (index < 2) {
    await Context.suspend();
    print(`async-loop=${integers[index] + cells[index].value}`);
    index = index + 1;
  }
  await Context.suspend();
  print(`async-outside-last=${integers[1] + cells[1].value}`);
}

export function main(): void {
  const generator: Generator<i32> = generatorValues();
  print(`generator=${generator.next().value}`);
  print(`generator=${generator.next().value}`);
  print(`generator=${generator.next().value}`);
  print(`generator=${generator.next().value}`);
}
