// corpus: reject/r235-sandbox-worker
// profile: sandbox
// purpose: Rejects Worker.spawn and the Inbox and Outbox types under the sandbox profile.
// exercises: sandbox-profile, Worker.spawn, Inbox, Outbox
// questions: Q35
// tsc: accepts
// expected-error: S025 at `Inbox`
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
  worker.close();
  worker.join();
}
