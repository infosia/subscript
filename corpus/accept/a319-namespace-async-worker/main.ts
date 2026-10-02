// corpus: accept/a319-namespace-async-worker/main
// interpreter: no — requires a runtime worker adapter and a second interpreter Context
// purpose: Resolves namespace calls before await and Worker entry checks.
// exercises: namespace-import, async-function, generic-async-function, Worker.spawn
// questions: compiler.md §148, Q35
// tsc: accepts; js-comparable: no Q35: The Worker API has no JavaScript shim.
import * as ns from "./lib";
import { f, g, entry, Message } from "./lib";
export async function main(): Promise<void> {
  const a: i32 = await ns.f();
  const b: i32 = await f();
  print(`${a} ${b}`);
  const c: i32 = await ns.g<i32>(5);
  const d: i32 = await g<i32>(5);
  print(`${c} ${d}`);
  const first: Worker<Message, Message> = Worker.spawn(ns.entry);
  const second: Worker<Message, Message> = Worker.spawn(entry);
  first.post(new Message(37));
  second.post(new Message(37));
  first.close();
  second.close();
  first.join();
  second.join();
  const x: Message | null = first.poll();
  const y: Message | null = second.poll();
  if (x !== null && y !== null) {
    print(`echo=${x.value} echo=${y.value}`);
  }
}
