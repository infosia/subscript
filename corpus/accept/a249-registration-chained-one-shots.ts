// corpus: accept/a249-registration-chained-one-shots
// interpreter: no — registers a callback with the synthetic native interop library
// purpose: A one-shot whose callback starts the next one-shot ends every registration of the chain one time, and one collection reclaims every link.
// exercises: interop-callback, callback-registration-explicit-end, callback-userdata-rooting, explicit-collection
// questions: Q7, Q13, Q16
// tsc: accepts; js-comparable: no Q7 Q13: The host C boundary has no JavaScript shim.
class Link {
  device: SubDevice;
  left: i32;
  items: i32[];

  constructor(device: SubDevice, left: i32) {
    this.device = device;
    this.left = left;
    this.items = [];
    // The graph of one link is large enough that the fall in live
    // bytes below cannot come from the strings this entry allocates.
    for (let index: i32 = 0; index < 4096; index = index + 1) {
      this.items.push(index);
    }
  }
}

function startLink(device: SubDevice, link: Link): void {
  const info: SubRequestInfo = new SubRequestInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        const state = userdata1 as Link;
        print(`link ${state.left}:${state.items.length + message.length}`);
        if (state.left > 1) {
          // The callback starts the next one-shot while this call runs,
          // so the adapter holds two registrations at the same time.
          // The new one-shot fires at the next pump.
          startLink(state.device, new Link(state.device, state.left - 1));
        }
      }
    },
    link,
    null,
  );
  // The one-shot completes at the next pump, not inside this call.
  subRequestStart(device, 2, 0, info);
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  // The helper frame is gone after this call and no script reference is
  // kept, so only the registration can root the first link.
  startLink(device, new Link(device, 3));
  Context.collect();

  // The mark reads the live bytes while one rooted link graph is live.
  subRequestMarkLiveBytes(device);

  subRequestPump(device);
  print(`released ${subRequestReleaseCount(device)}`);
  subRequestPump(device);
  print(`released ${subRequestReleaseCount(device)}`);
  subRequestPump(device);
  print(`released ${subRequestReleaseCount(device)}`);

  Context.collect();
  print(`reclaimed ${subRequestLiveBytesFellBy(device, 8192)}`);
  subDeviceRelease(device);
}
