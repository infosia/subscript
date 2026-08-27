// corpus: reject/r110-new-worker
// purpose: Rejects direct construction of a runtime-created Worker handle.
// exercises: new-Worker, checker-owned-construction-rejection
// questions: Q35
// tsc: rejects TS2673
// expected-error: S100 at new Worker

class Message {
  value: i32 = 0;
}

export function main(): void {
  const worker: Worker<Message, Message> = new Worker<Message, Message>();
  worker.close();
  worker.join();
}
