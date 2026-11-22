// corpus: accept/a506-worker-receive-waits-through-join
// interpreter: no — requires a runtime worker adapter and a second interpreter Context
// purpose: A receive that waits through join() completes at the next step with the message, and the next receive gives null.
// exercises: Worker.spawn, Worker.post, Worker.receive, Worker.close, Worker.join, held-handle
// questions: compiler.md §349 rules 4, 5, and 7, Q35
// tsc: accepts; js-comparable: no Q35: The Worker API has no JavaScript shim.
//
// At the call of `receive()` the Worker waits on its inbox and has
// posted nothing, so the receive waits. `join()` blocks until the Worker
// ends and does not read the outbox; the await runs the step that
// delivers the message.
class Msg {
  value: i32;
  constructor(value: i32) { this.value = value; }
}

function plusOne(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  const m: Msg | null = inbox.wait();
  if (m !== null) {
    outbox.post(new Msg(m.value + 1));
  }
}

export async function main(): Promise<void> {
  const w: Worker<Msg, Msg> = Worker.spawn(plusOne);
  const h: Promise<Msg | null> = w.receive();
  w.post(new Msg(41));
  w.close();
  w.join();
  print("joined");
  const m: Msg | null = await h;
  print(m !== null ? `got ${m.value}` : "none");
  const end: Msg | null = await w.receive();
  print(end === null ? "ended" : "more");
}
