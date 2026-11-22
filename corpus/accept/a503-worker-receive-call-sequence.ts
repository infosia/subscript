// corpus: accept/a503-worker-receive-call-sequence
// interpreter: no — requires a runtime worker adapter and a second interpreter Context
// purpose: Receives give messages in call sequence, whether each completes at its call or waits, and whatever the await order.
// exercises: Worker.spawn, Worker.post, Worker.receive, held-handle, await-order
// questions: compiler.md §349 rules 1 to 3 and 5, Q35
// tsc: accepts; js-comparable: no Q35: The Worker API has no JavaScript shim.
//
// Part one: the Worker ended with both messages in its outbox before the
// calls, so each receive completes at its call (rule 2). Part two: the
// Worker waits on its inbox when both receives are called, so both wait,
// and the oldest receive gets the oldest message (rule 3). In each part
// the second handle is awaited first; the results stay in call sequence.
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

function show(label: string, first: Msg | null, second: Msg | null): void {
  const a: string = first !== null ? `${first.value}` : "null";
  const b: string = second !== null ? `${second.value}` : "null";
  print(`${label}: first ${a} second ${b}`);
}

export async function main(): Promise<void> {
  const ended: Worker<Msg, Msg> = Worker.spawn(echo);
  ended.post(new Msg(1));
  ended.post(new Msg(2));
  ended.close();
  ended.join();
  const p1: Promise<Msg | null> = ended.receive();
  const p2: Promise<Msg | null> = ended.receive();
  const b1: Msg | null = await p2;
  const a1: Msg | null = await p1;
  show("ended", a1, b1);
  const rest: Msg | null = await ended.receive();
  print(rest === null ? "ended: null" : "ended: more");

  const runs: Worker<Msg, Msg> = Worker.spawn(echo);
  const q1: Promise<Msg | null> = runs.receive();
  const q2: Promise<Msg | null> = runs.receive();
  runs.post(new Msg(3));
  runs.post(new Msg(4));
  const b2: Msg | null = await q2;
  const a2: Msg | null = await q1;
  show("runs", a2, b2);
  runs.close();
  const last: Msg | null = await runs.receive();
  print(last === null ? "runs: null" : "runs: more");
  runs.join();
}
