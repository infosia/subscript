// corpus: warn/w03-fresh-callback-userdata-loop
// warning: W003
// purpose: Identifies fresh callback userdata registered once per loop iteration.

class LogSink {
  count: i32;

  constructor(count: i32) {
    this.count = count;
  }
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  for (let i: i32 = 0; i < 3; i += 1) {
    const sink: LogSink = new LogSink(i);
    const info: SubCallbackInfo = new SubCallbackInfo(
      (message, userdata1, userdata2) => {
        if (userdata1 !== null) {
          const registered = userdata1 as LogSink;
          registered.count = registered.count + message.length;
        }
      },
      sink,
      null,
    );
    subDeviceSetLogger(device, info);
  }
  subDeviceRelease(device);
}
