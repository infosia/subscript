// corpus: accept/a477-c-bytes-become-a-valid-string
// interpreter: no — calls the synthetic native interop library
// purpose: Decodes invalid C bytes at string-view entries with maximal-subpart replacement.
// exercises: string-view, callback, callback-registration-explicit-end, pair-read-back, utf8
// questions: Q13, Q21
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// compiler.md §327 rules 1 and 5.

class ExpectedText {
  text: string;
  constructor(text: string) { this.text = text; }
}

function show(text: string, expected: string): void {
  print(`${text.length}`);
  // Q21 indices are byte offsets. Each replacement takes three bytes.
  print(`${text.codePointAt(0)}`);
  for (let at: i32 = 1; at < text.length - 1; at += 3) {
    print(`${text.codePointAt(at)}`);
  }
  print(`${text.codePointAt(text.length - 1)}`);
  print(text.slice(1, -1));
  for (const point of text) print(point);
  print(text.toUpperCase());
  const parts = text.split("");
  print(`${parts.length}`);
  for (const part of parts) print(part);
  print(JSON.stringify(text));
  print(`${text === expected}`);
}

export function main(): void {
  const expected: string[] = ["a�b", "a�b", "a�b", "a���b", "a��b"];
  const state = new ExpectedText("");
  for (let sequence: i32 = 0; sequence < 5; sequence += 1) {
    state.text = expected[sequence];
    print(`A ${sequence}`);
    const record = new SubDescReadStr("before", 0);
    sub327Fill(record, sequence);
    show(record.label, state.text);
    print(`B ${sequence}`);
    sub327Callback(new SubCallbackInfo((message, userdata, userparam) => {
      if (userdata !== null) show(message, (userdata as ExpectedText).text);
    }, state, null), sequence);
    print(`C ${sequence}`);
    sub327Registration(new SubRequestInfo((message, userdata, userparam) => {
      if (userdata !== null) show(message, (userdata as ExpectedText).text);
    }, state, null), sequence);
    print(`D ${sequence}`);
    const items: Sub325Text[] = [];
    const received = new Sub325Texts(0, items);
    sub327Receive(received, sequence);
    Context.collect();
    show(received.items[0].label, state.text);
  }
  const kept = new SubDescReadStr("valid é😀", 0);
  sub327Keep(kept);
  sub327Keep(kept);
  print(kept.label);
  print(`${kept.n}`);
}
