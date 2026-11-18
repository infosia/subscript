// corpus: accept/a478-a-function-with-fewer-parameters-takes-a-c-callback-type
// interpreter: no — calls the synthetic native interop library
// purpose: C callbacks pass the leading arguments to functions with fewer parameters.
// exercises: C-callback, named-function, lambda, parameter-prefix
// questions: compiler.md §328 rules 1 and 2
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.

function messageOnly(message: string): void {
  print(`named: ${message}`);
}

function noParameters(): void {
  print("named: no parameters");
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  subDeviceSetLabel(device, "leading message");
  subDeviceSetLogger(device, new SubCallbackInfo(messageOnly, null, null));
  subDeviceSetLogger(device, new SubCallbackInfo(noParameters, null, null));
  subDeviceSetLogger(device, new SubCallbackInfo((message: string): void => {
    print(`lambda: ${message}`);
  }, null, null));
  subDeviceSetLogger(device, new SubCallbackInfo((): void => {
    print("lambda: no parameters");
  }, null, null));
  subDeviceRelease(device);
}
