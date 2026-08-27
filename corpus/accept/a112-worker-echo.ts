// corpus: accept/a112-worker-echo
// purpose: Pins one deterministic parent-to-worker-to-parent echo round-trip.
// exercises: Worker.spawn, Worker.post, Inbox.wait, Outbox.post, Worker.close, Worker.join, Worker.poll
// questions: Q35
// tsc: accepts; js-comparable: no Q35: The Worker API has no JavaScript shim.
class EchoMessage {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }
}

function echo(inbox: Inbox<EchoMessage>, outbox: Outbox<EchoMessage>): void {
  const message: EchoMessage | null = inbox.wait();
  if (message !== null) {
    outbox.post(message);
  }
}

export function main(): void {
  const worker: Worker<EchoMessage, EchoMessage> = Worker.spawn(echo);
  worker.post(new EchoMessage(37));
  worker.close();
  worker.join();
  const reply: EchoMessage | null = worker.poll();
  if (reply !== null) {
    print(`echo=${reply.value}`);
  }
}
