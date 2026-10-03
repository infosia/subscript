// corpus: trap/t82-worker-trap-site
// purpose: A Worker trap reports the read site to its parent.
// exercises: worker, worker-trapped, trap-position
// questions: compiler.md §152
// tier-policy: both tiers trap; interpreter has no runtime worker adapter
// js-comparable: no Q35: The Worker API has no JavaScript shim.
// expected-trap: worker-trapped at the Worker read (16:14; Problem program 9:14)
class Msg {
  value: i32;
  constructor(value: i32) { this.value = value; }
}
function crash(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  const m: Msg | null = inbox.wait();
  if (m !== null) {
    const xs: i32[] = [1];
    print(`${xs[m.value]}`);
  }
}
export function main(): void {
  const w: Worker<Msg, Msg> = Worker.spawn(crash);
  w.post(new Msg(5));
  w.close();
  w.join();
  print("after");
}
