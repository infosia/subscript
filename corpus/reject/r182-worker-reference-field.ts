// corpus: reject/r182-worker-reference-field
// purpose: Rejects a worker message class with a reference-class field.
// exercises: Worker.spawn, message-transferability, WorkerContextAffinity
// questions: Q35
// tsc: accepts
// expected-error: S100 at the reference-class field; WorkerContextAffinity block required
class BoxedCount {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }
}

class ReferenceMessage {
  boxed: BoxedCount;

  constructor(boxed: BoxedCount) {
    this.boxed = boxed;
  }
}

function echo(
  inbox: Inbox<ReferenceMessage>,
  outbox: Outbox<ReferenceMessage>,
): void {
  const message: ReferenceMessage | null = inbox.wait();
  if (message !== null) {
    outbox.post(message);
  }
}

export function main(): void {
  const worker: Worker<ReferenceMessage, ReferenceMessage> = Worker.spawn(echo);
  worker.close();
  worker.join();
}
