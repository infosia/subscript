// corpus: accept/a502-worker-receive-each-message
// interpreter: no — requires a runtime worker adapter and a second interpreter Context
// purpose: Each await of a Worker receive gives the next message, and a receive after the Worker ends gives null.
// exercises: Worker.spawn, Worker.post, Worker.receive, Worker.close, Worker.join, Inbox.wait, Outbox.post
// questions: compiler.md §349 rules 1 to 5, Q35
// tsc: accepts; js-comparable: no Q35: The Worker API has no JavaScript shim.
//
// The Worker posts ten times each value that it reads, in the sequence
// of its inbox. A receive completes at its call or at a later step;
// either way it gives the oldest message. After `close()` the inbox
// ends, the Worker returns, and the next receive gives null (rule 5).
class Msg {
  value: i32;
  constructor(value: i32) { this.value = value; }
}

function scale(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  let m: Msg | null = inbox.wait();
  while (m !== null) {
    outbox.post(new Msg(m.value * 10));
    m = inbox.wait();
  }
}

export async function main(): Promise<void> {
  const w: Worker<Msg, Msg> = Worker.spawn(scale);
  w.post(new Msg(1));
  w.post(new Msg(2));
  w.post(new Msg(3));
  for (let k: i32 = 0; k < 3; k += 1) {
    const m: Msg | null = await w.receive();
    print(m !== null ? `got ${m.value}` : "none");
  }
  w.close();
  const end: Msg | null = await w.receive();
  print(end === null ? "ended" : "more");
  w.join();
  print("joined");
}
