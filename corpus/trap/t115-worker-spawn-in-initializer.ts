// corpus: trap/t115-worker-spawn-in-initializer
// purpose: A Worker traps when its module initializer spawns another Worker.
// exercises: worker, module-initializer, worker-spawn-in-worker, worker-trapped
// questions: compiler.md §275 rule 8
// tier-policy: both tiers trap; interpreter has no runtime worker adapter
// js-comparable: no Q35: The Worker API has no JavaScript shim.
// expected-trap: worker-trapped at Worker.spawn(entry) in start (13:36)
class Msg {
  value: i32 = 0;
}
function entry(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {}
function start(): i32 {
  const worker: Worker<Msg, Msg> = Worker.spawn(entry);
  print("joining initializer");
  worker.join();
  return 1;
}
const initialized: i32 = start();
export function main(): void {
  print(`${initialized}`);
}
