// corpus: accept/a54-map-reference-key
// purpose: Proves reference-class Map keys use handle identity, so two
//          equal-shaped instances remain distinct and mutation is safe.
// exercises: map-reference-keys, identity-equality, identity-hashing
// questions: Q24, Q22, C2

class Key {
  id: i32;
  constructor(id: i32) {
    this.id = id;
  }
}

export function main(): void {
  const first: Key = new Key(5);
  const second: Key = new Key(5);
  const map: Map<Key, i32> = new Map<Key, i32>();
  map.set(first, 10);
  map.set(second, 20);
  first.id = 99;
  print(`identity ${map.size} ${map.getOr(first, -1)} ${map.getOr(second, -1)}`);
  print(`distinct ${map.has(new Key(5))} mutated ${map.has(first)}`);
}
