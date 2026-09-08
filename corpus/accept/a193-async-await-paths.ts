// corpus: accept/a193-async-await-paths
// purpose: The direct, held, method, generic-function, and generic-method awaits use one continuation protocol.
// exercises: direct-await, held-handle, async-method, generic-async-function, generic-method, settled-await
// questions: §94, §64, §93, Q34
// tsc: accepts; js-comparable: yes
async function unit(tag: string): Promise<i32> {
  print(`unit:${tag}`);
  return 1;
}

async function chatter(): Promise<i32> {
  print("chatter:start");
  await unit("chatter");
  print("chatter:end");
  return 5;
}

async function identity<T>(value: T): Promise<T> {
  print("generic-fn:start");
  await unit("generic-fn");
  print("generic-fn:end");
  return value;
}

class Service {
  base: i32;

  constructor(base: i32) {
    this.base = base;
  }

  async scale(factor: i32): Promise<i32> {
    print("method:start");
    await unit("method");
    print("method:end");
    return this.base * factor;
  }

  async pick<T>(value: T): Promise<T> {
    print("generic-method:start");
    await unit("generic-method");
    print("generic-method:end");
    return value;
  }
}

export async function main(): Promise<void> {
  const noise: Promise<i32> = chatter();
  print("main:held");

  const direct: i32 = await unit("direct");
  print(`direct=${direct}`);

  const held: Promise<i32> = unit("held");
  print("held:created");
  print(`held=${await held}`);

  const service: Service = new Service(10);
  const method: Promise<i32> = service.scale(2);
  print("method:created");
  print(`method=${await method}`);

  const generic: Promise<i32> = identity<i32>(7);
  print("generic-fn:created");
  print(`generic-fn=${await generic}`);

  const picked: Promise<i32> = service.pick<i32>(9);
  print("generic-method:created");
  print(`generic-method=${await picked}`);

  print(`noise=${await noise}`);
}
