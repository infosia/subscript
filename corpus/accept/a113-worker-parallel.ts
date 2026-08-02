// corpus: accept/a113-worker-parallel
// purpose: Pins two workers computing disjoint chunks with parent-observed worker-order output.
// exercises: two-workers, concurrent-computation, ordered-parent-aggregation
// questions: Q35

class RangeMessage {
  start: i32;
  end: i32;

  constructor(start: i32, end: i32) {
    this.start = start;
    this.end = end;
  }
}

class CountMessage {
  count: i32;

  constructor(count: i32) {
    this.count = count;
  }
}

function isPrime(value: i32): boolean {
  if (value < 2) {
    return false;
  }
  let divisor: i32 = 2;
  while (divisor * divisor <= value) {
    if (value % divisor === 0) {
      return false;
    }
    divisor += 1;
  }
  return true;
}

function countChunk(inbox: Inbox<RangeMessage>, outbox: Outbox<CountMessage>): void {
  const range: RangeMessage | null = inbox.wait();
  if (range === null) {
    return;
  }
  let count: i32 = 0;
  let value: i32 = range.start;
  while (value < range.end) {
    if (isPrime(value)) {
      count += 1;
    }
    value += 1;
  }
  outbox.post(new CountMessage(count));
}

export function main(): void {
  const first: Worker<RangeMessage, CountMessage> = Worker.spawn(countChunk);
  const second: Worker<RangeMessage, CountMessage> = Worker.spawn(countChunk);

  first.post(new RangeMessage(2, 100));
  second.post(new RangeMessage(100, 200));
  first.close();
  second.close();

  first.join();
  second.join();

  const firstResult: CountMessage | null = first.poll();
  const secondResult: CountMessage | null = second.poll();
  if (firstResult !== null && secondResult !== null) {
    print(`worker0=${firstResult.count}`);
    print(`worker1=${secondResult.count}`);
    print(`total=${firstResult.count + secondResult.count}`);
  }
}
