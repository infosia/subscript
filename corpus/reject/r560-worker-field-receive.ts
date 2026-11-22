// corpus: reject/r560-worker-field-receive
// purpose: Rejects a Worker class field that a method reads to await a receive; receive() keeps the affinity rules of a Worker.
// exercises: Worker, Worker.receive, class-field-affinity
// questions: compiler.md §349 rule 1, §40, Q35
// tsc: accepts
// expected-error: S100 at the Worker class field

class Msg {
  value: i32 = 0;
}

function echo(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  const m: Msg | null = inbox.wait();
  if (m !== null) { outbox.post(m); }
}

class Client {
  w: Worker<Msg, Msg>;
  constructor(w: Worker<Msg, Msg>) { this.w = w; }
  async next(): Promise<Msg | null> { return await this.w.receive(); }
}

export async function main(): Promise<void> {
  const client: Client = new Client(Worker.spawn(echo));
  client.w.post(new Msg());
  const m: Msg | null = await client.next();
  print(m === null ? "null" : "msg");
}
