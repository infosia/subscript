// corpus: accept/a145-emitted-identifiers
// purpose: Pins separate source and emitter identifier spaces in emitted C.
// exercises: function-source-prefix, local-name-table, coroutine-frame-members, async-resume-symbols
// questions: §66

function temporaryValue(value: i32): i32 {
  return value * 100;
}

function parameterProbe(
  _t0: i32,
  _t1: i32,
  _this: i32,
  _frame: i32,
  _out: i32,
  _f: i32,
  _state: i32,
  g0: i32,
  _L0: i32,
  ctx: i32,
): i32 {
  let total: i32 = 0;
  for (let i: i32 = 0; i < 3; i += 1) {
    {
      const nested: i32 =
        _t0 +
        _t1 +
        _this +
        _frame +
        _out +
        _f +
        _state +
        g0 +
        _L0 +
        ctx +
        temporaryValue(i);
      total += nested;
    }
  }
  return total;
}

function localProbe(): i32 {
  const _t0: i32 = 11;
  const _t1: i32 = 12;
  const _this: i32 = 13;
  const _frame: i32 = 14;
  const _out: i32 = 15;
  const _f: i32 = 16;
  const _state: i32 = 17;
  const g0: i32 = 18;
  const _L0: i32 = 19;
  const ctx: i32 = 20;
  let total: i32 = 0;
  for (let i: i32 = 0; i < 3; i += 1) {
    {
      const nested: i32 =
        _t0 +
        _t1 +
        _this +
        _frame +
        _out +
        _f +
        _state +
        g0 +
        _L0 +
        ctx +
        temporaryValue(i);
      total += nested;
    }
  }
  return total;
}

class ResumeCollision {
  async x(_this: i32): Promise<i32> {
    await Context.suspend();
    return _this + 1;
  }

  x_resume(): i32 {
    return 40;
  }
}

async function f(value: i32): Promise<i32> {
  await Context.suspend();
  return value + 1;
}

function f_resume(): i32 {
  return 50;
}

async function frameProbe(_state: i32, g0: i32): Promise<i32> {
  await Context.suspend();
  return _state + g0;
}

export async function main(): Promise<void> {
  print(`${parameterProbe(1, 2, 3, 4, 5, 6, 7, 8, 9, 10)}`);
  print(`${localProbe()}`);

  const collision: ResumeCollision = new ResumeCollision();
  const methodValue: i32 = await collision.x(2);
  print(`${methodValue},${collision.x_resume()}`);

  const functionValue: i32 = await f(5);
  print(`${functionValue},${f_resume()}`);

  print(`${await frameProbe(7, 8)}`);
}
