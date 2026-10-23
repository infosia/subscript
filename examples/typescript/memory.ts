// Memory. In JavaScript a garbage collector runs when it wants to. In
// subscript no collector runs by itself. This script spawns bullets in a
// game and shows the three ways their memory comes back.

class Bullet {
  id: i32;
  constructor(id: i32) {
    this.id = id;
  }
}

// A class with a `[Symbol.dispose]` method can go in a `using`
// declaration. The method runs when the block ends.
class SoundChannel {
  name: string;
  constructor(name: string) {
    this.name = name;
    print(`open ${name}`);
  }
  [Symbol.dispose](): void {
    print(`close ${this.name}`);
  }
}

export function main(): void {
  // Way 1: write no release code. The objects stay in memory until the
  // host collects. A game host collects between two frames. A program
  // that never frees memory is correct, but it uses more memory.
  const bullets: Bullet[] = [];
  for (let i: i32 = 0; i < 3; i += 1) {
    bullets.push(new Bullet(i));
  }
  print(`${bullets.length} bullets in flight`);

  // Way 2: `Context.free` releases one object at once. After the call,
  // do not use the object. If you read it, the compiler gives a warning,
  // and the read stops the program.
  const spark = new Bullet(99);
  print(`spark ${spark.id}`);
  Context.free(spark);

  // Way 3: `using` runs `[Symbol.dispose]` at the end of the block, in
  // the reverse order of the declarations.
  {
    using music = new SoundChannel("music");
    using effects = new SoundChannel("effects");
    print(`play ${music.name} and ${effects.name}`);
  }

  // A script can also collect by itself. The collector frees each object
  // that no variable reaches.
  Context.collect();
  print(`first bullet ${bullets[0].id}`);
}
