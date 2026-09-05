// corpus: accept/a111-interop-async-method-poll
// interpreter: no — calls the synthetic native interop library
// purpose: Exercises deterministic foreign polling from an async instance method.
// exercises: async-instance-method, foreign-poll, receiver-state, Context.suspend
// questions: R13, Q34, Q1, C8
// tsc: accepts; js-comparable: no C8 Q13: The host C boundary has no JavaScript shim.
class DevicePoller {
  attempt: i32 = 0;

  async poll(): Promise<void> {
    print("method-poll:start");
    while (subDevicePoll(this.attempt) === 0) {
      print(`method-poll:pending=${this.attempt}`);
      this.attempt += 1;
      await Context.suspend();
    }
    print(`method-poll:ready=${this.attempt}`);
  }
}

export async function main(): Promise<void> {
  await new DevicePoller().poll();
}
