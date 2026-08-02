// corpus: reject/r107-async-worker-entry
// purpose: Rejects an async function as a Worker.spawn entry.
// exercises: Worker.spawn, async-entry, synchronous-entry-shape
// questions: Q35, Q34
// tsc-clean-standalone: verified with node_modules/.bin/tsc against prelude/lang.d.ts; stock TypeScript permits a Promise-returning function where a void-returning callback is expected.
// expected-error: S100 at the async function passed to Worker.spawn

class Message {
  value: i32 = 0;
}

async function asyncEntry(inbox: Inbox<Message>, outbox: Outbox<Message>): Promise<void> {
  const message: Message | null = inbox.wait();
  if (message !== null) {
    outbox.post(message);
  }
}

export function main(): void {
  const worker: Worker<Message, Message> = Worker.spawn(asyncEntry);
  worker.close();
  worker.join();
}
