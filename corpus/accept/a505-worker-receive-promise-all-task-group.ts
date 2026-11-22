// corpus: accept/a505-worker-receive-promise-all-task-group
// interpreter: no — requires a runtime worker adapter and a second interpreter Context
// purpose: Promise.all and a TaskGroup await the receives of two Workers; each result is read from the stream of its own Worker.
// exercises: Worker.spawn, Worker.receive, Promise.all, TaskGroup, Worker parameter, object field write
// questions: compiler.md §349 rules 1 and 3, §170, Q35
// tsc: accepts; js-comparable: no Q35: The Worker API has no JavaScript shim.
//
// Each Worker posts its own values, so the result of each receive does
// not depend on which Worker posts first. Promise.all gives its results
// in array order. Each TaskGroup task writes one field of `into`.
class Msg {
  value: i32;
  constructor(value: i32) { this.value = value; }
}

class Results {
  left: i32 = 0;
  right: i32 = 0;
}

function hundreds(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  let m: Msg | null = inbox.wait();
  while (m !== null) {
    outbox.post(new Msg(m.value * 100));
    m = inbox.wait();
  }
}

function negate(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  let m: Msg | null = inbox.wait();
  while (m !== null) {
    outbox.post(new Msg(-m.value));
    m = inbox.wait();
  }
}

async function take(w: Worker<Msg, Msg>, into: Results, left: boolean): Promise<void> {
  const m: Msg | null = await w.receive();
  const value: i32 = m !== null ? m.value : 0;
  if (left) { into.left = value; } else { into.right = value; }
}

export async function main(): Promise<void> {
  const a: Worker<Msg, Msg> = Worker.spawn(hundreds);
  const b: Worker<Msg, Msg> = Worker.spawn(negate);
  b.post(new Msg(5));
  a.post(new Msg(4));
  const both: (Msg | null)[] = await Promise.all([a.receive(), b.receive()]);
  for (const m of both) {
    print(m !== null ? `all ${m.value}` : "all null");
  }

  const into: Results = new Results();
  const group: TaskGroup = new TaskGroup();
  group.add(take(a, into, true));
  group.add(take(b, into, false));
  a.post(new Msg(6));
  b.post(new Msg(7));
  await group.join();
  print(`group ${into.left} ${into.right}`);

  a.close();
  b.close();
  const ends: (Msg | null)[] = await Promise.all([a.receive(), b.receive()]);
  print(ends[0] === null && ends[1] === null ? "both ended" : "more");
  a.join();
  b.join();
}
