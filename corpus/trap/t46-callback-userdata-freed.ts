// corpus: trap/t46-callback-userdata-freed
// purpose: A deferred callback traps before script entry when registered userdata was explicitly freed.
// exercises: interop-callback, async-deferred-fire, Context.free, callback-userdata-fire-check
// questions: Q6, Q13
// tier-policy: both tiers trap with freed-handle diagnostics enabled
// expected-trap: callback-userdata-freed at the freed userdata allocation site

class FreedCallbackUserdata {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}

function registerFreed(device: SubDevice, userdata: FreedCallbackUserdata): void {
  const info: SubCallbackInfo = new SubCallbackInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        const value = userdata1 as FreedCallbackUserdata;
        print(`${value.value}:${message.length}`);
      }
    },
    userdata,
    null,
  );
  subDeviceKickAsync(device, 2, info);
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  const userdata: FreedCallbackUserdata = new FreedCallbackUserdata(29);
  registerFreed(device, userdata);
  print("registered");

  Context.free(userdata);
  print("freed");

  const waits: SubWaitEntry[] = [];
  subDeviceWait(device, waits);
  print("after fire");
}
