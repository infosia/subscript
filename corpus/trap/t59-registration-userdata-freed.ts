// corpus: trap/t59-registration-userdata-freed
// purpose: A deferred callback through an explicit-lifetime registration traps before script entry when its userdata was freed.
// exercises: interop-callback, callback-registration-explicit-end, Context.free, callback-userdata-fire-check
// questions: Q6, Q13
// tier-policy: both tiers trap with freed-handle diagnostics enabled
// expected-trap: callback-userdata-freed at the freed userdata allocation site

class RegisteredUserdata {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}

function startRequest(
  device: SubDevice,
  userdata: RegisteredUserdata,
  immediate: i32,
): void {
  const info: SubRequestInfo = new SubRequestInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        const state = userdata1 as RegisteredUserdata;
        print(`${state.value}:${message.length}`);
      }
    },
    userdata,
    null,
  );
  subRequestStart(device, 2, immediate, info);
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);

  // A request that completes and ends before the failing one starts, so
  // the printed count states that a registration really existed.
  const warm: RegisteredUserdata = new RegisteredUserdata(7);
  startRequest(device, warm, 1);
  print(`released ${subRequestReleaseCount(device)}`);

  const userdata: RegisteredUserdata = new RegisteredUserdata(29);
  startRequest(device, userdata, 0);
  print("registered");

  Context.free(userdata);
  print("freed");

  subRequestPump(device);
  print("after fire");
}
