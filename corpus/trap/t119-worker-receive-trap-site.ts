// corpus: trap/t119-worker-receive-trap-site
// purpose: A receive after a Worker trap traps the Context at the trap site of the Worker, after the message that the Worker posted first.
// exercises: worker, worker-trapped, trap-position, Worker.receive
// questions: compiler.md §349 rule 6, §152
// tsc: accepts
// js-comparable: no Q35: The Worker API has no JavaScript shim.
// tier-policy: both tiers trap; interpreter has no runtime worker adapter
// expected-trap: worker-trapped at the Worker read (18:14), the position that join() reports for the same Worker
class Msg {
  value: i32;
  constructor(value: i32) { this.value = value; }
}
function crash(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  const m: Msg | null = inbox.wait();
  if (m !== null) {
    outbox.post(new Msg(m.value));
    const xs: i32[] = [1];
    print(`${xs[m.value]}`);
  }
}
export async function main(): Promise<void> {
  const w: Worker<Msg, Msg> = Worker.spawn(crash);
  w.post(new Msg(5));
  const a: Msg | null = await w.receive();
  if (a !== null) { print(`got ${a.value}`); }
  const b: Msg | null = await w.receive();
  print(b === null ? "null" : "msg");
  print("after");
}

// pin: c7ca4b37
// pin-dev-jit: Rejected at the check with S018 "has no async method `receive`" at the first receive (24); no output. Measured at 7f7be71d, which has the code tree of the pin.
// pin-c-aot: Rejected at the check with S018 "has no async method `receive`" at the first receive (24); no build. Measured at 7f7be71d, which has the code tree of the pin.
// pin-interpreter: Rejected at the check with S018 "has no async method `receive`" at the first receive (24); no output. Measured at 7f7be71d, which has the code tree of the pin.
// tsc-version: TypeScript 5.9.2; exit 0.
