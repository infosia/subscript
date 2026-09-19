// corpus: trap/t61-sandbox-callback-bind-quota
// profile: sandbox
// purpose: A Context-lifetime callback-info crossing that passes the allocation quota reports the crossing.
// exercises: sandbox-profile, allocation-quota, interop-callback, trap-position
// questions: Q7, Q13, compiler section 112
// tier-policy: the binding record is the first charge on every tier, because the entry allocates nothing before the crossing; the harness sets a quota under one record
// expected-trap: allocation-quota at the `subDeviceSetLogger` call, which is the crossing of the Context-lifetime callback info

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  const logger: SubCallbackInfo = new SubCallbackInfo(
    (message, userdata1, userdata2) => {
      print(`logged ${message.length}`);
    },
    null,
    null,
  );
  subDeviceSetLogger(device, logger);
}
