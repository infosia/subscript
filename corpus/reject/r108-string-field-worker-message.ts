// corpus: reject/r108-string-field-worker-message
// purpose: Rejects a worker message class whose innermost field is not transferable.
// exercises: Worker.spawn, message-transferability, innermost-field-diagnostic
// questions: Q35
// tsc: accepts
// expected-error: S100 at the string field
class TextMessage {
  text: string;

  constructor(text: string) {
    this.text = text;
  }
}

function echo(inbox: Inbox<TextMessage>, outbox: Outbox<TextMessage>): void {
  const message: TextMessage | null = inbox.wait();
  if (message !== null) {
    outbox.post(message);
  }
}

export function main(): void {
  const worker: Worker<TextMessage, TextMessage> = Worker.spawn(echo);
  worker.close();
  worker.join();
}
