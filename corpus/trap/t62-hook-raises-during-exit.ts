// corpus: trap/t62-hook-raises-during-exit
// purpose: A dispose hook that raises while another exception is pending traps with dispose-raised-during-exit; no handler catches the trap.
// exercises: using-declaration, symbol-dispose, exception-exit, dispose-raises, trap-stop
// questions: Q9, §60
// tier-policy: both tiers trap
// expected-trap: dispose-raised-during-exit at the `using` binding of `held`
class Resource {
  label: string;
  fails: boolean;

  constructor(label: string, fails: boolean) {
    this.label = label;
    this.fails = fails;
  }

  [Symbol.dispose](): void {
    print(`dispose ${this.label}`);
    if (this.fails) {
      throw new Error(`${this.label} failed`);
    }
  }
}

function run(): void {
  using outer = new Resource("outer", false);
  using held = new Resource("held", true);
  print("body");
  throw new Error("body failed");
}

export function main(): void {
  try {
    run();
  } catch (e) {
    print("unreached");
  }
}
