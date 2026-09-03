// corpus: reject/r183-worker-growable-array-field
// purpose: Rejects a worker message class with a growable-array field.
// exercises: Worker.spawn, message-transferability, WorkerContextAffinity
// questions: Q35
// tsc: accepts
// expected-error: S100 at the growable-array field; WorkerContextAffinity block required
class ArrayMessage {
  values: i32[];

  constructor(values: i32[]) {
    this.values = values;
  }
}

function echo(
  inbox: Inbox<ArrayMessage>,
  outbox: Outbox<ArrayMessage>,
): void {
  const message: ArrayMessage | null = inbox.wait();
  if (message !== null) {
    outbox.post(message);
  }
}

export function main(): void {
  const worker: Worker<ArrayMessage, ArrayMessage> = Worker.spawn(echo);
  worker.close();
  worker.join();
}
