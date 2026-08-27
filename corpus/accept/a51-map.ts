// corpus: accept/a51-map
// purpose: Exercises the accepted Map battery, including nullable get,
//          total getOr, integer/string keys, inline value-class values,
//          eager clear/delete, and explicit collection of a dropped map.
// exercises: map-methods, monomorphization, container-collection
// questions: Q24, C2, C7
// tsc: accepts
@CStruct
class Vec2 {
  x: i32;
  y: i32;
  constructor(x: i32, y: i32) {
    this.x = x;
    this.y = y;
  }
}

class Boxed {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}

function leaveDroppedMap(): void {
  const dropped: Map<string, Boxed> = new Map<string, Boxed>();
  dropped.set("held-only-here", new Boxed(99));
}

export function main(): void {
  const numbers: Map<i32, i32> = new Map<i32, i32>();
  print(`set receiver ${numbers.set(1, 10) === numbers}`);
  numbers.set(2, 20);
  numbers.set(1, 11);
  print(`numbers ${numbers.size} ${numbers.has(1)} ${numbers.getOr(1, -1)} ${numbers.getOr(9, -1)}`);
  print(`delete ${numbers.delete(2)} ${numbers.delete(2)} ${numbers.size}`);

  const references: Map<i32, Boxed> = new Map<i32, Boxed>();
  references.set(7, new Boxed(70));
  print(`get ${references.get(7) === null} ${references.get(8) === null}`);

  const vectors: Map<string, Vec2> = new Map<string, Vec2>();
  vectors.set("p", new Vec2(3, 4));
  Context.collect();
  const vector: Vec2 = vectors.getOr("p", new Vec2(0, 0));
  print(`vector ${vector.x},${vector.y} ${vectors.has("p")}`);

  numbers.clear();
  print(`clear ${numbers.size} ${numbers.has(1)}`);
  Context.free(numbers);

  leaveDroppedMap();
  Context.collect();
  print("collected");
}
