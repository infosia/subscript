// corpus: accept/a246-registration-release-inside-callback
// interpreter: no — registers a callback with the synthetic native interop library
// purpose: The adapter ends a registration from inside its own callback, and the userdata stays live for the rest of that call.
// exercises: interop-callback, callback-registration-explicit-end, callback-userdata-rooting, explicit-collection
// questions: Q7, Q13, Q16
// tsc: accepts; js-comparable: no Q7 Q13: The host C boundary has no JavaScript shim.
class ActiveState {
  device: SubDevice;
  total: i32;

  constructor(device: SubDevice) {
    this.device = device;
    this.total = 0;
  }
}

function startInside(device: SubDevice, state: ActiveState): void {
  const info: SubRequestInfo = new SubRequestInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        const active = userdata1 as ActiveState;
        // §111.2: a release from inside the callback is legal. The
        // callback reaches the adapter through its own userdata, because
        // a boundary callback captures nothing.
        subRequestReleaseActive(active.device);
        // §111 rule 6 keeps the userdata rooted while this call runs, so
        // the collection below cannot reclaim it.
        Context.collect();
        active.total = active.total + message.length;
        print(`inside ${active.total}`);
      }
    },
    state,
    null,
  );
  subRequestStart(device, 4, 0, info);
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  let state: ActiveState | null = new ActiveState(device);
  if (state !== null) {
    startInside(device, state);
  }

  state = null;
  Context.collect();

  subRequestPump(device);
  print(`released ${subRequestReleaseCount(device)}`);
  subDeviceRelease(device);
}
