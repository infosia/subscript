// corpus: reject/r473-element-reach-accessor
// purpose: A method reaches a move through a read accessor, and an accessor on an element is a call on that element.
// exercises: value-struct, dynamic-array, method-receiver, read-accessor, reach
// questions: compiler.md §221.1 rule 4
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
class World {
  list: V[] = [];

  get size(): i32 {
    this.list.reverse();
    return this.list.length;
  }
}

const world: World = new World();

@ValueType
class V {
  a: i32 = 1;

  get size(): i32 {
    return world.size + this.a;
  }
}

export function main(): void {
  world.list.push(new V());
  print(`${world.list[0].size}`);
}
