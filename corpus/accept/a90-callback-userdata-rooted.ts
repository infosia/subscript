// corpus: accept/a90-callback-userdata-rooted
// purpose: Registered callback userdata remains live across Context.collect after all script references are dropped.
// exercises: interop-callback, two-userdata, async-deferred-fire, callback-userdata-rooting, explicit-collection
// questions: Q7, Q13, Q16

class RootedPrimary {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}

class RootedSecondary {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}

function registerRooted(
  device: SubDevice,
  first: RootedPrimary,
  second: RootedSecondary,
): void {
  const info: SubCallbackInfo = new SubCallbackInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null && userdata2 !== null) {
        const primary = userdata1 as RootedPrimary;
        const secondary = userdata2 as RootedSecondary;
        print(`${primary.value}:${secondary.value}:${message.length}`);
      }
    },
    first,
    second,
  );
  subDeviceKickAsync(device, 1, info);
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  let first: RootedPrimary | null = new RootedPrimary(41);
  let second: RootedSecondary | null = new RootedSecondary(7);
  if (first !== null && second !== null) {
    registerRooted(device, first, second);
  }

  // The registering helper's stack frame is gone and these are the final
  // script references. Only Context::callbacks can keep the objects live.
  first = null;
  second = null;
  Context.collect();

  const waits: SubWaitEntry[] = [];
  subDeviceWait(device, waits);
  subDeviceRelease(device);
}
