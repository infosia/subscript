// corpus: accept/a182-worker-string-message
// purpose: Pins copied string fields across two worker message round trips.
// exercises: Worker messages, string byte copies, FixedArray string slots, Context isolation
// questions: Q35
// tsc: accepts; js-comparable: no Q35: The Worker API has no JavaScript shim.
class StringMessage {
  text: string;
  count: i32;
  tags: FixedArray<string, 2>;

  constructor(text: string, count: i32, tags: FixedArray<string, 2>) {
    this.text = text;
    this.count = count;
    this.tags = tags;
  }
}

function replyToStrings(
  inbox: Inbox<StringMessage>,
  outbox: Outbox<StringMessage>,
): void {
  while (true) {
    const message: StringMessage | null = inbox.wait();
    if (message === null) {
      return;
    }
    const reply: StringMessage = new StringMessage(
      `worker:${message.text}`,
      message.count + 1,
      [message.tags[0], ""],
    );
    outbox.post(reply);
  }
}

export function main(): void {
  const worker: Worker<StringMessage, StringMessage> =
    Worker.spawn(replyToStrings);
  const first: StringMessage = new StringMessage(
    "東京🙂",
    7,
    ["親", "first"],
  );
  print(`sent=${first.text} bytes=${first.text.length}`);
  worker.post(first);
  first.text = "parent-only";

  const second: StringMessage = new StringMessage(
    "café",
    20,
    ["deux", "second"],
  );
  worker.post(second);
  worker.close();
  worker.join();

  const firstReply: StringMessage | null = worker.poll();
  if (firstReply !== null) {
    print(
      `reply1=${firstReply.text} bytes=${firstReply.text.length} count=${firstReply.count} tags=[${firstReply.tags[0]},${firstReply.tags[1]}]`,
    );
  }
  const secondReply: StringMessage | null = worker.poll();
  if (secondReply !== null) {
    print(
      `reply2=${secondReply.text} bytes=${secondReply.text.length} count=${secondReply.count} tags=[${secondReply.tags[0]},${secondReply.tags[1]}]`,
    );
  }
  print(`parent=${first.text} count=${first.count} tags=[${first.tags[0]},${first.tags[1]}]`);
}
