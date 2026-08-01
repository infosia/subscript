// corpus: accept/a110-async-method-receiver
// purpose: Pins async method receiver state and lifetime across nested suspension and explicit collection.
// exercises: async-instance-method, this-receiver, nested-method-await, receiver-frame-root
// questions: R13, Q34, C8

class Session {
  state: i32;

  constructor() {
    this.state = 1;
  }

  async sibling(delta: i32): Promise<i32> {
    print(`sibling:start=${this.state}`);
    this.state += delta;
    await Context.suspend();
    print(`sibling:resume=${this.state}`);
    this.state += 10;
    return this.state;
  }

  async run(increment: i32): Promise<i32> {
    print(`run:start=${this.state}`);
    this.state += increment;
    await Context.suspend();
    print(`run:resume=${this.state}`);
    const siblingValue: i32 = await this.sibling(3);
    print(`run:sibling=${siblingValue}`);
    this.state += 1;
    return this.state;
  }
}

function receiver(): Session {
  print("receiver:evaluated");
  return new Session();
}

function argument(): i32 {
  print("argument:evaluated");
  Context.collect();
  return 1;
}

export async function main(): Promise<void> {
  print("main:kick");
  const result: i32 = await receiver().run(argument());
  print(`main:done=${result}`);
}

export async function collector(): Promise<void> {
  print("collector:collect");
  Context.collect();
  print("collector:done");
}
