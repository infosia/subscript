// corpus: accept/a247-registration-one-shot-paths
// interpreter: no — registers a callback with the synthetic native interop library
// purpose: A one-shot that completes inside the start call and one that completes at a later pump each end one time.
// exercises: interop-callback, callback-registration-explicit-end, two-userdata, foreign-call
// questions: Q13, Q16
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
class Tally {
  count: i32;

  constructor() {
    this.count = 0;
  }
}

function startImmediate(device: SubDevice, tally: Tally): void {
  const info: SubRequestInfo = new SubRequestInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        const state = userdata1 as Tally;
        state.count = state.count + 1;
        print(`immediate ${state.count}:${message.length}`);
      }
    },
    tally,
    null,
  );
  // The start completes before it returns.
  subRequestStart(device, 6, 1, info);
}

function startDeferred(device: SubDevice, tally: Tally): void {
  const info: SubRequestInfo = new SubRequestInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        const state = userdata1 as Tally;
        state.count = state.count + 1;
        print(`deferred ${state.count}:${message.length}`);
      }
    },
    tally,
    null,
  );
  // The start returns without firing; the pump completes it.
  subRequestStart(device, 8, 0, info);
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  const tally: Tally = new Tally();

  startImmediate(device, tally);
  print(`released ${subRequestReleaseCount(device)}`);

  startDeferred(device, tally);
  print(`released ${subRequestReleaseCount(device)}`);

  subRequestPump(device);
  print(`released ${subRequestReleaseCount(device)}`);
  subDeviceRelease(device);
}
