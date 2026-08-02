class PrimeRange {
  start: i32;
  end: i32;

  constructor(start: i32, end: i32) {
    this.start = start;
    this.end = end;
  }
}

class PrimeCount {
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

function countPrimes(inbox: Inbox<PrimeRange>, outbox: Outbox<PrimeCount>): void {
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
  outbox.post(new PrimeCount(count));
}

export function main(): void {
  const worker0: Worker<PrimeRange, PrimeCount> = Worker.spawn(countPrimes);
  const worker1: Worker<PrimeRange, PrimeCount> = Worker.spawn(countPrimes);
  const worker2: Worker<PrimeRange, PrimeCount> = Worker.spawn(countPrimes);
  const worker3: Worker<PrimeRange, PrimeCount> = Worker.spawn(countPrimes);

  worker0.post(new PrimeRange(2, 10000));
  worker1.post(new PrimeRange(10000, 20000));
  worker2.post(new PrimeRange(20000, 30000));
  worker3.post(new PrimeRange(30000, 40000));
  worker0.close();
  worker1.close();
  worker2.close();
  worker3.close();

  worker0.join();
  worker1.join();
  worker2.join();
  worker3.join();

  const result0: PrimeCount | null = worker0.poll();
  const result1: PrimeCount | null = worker1.poll();
  const result2: PrimeCount | null = worker2.poll();
  const result3: PrimeCount | null = worker3.poll();
  if (result0 !== null && result1 !== null && result2 !== null && result3 !== null) {
    print(`worker0=${result0.count}`);
    print(`worker1=${result1.count}`);
    print(`worker2=${result2.count}`);
    print(`worker3=${result3.count}`);
    print(`total=${result0.count + result1.count + result2.count + result3.count}`);
  }
}
