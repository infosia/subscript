// corpus: accept/a190-async-children-across-suspension
// purpose: Two held children progress together across the holder's own suspension, and carry value-class results.
// exercises: async-call, held-handle, Context.suspend, checkpoint-order, CStruct-result
// questions: §94, Q34, C8
// tsc: accepts; js-comparable: no C2 C8: The CStruct decorator and the Context API have no JavaScript shim.
@CStruct
class Pair {
  x: i32 = 0;
  y: i32 = 0;

  constructor(x: i32, y: i32) {
    this.x = x;
    this.y = y;
  }
}

async function work(id: i32): Promise<Pair> {
  print(`start${id}`);
  await Context.suspend();
  print(`mid${id}`);
  await Context.suspend();
  print(`end${id}`);
  return new Pair(id, id * 10);
}

export async function main(): Promise<void> {
  const a: Promise<Pair> = work(1);
  const b: Promise<Pair> = work(2);
  print("held");
  await Context.suspend();
  print("before-await");
  const first: Pair = await a;
  print(`a=${first.x},${first.y}`);
  const second: Pair = await b;
  print(`b=${second.x},${second.y}`);
}
