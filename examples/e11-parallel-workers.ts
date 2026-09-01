// example: e11-parallel-workers
// teaches: Divide one computation over four workers, then read each result back in worker order.
// differs-from-typescript: Q35 gives every worker its own Context and copies every message; nothing is shared.
// see: corpus/accept/a112-worker-echo.ts, corpus/accept/a113-worker-parallel.ts, corpus/reject/r106-capturing-lambda-worker-entry.ts, corpus/reject/r109-worker-module-global.ts, collisions.md Q35, stdlib.md §16

// The message type. Q35 and stdlib.md §16.2 restrict a message to a plain
// class of transferable fields, so this one holds two sized integers.
class PrimeRange {
  start: i32;
  end: i32;

  constructor(start: i32, end: i32) {
    this.start = start;
    this.end = end;
  }
}

// The result type. It travels back the same way, as a byte copy that the
// parent Context materializes as an instance of its own.
class PrimeCount {
  count: i32;

  constructor(count: i32) {
    this.count = count;
  }
}

// Ordinary computation, and the reason the work divides: each candidate is
// independent of every other candidate.
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

// The worker entry. stdlib.md §16.3 requires a named module function that
// captures nothing, so the runtime starts it on a thread of its own.
function countPrimes(inbox: Inbox<PrimeRange>, outbox: Outbox<PrimeCount>): void {
  // Q35: wait blocks on the worker's own thread only. It returns null after
  // the parent closes the inbox and the queue drains.
  const range: PrimeRange | null = inbox.wait();
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
  // Q35: post copies the payload bytes, so the parent reads its own instance
  // and the worker keeps this one.
  outbox.post(new PrimeCount(count));
}

export function main(): void {
  // Setup: four workers, four threads, four Contexts. spawn never blocks the
  // parent, so all four run while main continues.
  const worker0: Worker<PrimeRange, PrimeCount> = Worker.spawn(countPrimes);
  const worker1: Worker<PrimeRange, PrimeCount> = Worker.spawn(countPrimes);
  const worker2: Worker<PrimeRange, PrimeCount> = Worker.spawn(countPrimes);
  const worker3: Worker<PrimeRange, PrimeCount> = Worker.spawn(countPrimes);

  // Fan out: one message each, over disjoint ranges. close states that no
  // further input arrives, so each worker's wait returns null and its entry
  // returns.
  worker0.post(new PrimeRange(2, 10000));
  worker1.post(new PrimeRange(10000, 20000));
  worker2.post(new PrimeRange(20000, 30000));
  worker3.post(new PrimeRange(30000, 40000));
  worker0.close();
  worker1.close();
  worker2.close();
  worker3.close();

  // join waits for each worker thread to end. A worker trap surfaces here as
  // trap kind 22, never silently.
  worker0.join();
  worker1.join();
  worker2.join();
  worker3.join();

  // Collect: poll never blocks. Every worker already ended at join, so each
  // result is present.
  const result0: PrimeCount | null = worker0.poll();
  const result1: PrimeCount | null = worker1.poll();
  const result2: PrimeCount | null = worker2.poll();
  const result3: PrimeCount | null = worker3.poll();
  // C7: poll returns PrimeCount | null, so each result narrows before a field
  // read. The four counts and the total are fixed, because the ranges do not
  // overlap and the parent adds them in worker order.
  if (result0 !== null && result1 !== null && result2 !== null && result3 !== null) {
    print(`worker0=${result0.count}`);
    print(`worker1=${result1.count}`);
    print(`worker2=${result2.count}`);
    print(`worker3=${result3.count}`);
    print(`total=${result0.count + result1.count + result2.count + result3.count}`);
  }
}
