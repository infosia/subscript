// corpus: accept/a244-registration-userdata-reclaimed
// interpreter: no — registers a callback with the synthetic native interop library
// purpose: An explicit-lifetime registration roots its userdata, and the host release plus one collection reclaim the graph.
// exercises: interop-callback, callback-registration-explicit-end, callback-userdata-rooting, explicit-collection
// questions: Q7, Q13, Q16
// tsc: accepts; js-comparable: no Q7 Q13: The host C boundary has no JavaScript shim.
class RequestPayload {
  items: i32[];

  constructor(items: i32[]) {
    this.items = items;
  }
}

function makePayload(): RequestPayload {
  const items: i32[] = [];
  // The graph is large enough that the strings this entry allocates
  // after the collection cannot reach the threshold below.
  for (let index: i32 = 0; index < 4096; index = index + 1) {
    items.push(index);
  }
  return new RequestPayload(items);
}

function startRequest(device: SubDevice, payload: RequestPayload): void {
  const info: SubRequestInfo = new SubRequestInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        const state = userdata1 as RequestPayload;
        print(`fired ${state.items.length + message.length}`);
      }
    },
    payload,
    null,
  );
  // The one-shot completes at the next pump, not inside this call.
  subRequestStart(device, 3, 0, info);
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  let payload: RequestPayload | null = makePayload();
  if (payload !== null) {
    startRequest(device, payload);
  }

  // The helper frames are gone and this is the final script reference.
  // Only the registration can keep the graph live.
  payload = null;
  Context.collect();

  // The mark reads the live bytes while the rooted graph is live.
  subRequestMarkLiveBytes(device);
  subRequestPump(device);
  Context.collect();

  print(`released ${subRequestReleaseCount(device)}`);
  print(`reclaimed ${subRequestLiveBytesFellBy(device, 8192)}`);
  subDeviceRelease(device);
}
