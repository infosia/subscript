// corpus: accept/a509-worker-receive-promise-race-index
// interpreter: no — requires a runtime worker adapter and a second interpreter Context
// purpose: A race over the tagged receives of two Workers takes each message of both, and the result does not depend on the arrival sequence.
// exercises: Worker.spawn, Worker.receive, Worker.close, Worker.join, promise-race, array-store, nullable
// questions: compiler.md §350 rules 8 and 9, §349 rules 1 and 5, Q35
// tsc: accepts; js-comparable: no Q35: The Worker API has no JavaScript shim.
//
// The race gives the value only, so `tagged` returns the index of its
// Worker with the message (rule 9). The handle of the Worker that won
// gets a new `tagged` call; the handle that lost stays in the race and
// takes the next message of its Worker (rule 8). When one Worker ends,
// the race continues over the handle of the other Worker. The loop ends
// when both Workers ended. Each handle that remains has a completion, and
// the program awaits it before the array drops it. The program prints
// only after the loop, so the output does not depend on the arrival
// sequence.
class Msg {
  value: i32;
  constructor(value: i32) { this.value = value; }
}

class Tagged {
  index: i32;
  msg: Msg | null;
  constructor(index: i32, msg: Msg | null) { this.index = index; this.msg = msg; }
}

function five(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  for (let k: i32 = 1; k <= 5; k += 1) { outbox.post(new Msg(k)); }
  let m: Msg | null = inbox.wait();
  while (m !== null) { m = inbox.wait(); }
}

function seven(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  for (let k: i32 = 1; k <= 7; k += 1) { outbox.post(new Msg(k * 10)); }
  let m: Msg | null = inbox.wait();
  while (m !== null) { m = inbox.wait(); }
}

async function tagged(index: i32, w: Worker<Msg, Msg>): Promise<Tagged> {
  return new Tagged(index, await w.receive());
}

export async function main(): Promise<void> {
  const a: Worker<Msg, Msg> = Worker.spawn(five);
  const b: Worker<Msg, Msg> = Worker.spawn(seven);
  a.close();
  b.close();
  const sums: i32[] = [0, 0];
  const counts: i32[] = [0, 0];
  let handles: Promise<Tagged>[] = [tagged(0, a), tagged(1, b)];
  let live: i32 = 2;
  while (live > 0) {
    const t: Tagged = await Promise.race(handles);
    const m: Msg | null = t.msg;
    if (m !== null) {
      sums[t.index] += m.value;
      counts[t.index] += 1;
      const slot: i32 = live === 2 ? t.index : 0;
      handles[slot] = tagged(t.index, t.index === 0 ? a : b);
    } else {
      live -= 1;
      if (live === 1) { handles = [handles[1 - t.index]]; }
    }
  }
  for (const h of handles) { await h; }
  print(`worker 0 sum ${sums[0]} count ${counts[0]}`);
  print(`worker 1 sum ${sums[1]} count ${counts[1]}`);
  a.join();
  b.join();
  print("joined");
}
