// corpus: accept/a504-worker-poll-while-receive-waits
// interpreter: no — requires a runtime worker adapter and a second interpreter Context
// purpose: poll() gives null while a receive waits, and gives the next message after that receive completes.
// exercises: Worker.spawn, Worker.post, Worker.receive, Worker.poll, Worker.join, held-handle
// questions: compiler.md §349 rules 2, 4, 5, and 7, Q35
// tsc: accepts; js-comparable: no Q35: The Worker API has no JavaScript shim.
//
// `h` waits: at its call the Worker waits on its inbox and has posted
// nothing. After `join()` both messages are in the outbox, but no step
// ran, so `h` still waits (rule 4). The first `poll()` gives null by
// rule 7 only: without the rule it gives message 1. The await runs a
// step, `h` takes message 1, and the next `poll()` gives message 2.
class Msg {
  value: i32;
  constructor(value: i32) { this.value = value; }
}

function echo(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  let m: Msg | null = inbox.wait();
  while (m !== null) {
    outbox.post(new Msg(m.value));
    m = inbox.wait();
  }
}

function show(label: string, m: Msg | null): void {
  print(m !== null ? `${label} ${m.value}` : `${label} null`);
}

export async function main(): Promise<void> {
  const w: Worker<Msg, Msg> = Worker.spawn(echo);
  const h: Promise<Msg | null> = w.receive();
  w.post(new Msg(1));
  w.post(new Msg(2));
  w.close();
  w.join();
  show("poll while waiting:", w.poll());
  show("receive:", await h);
  show("poll after:", w.poll());
  show("poll at end:", w.poll());
  show("receive at end:", await w.receive());
}
