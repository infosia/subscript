// corpus: accept/a319-namespace-async-worker/lib
// purpose: Exports async functions and a Worker entry.
// exercises: module-export, async-function, worker-message
// questions: compiler.md §148, Q35
// tsc: accepts
export async function f(): Promise<i32> { return 3; }
export async function g<T>(value: T): Promise<T> { return value; }
export class Message {
  value: i32;
  constructor(value: i32) { this.value = value; }
}
export function entry(inbox: Inbox<Message>, outbox: Outbox<Message>): void {
  const message: Message | null = inbox.wait();
  if (message !== null) { outbox.post(message); }
}
