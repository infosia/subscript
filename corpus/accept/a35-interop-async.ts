// corpus: accept/a35-interop-async
// purpose: Deferred (host-driven) callback — registered now, fired later by a pump; userdata outlives the registration.
// exercises: interop-callback, async-deferred-fire, userdata-lifetime, as-narrowing, foreign-call
// questions: Q13, Q16

// P6.3 async model (compiler.md §13.3). Unlike a28 (subDeviceSetLogger fires
// the callback synchronously inside the registering call), subDeviceOnComplete
// only STORES the callback-info and returns; subDevicePump fires it LATER. The
// program observes the sink after registration (still 0 — not fired) and after
// intervening work (still 0), then after the pump (nonzero). This proves the
// deferred fire and that the userdata (the sink) — and the Context-held
// callback binding behind it — outlive the registering call (the Q13 lifetime
// rule). The userdata crosses the boundary as `object | null` and returns to
// its concrete class through a checked `as` (C3).

class LogSink {
  count: i32;
  constructor() {
    this.count = 0;
  }
}

export function main(): void {
  // Q16: self-created handle via subDeviceCreate(null) (chain depth 0).
  const device: SubDevice = subDeviceCreate(null);

  const sink: LogSink = new LogSink();
  const info: SubCompletionInfo = new SubCompletionInfo(
    (message, userdata) => {
      if (userdata !== null) {
        const s = userdata as LogSink;
        s.count = s.count + message.length;
      }
    },
    sink,
  );

  // Register: the call returns WITHOUT firing the callback.
  subDeviceOnComplete(device, info);
  print(`${sink.count}`); // 0 — deferred, not fired yet

  // Intervening work: submit accumulates a running sum (10+20+30 = 60). No
  // synchronous logger is registered, so nothing fires here either.
  const commands: u32[] = [10, 20, 30];
  subDeviceSubmit(device, commands);
  print(`${sink.count}`); // 0 — still not fired

  // Host-driven deferred fire: the stored callback runs now, observing the
  // work done since registration through message.length = sum + depth = 60.
  subDevicePump(device);
  print(`${sink.count}`); // 60 — fired after the registering call returned
}
