let ticks: i32 = 0;

function doubled(value: i32): i32 {
  return value * 2;
}

export function update(): void {
  ticks += 1;
  print(`tick=${ticks}, helper=${doubled(ticks)}`);
}

export function main(): void {}
