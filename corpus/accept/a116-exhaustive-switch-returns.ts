// corpus: accept/a116-exhaustive-switch-returns
// purpose: Connects Q32 switch exhaustiveness and unreachable() to all-paths-return flow.
// exercises: string-literal-union, exhaustive-switch, divergence-flow, unreachable
// questions: Q32, R15
// tsc: accepts
type GPUBufferMapState = "unmapped" | "pending" | "mapped";

function lower(v: GPUBufferMapState): i32 {
  switch (v) {
    case "unmapped": return 1;
    case "pending":  return 2;
    case "mapped":   return 3;
  }
}

function requireNonnegative(value: i32): i32 {
  if (value === 0) {
    return 10;
  }
  if (value > 0) {
    return value;
  }
  unreachable();
}

export function main(): void {
  const unmapped: GPUBufferMapState = "unmapped";
  const pending: GPUBufferMapState = "pending";
  const mapped: GPUBufferMapState = "mapped";
  print(`unmapped=${lower(unmapped)}`);
  print(`pending=${lower(pending)}`);
  print(`mapped=${lower(mapped)}`);
  print(`tail=${requireNonnegative(0)}/${requireNonnegative(4)}`);
}
