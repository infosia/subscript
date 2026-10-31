// corpus: accept/a414-element-receiver-stays-accepted
// purpose: Each form that moves no element of the receiver array stays accepted: pop, a push of another element type, a move in an argument, a stable receiver, a push to the outer array of a nested receiver, and the spelled copy.
// exercises: value-struct, dynamic-array, method-receiver, Array.pop, array-push
// questions: compiler.md §221.1 rule 2, 6, and 7
// tsc: accepts
// js-comparable: no C2: The ValueType decorator has no JavaScript shim.
@ValueType
class V {
  a: i32 = 1;

  drop(list: V[]): void {
    list.pop();
    this.a = 5;
  }

  log(other: i32[]): void {
    other.push(this.a);
    this.a = 6;
  }

  plus(n: i32): i32 {
    return this.a + n;
  }

  grow(list: V[]): void {
    list.push(new V());
    this.a = 8;
  }

  outer(rows: V[][]): void {
    for (let i: i32 = 0; i < 64; i = i + 1) {
      rows.push([]);
    }
    this.a = 9;
  }

  store(list: V[], x: V): void {
    list[1] = x;
    this.a = 3;
  }
}

class Box {
  v: V = new V();
}

function moved(list: V[]): i32 {
  list.push(new V());
  return 2;
}

export function main(): void {
  const all: V[] = [new V(), new V(), new V()];
  all[0].drop(all);
  print(`pop ${all.length} ${all[0].a}`);

  const other: i32[] = [];
  all[0].log(other);
  print(`other ${other.join(",")} ${all[0].a}`);

  print(`argument ${all[0].plus(moved(all))} ${all.length}`);

  const local: V = new V();
  local.grow(all);
  const box: Box = new Box();
  box.v.grow(all);
  print(`stable ${local.a} ${box.v.a} ${all.length}`);

  const boxes: Box[] = [box];
  boxes[0].v.grow(all);
  print(`handle ${box.v.a} ${all.length}`);

  const t: V = all[0];
  t.grow(all);
  all[0] = t;
  print(`copy ${all[0].a} ${all.length}`);

  all[0].store(all, new V());
  print(`store ${all[0].a}`);

  const rows: V[][] = [[new V()]];
  rows[0][0].outer(rows);
  print(`outer ${rows[0][0].a} ${rows.length}`);
}
