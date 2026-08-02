// corpus: reject/r111-worker-in-map-value
// purpose: Rejects a Context-affine Worker used as a module-global Map value type.
// exercises: Worker, Map-value, module-global-affinity
// questions: Q35
// tsc-clean-standalone: verified with node_modules/.bin/tsc against prelude/lang.d.ts; stock TypeScript accepts Worker as a Map value type.
// expected-error: S100 at the Worker Map value type argument

class Message {
  value: i32 = 0;
}

function echo(inbox: Inbox<Message>, outbox: Outbox<Message>): void {
  const message: Message | null = inbox.wait();
  if (message !== null) {
    outbox.post(message);
  }
}

const workers: Map<i32, Worker<Message, Message>> = new Map<i32, Worker<Message, Message>>();

export function main(): void {
  const worker: Worker<Message, Message> = Worker.spawn(echo);
  workers.set(1, worker);
  const stored: Worker<Message, Message> | null = workers.get(1) ?? null;
  stored?.close();
  stored?.join();
}
