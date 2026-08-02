// corpus: reject/r109-worker-module-global
// purpose: Rejects a Context-affine Worker handle stored in a module global.
// exercises: Worker, module-global-affinity
// questions: Q35
// expected-error: S100 at the Worker module global

class Message {
  value: i32 = 0;
}

function echo(inbox: Inbox<Message>, outbox: Outbox<Message>): void {
  const message: Message | null = inbox.wait();
  if (message !== null) {
    outbox.post(message);
  }
}

const worker: Worker<Message, Message> = Worker.spawn(echo);

export function main(): void {
  worker.close();
  worker.join();
}
