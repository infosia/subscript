// corpus: accept/a245-registration-two-crossings
// interpreter: no — registers a callback with the synthetic native interop library
// purpose: Two crossings with one callback and one userdata create two registrations, and ending one keeps the other valid.
// exercises: interop-callback, callback-registration-explicit-end, callback-userdata-rooting, explicit-collection
// questions: Q7, Q13, Q16
// tsc: accepts; js-comparable: no Q7 Q13: The host C boundary has no JavaScript shim.
class Sink {
  hits: i32;

  constructor() {
    this.hits = 0;
  }
}

function registerTwice(device: SubDevice, sink: Sink): void {
  const listener: SubLogCallback = (message, userdata1, userdata2) => {
    if (userdata1 !== null) {
      const state = userdata1 as Sink;
      state.hits = state.hits + message.length;
      print(`hit ${state.hits}`);
    }
  };
  // The one-shot completes inside the start call and ends its own
  // registration.
  const first: SubRequestInfo = new SubRequestInfo(listener, sink, null);
  subRequestStart(device, 2, 1, first);
  // The same callback and the same userdata cross a second time. §111
  // rule 3 forbids interning, so this is a second registration and it
  // ends apart from the first.
  const second: SubRequestInfo = new SubRequestInfo(listener, sink, null);
  subRequestSubscribe(device, second);
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  let sink: Sink | null = new Sink();
  if (sink !== null) {
    registerTwice(device, sink);
  }

  // The first registration ended. Only the second can keep the sink live.
  sink = null;
  Context.collect();

  subRequestNotify(device, 5);
  subRequestPump(device);
  print(`released ${subRequestReleaseCount(device)}`);

  subRequestUnsubscribe(device);
  subRequestPump(device);
  print(`released ${subRequestReleaseCount(device)}`);
  subDeviceRelease(device);
}
