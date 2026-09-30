// corpus: reject/r281-affine-new-map
// purpose: Rejects a Context-affine Worker as a type argument of new Map.
// exercises: Worker, Map-value, new-type-arguments, function-local-affinity
// questions: Q35
// tsc: accepts
// expected-error: S100 at line 13, the Worker Map value type argument

class Message {
  value: i32 = 0;
}

export function main(): void {
  const m = new Map<i32, Worker<Message, Message>>();
  m.clear();
}
