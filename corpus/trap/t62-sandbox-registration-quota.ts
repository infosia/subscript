// corpus: trap/t62-sandbox-registration-quota
// profile: sandbox
// purpose: An explicit-lifetime registration crossing that passes the allocation quota reports the crossing.
// exercises: sandbox-profile, allocation-quota, interop-callback, callback-registration-explicit-end, trap-position
// questions: Q7, Q13, compiler section 112
// tier-policy: the registration record is the first charge on every tier, because the entry allocates nothing before the crossing; the harness sets a quota under one record
// expected-trap: allocation-quota at the `subRequestSubscribe` call, which is the crossing of the explicit-lifetime request info

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  const info: SubRequestInfo = new SubRequestInfo(
    (message, userdata1, userdata2) => {
      print(`notified ${message.length}`);
    },
    null,
    null,
  );
  subRequestSubscribe(device, info);
}
