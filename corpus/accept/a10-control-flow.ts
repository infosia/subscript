// corpus: accept/a10-control-flow
// purpose: Exercises the founding control-flow statements in one terminating program.
// exercises: if, while, for, switch, break, continue
// questions: Q1, Q12

export function main(): void {
  let total: i32 = 0;
  let cursor: i32 = 0;

  while (cursor < 4) {
    if (cursor === 2) {
      cursor += 1;
      continue;
    }
    total += cursor;
    cursor += 1;
  }

  for (let index: i32 = 0; index < 6; index += 1) {
    if (index === 4) {
      break;
    }
    total += index;
  }

  switch (total) {
    case 10:
      total += 100;
      break;
    default:
      total = -1;
      break;
  }

  print(`${total}`);
}
