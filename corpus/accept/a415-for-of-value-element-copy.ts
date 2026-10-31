// corpus: accept/a415-for-of-value-element-copy
// purpose: A for-of binding of a value element is a copy on each tier, so a later write or a growth of the array leaves the binding unchanged.
// exercises: value-struct, dynamic-array, for-of, generator, async
// questions: compiler.md §221.1 rule 12, §220.1 rule 4
// tsc: accepts
// js-comparable: no C2: The ValueType decorator has no JavaScript shim, and a for-of binding of a value element is a copy.
@ValueType
class V {
  a: i32 = 1;
  b: i32 = 2;
}

function* values(all: V[]): Generator<i32> {
  for (const v of all) {
    all[0].b = 40;
    yield v.b;
    print(`after-yield ${v.b}`);
    break;
  }
}

async function late(all: V[]): Promise<void> {
  for (const v of all) {
    all[0].b = 50;
    await Context.suspend();
    print(`after-suspend ${v.b}`);
    break;
  }
}

export async function main(): Promise<void> {
  const all: V[] = [new V()];
  for (const v of all) {
    all[0].b = 9;
    print(`write ${v.b}`);
    break;
  }

  const grown: V[] = [new V()];
  for (const v of grown) {
    for (let i: i32 = 0; i < 5000; i = i + 1) {
      grown.push(new V());
    }
    print(`grow ${v.b}`);
    break;
  }

  const kept: V[] = [new V()];
  for (const v of kept) {
    const w: V = v;
    kept[0].b = 7;
    print(`const ${v.b} ${w.b}`);
    break;
  }

  const each: V[] = [new V(), new V()];
  let total: i32 = 0;
  for (const v of each) {
    each[1].b = 5;
    total = total + v.b;
  }
  print(`each ${total}`);

  const stepped: V[] = [new V()];
  const steps: Generator<i32> = values(stepped);
  print(`yield ${steps.next().value}`);
  steps.next();

  await late([new V()]);
}
