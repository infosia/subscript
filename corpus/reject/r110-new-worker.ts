// corpus: reject/r110-new-worker
// purpose: Rejects direct construction of a runtime-created Worker handle.
// exercises: new-Worker, checker-owned-construction-rejection
// questions: Q35
// tsc-status: stock TypeScript also rejects this program because Worker's constructor is private; this is not a tsc-clean pin.
// expected-error: S100 at new Worker

class Message {
  value: i32 = 0;
}

export function main(): void {
  const worker: Worker<Message, Message> = new Worker<Message, Message>();
  worker.close();
  worker.join();
}
