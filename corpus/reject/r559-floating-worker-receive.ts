// corpus: reject/r559-floating-worker-receive
// purpose: Rejects a dropped Worker receive handle; the handle follows the rules of a Promise value.
// exercises: dropped-async-method-handle, Worker.receive
// questions: compiler.md §349 rule 1, §70, Q35
// tsc: accepts
// expected-error: S013 at the dropped receive call

class Msg {
  value: i32 = 0;
}

function echo(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  const m: Msg | null = inbox.wait();
  if (m !== null) { outbox.post(m); }
}

export async function main(): Promise<void> {
  const w: Worker<Msg, Msg> = Worker.spawn(echo);
  w.post(new Msg());
  w.receive();
  w.close();
  w.join();
}
