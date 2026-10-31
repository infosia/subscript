// corpus: reject/r470-element-reach-method-call
// purpose: A method reaches a move through a method of another class.
// exercises: value-struct, dynamic-array, method-receiver, reach
// questions: compiler.md §221.1 rule 4
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: i32 = 1;

  step(world: World): void {
    world.grow();
  }
}

class World {
  list: V[] = [new V()];

  grow(): void {
    this.list.unshift(new V());
  }
}

export function main(): void {
  const world: World = new World();
  world.list[0].step(world);
  print(`${world.list.length}`);
}
