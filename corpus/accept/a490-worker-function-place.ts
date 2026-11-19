// corpus: accept/a490-worker-function-place
// interpreter: no — requires a runtime worker adapter
// purpose: Worker entries discard results, take endpoint prefixes, and evaluate trailing defaults.
// exercises: Worker.spawn, counted-result, function-place, default-parameter
// questions: compiler.md §339 rule 6, Q35
// compiler.md §339: debug dev + ship: 0.449 s; 1 C build.
// tsc: accepts
// js-comparable: no Q35: The Worker API has no JavaScript shim.
class Message {
  value: i32;
  constructor(value: i32) { this.value = value; }
}
function value(i: Inbox<Message>, o: Outbox<Message>): i32 {
  const m = i.wait();
  if (m !== null) { o.post(m); }
  return 7;
}
function* sequence(): Generator<i32> { yield 7; }
function counted(i: Inbox<Message>, o: Outbox<Message>): Generator<i32> {
  const m = i.wait();
  if (m !== null) { o.post(m); }
  return sequence();
}
function one(i: Inbox<Message>): void { i.wait(); }
function none(): void {}
function withDefault(i: Inbox<Message>, o: Outbox<Message>, n: i32 = 44): i32 {
  o.post(new Message(n));
  return n;
}
export function main(): void {
  const a = Worker.spawn<Message, Message>(value);
  a.post(new Message(11)); a.close(); a.join();
  const ar = a.poll(); if (ar !== null) { print(`${ar.value}`); }
  const b = Worker.spawn<Message, Message>(counted);
  b.post(new Message(22)); b.close(); b.join();
  const br = b.poll(); if (br !== null) { print(`${br.value}`); }
  const c = Worker.spawn<Message, Message>(one);
  c.post(new Message(33)); c.close(); c.join(); print("one");
  const d = Worker.spawn<Message, Message>(none);
  d.close(); d.join(); print("none");
  const e = Worker.spawn<Message, Message>(withDefault);
  e.close(); e.join();
  const er = e.poll(); if (er !== null) { print(`${er.value}`); }
}
