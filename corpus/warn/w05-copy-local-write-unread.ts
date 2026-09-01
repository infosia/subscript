// corpus: warn/w05-copy-local-write-unread
// warning: W004
// purpose: Identifies a field write through a local copied from a field that nothing reads.
// exercises: value-struct, copy-on-assign, write-only-copy
// questions: Q2, Q17

@CStruct
class Vec2f {
  x: f32;
  y: f32;

  constructor(x: f32, y: f32) {
    this.x = x;
    this.y = y;
  }
}

@CStruct
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

export function main(): void {
  const state: HostState = new HostState(new Bag(new Vec2f(9.0, 9.0)));
  const alias: Bag = state.bag;
  alias.pos = new Vec2f(5.0, 5.0);
  print(`${state.bag.pos.x}`);
  Context.free(state);
}
