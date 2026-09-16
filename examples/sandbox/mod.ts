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

// Under the profile the compiler rejects `Context.free` (S023), so
// collection is the one way memory returns, and the host's quota is a
// stop, not a pacer. The two exports below are the two boundaries a
// collect happens at: the host's, and the script's.

class Node {
  label: string;
  value: i32;

  constructor(label: string, value: i32) {
    this.label = label;
    this.value = value;
  }
}

// 1,280 nodes per batch, each node owning one string. The size is
// measured, not chosen: it grows the live set by 86,832 bytes per frame,
// so a host pacer at three quarters of a 1 MiB quota fires on frame 10.
const BATCH_SIZE: i32 = 1280;

// The window keeps the last three batches reachable. A newer batch
// replaces a slot, and the batch that leaves the slot becomes
// unreachable one or two frames after the frame that built it.
const WINDOW_SIZE: i32 = 3;

let windowBatches: Node[][] = [];
let windowNext: i32 = 0;

function buildBatch(): Node[] {
  const batch: Node[] = [];
  let index: i32 = 0;
  while (index < BATCH_SIZE) {
    batch.push(new Node(`node-${index}`, index));
    index = index + 1;
  }
  return batch;
}

// One frame of work. It prints nothing, and it collects nothing: every
// batch it drops stays in the Context until something collects. The
// host's pacer is the only thing that keeps this export under the quota.
export function frame(): void {
  const batch: Node[] = buildBatch();
  if (windowBatches.length < WINDOW_SIZE) {
    windowBatches.push(batch);
  } else {
    windowBatches[windowNext] = batch;
  }
  windowNext = windowNext + 1;
  if (windowNext >= WINDOW_SIZE) {
    windowNext = 0;
  }
}

// The same frame, with the script's own collect as its last statement.
// The live set then stays at the window, and the host collects nothing.
export function frameAndCollect(): void {
  frame();
  Context.collect();
}
