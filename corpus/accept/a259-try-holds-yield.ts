// corpus: accept/a259-try-holds-yield
// purpose: A try block in a generator holds a yield, and its catch handles a throw after the resume inside the generator body.
// exercises: throw, try-catch, generator, yield, generator-driven-for-of, instanceof-narrowing
// questions: Q9, Q11
// tsc: accepts; js-comparable: yes
function* steps(limit: i32): Generator<i32> {
  let total: i32 = 0;
  for (let i: i32 = 0; i < limit; i++) {
    try {
      yield i;
      total += i;
      if (i === 1) {
        throw new Error(`step ${i} failed`);
      }
      print(`steps:after yield ${i} total=${total}`);
    } catch (e) {
      if (e instanceof Error) {
        print(`steps:caught ${e.message} total=${total}`);
      }
      yield 100 + i;
    }
  }
  print(`steps:done total=${total}`);
}

function* spans(): Generator<i32> {
  let seen: i32 = 0;
  try {
    yield 1;
    seen += 1;
    yield 2;
    seen += 1;
    throw new TypeError("after two resumes");
  } catch (e) {
    if (e instanceof TypeError) {
      print(`spans:caught ${e.name} ${e.message} seen=${seen}`);
    }
  }
  yield 3;
}

export function main(): void {
  for (const n of steps(3)) {
    print(`main:n=${n}`);
  }
  const g: Generator<i32> = spans();
  let r = g.next();
  while (!r.done) {
    print(`main:spans ${r.value}`);
    r = g.next();
  }
  print("main:end");
}
