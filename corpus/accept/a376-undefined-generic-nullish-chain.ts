// corpus: accept/a376-undefined-generic-nullish-chain
// purpose: A generic class chains Map.get results with ?? and keeps one absence spelling at each step.
// exercises: undefined, nullable, nullish, generics, Map.get
// questions: compiler.md §195.4 rule 1, compiler.md §195.11 rule 5
// tsc: accepts; js-comparable: yes
class Box {
  v: i32;
  constructor(v: i32) {
    this.v = v;
  }
}

class Layers<V> {
  a: Map<string, V> = new Map<string, V>();
  b: Map<string, V> = new Map<string, V>();
  d: V | null = null;
  look(k: string): V | null {
    return this.a.get(k) ?? this.b.get(k) ?? this.d;
  }
  either(k: string): boolean {
    const x = this.a.get(k) ?? this.b.get(k);
    return x !== undefined;
  }
  first(k: string): boolean {
    const x = this.d ?? this.a.get(k);
    return x !== undefined;
  }
  slot(k: string): V | null {
    return this.a.get(k) ?? null;
  }
}

export function main(): void {
  const l = new Layers<Box>();
  l.b.set("k", new Box(2));
  l.a.set("j", new Box(5));
  print(`${l.look("k") !== null} ${l.look("z") !== null}`);
  print(`${l.either("k")} ${l.either("z")}`);
  print(`${l.first("j")} ${l.first("k")}`);
  print(`${l.slot("j") === null} ${l.slot("k") === null}`);
  l.d = new Box(9);
  const hit = l.look("z");
  if (hit !== null) {
    print(`fallback ${hit.v}`);
  }
}
