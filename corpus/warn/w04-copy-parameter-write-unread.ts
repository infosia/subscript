// corpus: warn/w04-copy-parameter-write-unread
// warning: W004
// purpose: Identifies a field write through a by-value @ValueType parameter that nothing reads.
// exercises: value-struct, copy-on-pass, write-only-copy
// questions: Q2, Q17

@ValueType
class Vec2f {
  x: f32;
  y: f32;

  constructor(x: f32, y: f32) {
    this.x = x;
    this.y = y;
  }
}

@ValueType
class Bag {
  pos: Vec2f;

  constructor(pos: Vec2f) {
    this.pos = pos;
  }
}

class HostState {
  bag: Bag;

  constructor(bag: Bag) {
    this.bag = bag;
  }
}

function mutate(bag: Bag, x: f32): void {
  bag.pos = new Vec2f(x, x);
}

export function main(): void {
  const state: HostState = new HostState(new Bag(new Vec2f(9.0, 9.0)));
  mutate(state.bag, 7.0);
  print(`${state.bag.pos.x}`);
  Context.free(state);
}
