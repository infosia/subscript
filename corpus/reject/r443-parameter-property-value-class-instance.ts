// corpus: reject/r443-parameter-property-value-class-instance
// purpose: A parameter property whose type first names a value-class instance checks that instance shape.
// exercises: parameter-property, value-class, generics
// questions: compiler.md §198.1 rule 5a, compiler.md §198.3
// tsc: accepts
// expected-error: S100, field type `Enemy` is outside the value-class whitelist
class Enemy {
  constructor(public hp: i32) {}
}

@ValueType
class Slot<T> {
  item: T;
  constructor(item: T) {
    this.item = item;
  }
}

class World {
  constructor(public slot: Slot<Enemy>) {}
}

export function main(): void {
  const w = new World(new Slot<Enemy>(new Enemy(5)));
  print(`${w.slot.item.hp}`);
}
