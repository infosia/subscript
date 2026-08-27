// corpus: accept/a39-interop-async-capstone
// purpose: Composed Future-shape async — kick returns a future by value, wait writes an out-array and fires a two-userdata callback.
// exercises: interop-struct-return, two-userdata, out-array, async-deferred-fire, as-narrowing, foreign-call
// questions: Q13, Q16
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// §14.5 capstone (compiler.md §14.4/§14.5). Composes the P7.1 shapes into
// the common main-thread-driven async model: subDeviceKickAsync returns a
// future BY VALUE (§14.2) while registering a callback-info with TWO
// userdata slots (§14.4), storing it without firing (the a35 deferred
// model). subDeviceWait takes an OUT-ARRAY of SubWaitEntry (§14.3): the
// callee writes each entry's `completed` flag in the caller's own array
// storage (no copy-back), then fires the registered callback on THIS thread
// (§14.6 main-thread model) delivering both userdata, each narrowed with
// `as` (C3). The two userdata are distinct sinks, so both deliveries are
// separately observable.

class Sink {
  total: i32;
  constructor() {
    this.total = 0;
  }
}

class Counter {
  hits: i32;
  constructor() {
    this.hits = 0;
  }
}

export function main(): void {
  // Q16: self-created handle via subDeviceCreate(null) (chain depth 0).
  const device: SubDevice = subDeviceCreate(null);

  const sink: Sink = new Sink();
  const counter: Counter = new Counter();
  const info: SubCallbackInfo = new SubCallbackInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        const a = userdata1 as Sink;
        a.total = a.total + message.length;
      }
      if (userdata2 !== null) {
        const b = userdata2 as Counter;
        b.hits = b.hits + 1;
      }
    },
    sink,
    counter,
  );

  // Kick: returns a future BY VALUE, stores the callback-info, does NOT fire.
  const f: SubFuture = subDeviceKickAsync(device, 5, info);
  print(`${f.id}`); // 16 = 5*3 + 1 (struct return)

  // Out-array of wait entries (completed = 0 — not yet done).
  const waits: SubWaitEntry[] = [
    new SubWaitEntry(f, 0),
    new SubWaitEntry(subFutureMake(7), 0),
  ];
  print(`${waits[0].completed}`); // 0 — before the wait
  print(`${sink.total}`); // 0 — deferred, not fired yet

  // Wait/process-events: writes each entry's completed flag, then fires the
  // callback delivering both userdata.
  subDeviceWait(device, waits);

  print(`${waits[0].completed}`); // 1 — written by the callee
  print(`${waits[1].completed}`); // 1 — written by the callee
  print(`${sink.total}`); // 2 — message.length = entries(2) + depth(0)
  print(`${counter.hits}`); // 1 — fired once; second userdata delivered
}
