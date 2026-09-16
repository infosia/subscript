// host example: sandbox
// proves: A host runs content it did not write under the sandbox profile, and stops it from another thread.
// see: compiler.md §109

// The host compiles this file with `subscript build --profile sandbox`.
// Under that profile the compiler puts a checkpoint at every function
// entry and on every loop edge, and each checkpoint reads the Context
// interrupt flag.

// `tick` prints one line and then never returns. A host that calls a
// first-party entry has no defence against that shape: the call is
// synchronous and the thread stays inside it. The host's second thread
// sets the interrupt flag, and the run stops at the next checkpoint with
// the `interrupted` trap.
export function tick(): void {
  print("script:tick running");
  // `spin` never passes 1,000,000, so the condition stays true and no
  // overflow occurs. Only the host ends this loop.
  let spin: i32 = 0;
  while (spin >= 0) {
    spin = spin + 1;
    if (spin > 1000000) {
      spin = 1;
    }
  }
}
