// corpus: trap/t60-registration-fire-after-release
// purpose: A fire through a registration the host released enters no script code and records the ended-registration trap.
// exercises: interop-callback, callback-registration-explicit-end, callback-registration-ended
// questions: Q13
// tier-policy: both tiers trap
// expected-trap: callback-registration-ended at position 0, because the refused fire reaches no script site

class Refire {
  device: SubDevice;
  fires: i32;
  constructor(device: SubDevice) {
    this.device = device;
    this.fires = 0;
  }
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  const state: Refire = new Refire(device);
  const info: SubRequestInfo = new SubRequestInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        const active = userdata1 as Refire;
        active.fires = active.fires + 1;
        print(`fire ${active.fires}`);
        // The adapter ends the registration and then fires it again
        // from inside this call. The active call keeps the closed
        // record, so §111 rule 14 refuses the second fire and traps.
        subRequestReleaseAndRefire(active.device);
      }
    },
    state,
    null,
  );

  subRequestStart(device, 1, 1, info);
  print("after start");
}
