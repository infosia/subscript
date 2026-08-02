// corpus: reject/r106-capturing-lambda-worker-entry
// purpose: Rejects a capturing lambda where Worker.spawn requires a directly named module function.
// exercises: Worker.spawn, capturing-lambda-entry, direct-entry-affinity
// questions: Q35, C5
// tsc-clean-standalone: verified with node_modules/.bin/tsc against prelude/lang.d.ts; stock TypeScript accepts the capturing callback.
// expected-error: S100 at the capturing lambda passed to Worker.spawn

class Message {
  value: i32 = 0;
}

export function main(): void {
  const increment: i32 = 1;
  const worker: Worker<Message, Message> = Worker.spawn(
    (inbox: Inbox<Message>, outbox: Outbox<Message>): void => {
      const message: Message | null = inbox.wait();
      if (message !== null) {
        message.value += increment;
        outbox.post(message);
      }
    },
  );
  worker.close();
  worker.join();
}
