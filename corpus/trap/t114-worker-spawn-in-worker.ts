// corpus: trap/t114-worker-spawn-in-worker
// purpose: A Worker Context cannot spawn another Worker.
// exercises: worker, worker-spawn-in-worker, worker-trapped
// questions: compiler.md §275
// tier-policy: both tiers trap; interpreter has no runtime worker adapter
// js-comparable: no Q35: The Worker API has no JavaScript shim.
// expected-trap: worker-trapped at Worker.spawn(leaf) in nested (13:35)
class Msg {
  value: i32 = 0;
}
function leaf(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {}
function nested(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  const child: Worker<Msg, Msg> = Worker.spawn(leaf);
  child.join();
}
export function main(): void {
  const worker: Worker<Msg, Msg> = Worker.spawn(nested);
  print("joining");
  worker.join();
  print("after");
}
