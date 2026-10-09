// corpus: accept/a360-class-then-method
// purpose: A class method named then, catch, or finally is an ordinary method call.
// exercises: class, method, Promise.then
// questions: compiler.md §186 rule 5, collisions.md C8
// tsc: accepts; js-comparable: yes
class Box {
  n: i32 = 7;
  then(cb: (v: i32) => void): void { cb(this.n); }
  catch(cb: (v: i32) => void): void { cb(-this.n); }
  finally(): i32 { return this.n * 2; }
}
export async function main(): Promise<void> {
  const b = new Box();
  b.then((v: i32): void => { print(`then ${v}`); });
  b.catch((v: i32): void => { print(`catch ${v}`); });
  print(`finally ${b.finally()}`);
}

// pin: 5875a70c
// pin-dev-jit: Exit 1; 3 errors, the first S013 "Promise combinator `.then(...)` is not in the language" at 14:5.
// pin-c-aot: Exit 1; the same 3 errors before C emission.
// pin-interpreter: Checker rejects with the same 3 errors before lowering.
