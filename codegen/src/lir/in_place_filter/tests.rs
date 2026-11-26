use super::*;
use subscript_compiler::{check_program, SourceFile};

fn checked(source: &str) -> hir::Module {
    check_program(&[SourceFile::new("decision.ts", source)]).expect("the program checks")
}

/// The lines of the in-place calls, in source order.
fn in_place_lines(source: &str) -> Vec<u32> {
    let mut lines = in_place_filters(&checked(source), InPlaceMode::Normal)
        .into_iter()
        .map(|key| key.1)
        .collect::<Vec<_>>();
    lines.sort_unstable();
    lines
}

// Cost: one check of a short program; under 0.01 s.
#[test]
fn each_form_of_rule_2_is_in_place_and_each_form_of_rule_4_is_not() {
    let source = "function keep(x: i32): boolean { return x > 1; }
function fromParameter(values: i32[]): i32[] { return values.filter(keep); }
export function main(): void {
  const xs: i32[] = [1, 2, 3];
  const a: i32[] = xs.map((x: i32): i32 => x).filter(keep);
  const b: i32[] = [1, 2, 3].filter(keep);
  const mapped: i32[] = xs.map((x: i32): i32 => x);
  const c: i32[] = mapped.filter(keep);
  let changed: i32[] = xs.map((x: i32): i32 => x);
  const d: i32[] = changed.filter(keep);
  const twice: i32[] = xs.map((x: i32): i32 => x);
  const e: i32[] = twice.filter(keep);
  const k = keep;
  const f: i32[] = xs.map((x: i32): i32 => x).filter(k);
  const g: i32[] = xs.slice(0).filter(keep);
  print(`${a.length} ${b.length} ${c.length} ${d.length} ${e.length} ${twice.length} ${f.length} ${g.length} ${fromParameter(xs).length}`);
  changed = [];
}
";
    // Lines 5, 6, and 8: forms 2a, 2a, and 2b. Line 4 (`xs`) is read
    // again; lines 10, 12, 14, 15, and 2 are a `let`, a second read, a
    // function value, a runtime method, and a parameter.
    assert_eq!(in_place_lines(source), vec![5, 6, 8]);
}

// Cost: one check of a short program; under 0.01 s.
#[test]
fn a_generator_an_async_function_and_a_capture_have_no_in_place_form() {
    let source = "function keep(x: i32): boolean { return x > 1; }
function* items(): Generator<i32> { const a: i32[] = [1, 2].filter(keep); yield a.length; }
async function later(): Promise<i32> { const a: i32[] = [1, 2].filter(keep); return a.length; }
export async function main(): Promise<void> {
  const ys: i32[] = [1, 2];
  const captured: i32[] = ys.map((x: i32): i32 => x);
  const read = (): i32 => captured.filter(keep).length;
  print(`${read()} ${await later()}`);
  for (const item of items()) { print(`${item}`); }
}
export function control(): void {
  const a: i32[] = [1, 2].filter(keep);
  print(`${a.length}`);
}
";
    // Only line 12, in the plain function, is in place.
    assert_eq!(in_place_lines(source), vec![12]);
}

// Cost: one check of a short program; under 0.01 s.
#[test]
fn rule_3_takes_each_loop_kind_and_a_lambda_body_as_a_loop() {
    let source = "function keep(x: i32): boolean { return x > 1; }
export function main(): void {
  const xs: i32[] = [1, 2, 3];
  const outer: i32[] = xs.map((x: i32): i32 => x);
  for (let i: i32 = 0; i < 2; i++) { print(`${outer.filter(keep).length}`); }
  const w: i32[] = xs.map((x: i32): i32 => x);
  let n: i32 = 0;
  while (n < 1) { n++; print(`${w.filter(keep).length}`); }
  const dw: i32[] = xs.map((x: i32): i32 => x);
  do { n++; print(`${dw.filter(keep).length}`); } while (n < 2);
  const fo: i32[] = xs.map((x: i32): i32 => x);
  for (const x of xs) { print(`${fo.filter(keep).length + x}`); }
  const lam: i32[] = xs.map((x: i32): i32 => x);
  xs.forEach((x: i32): void => { print(`${lam.filter(keep).length + x}`); });
  xs.forEach((x: i32): void => {
    const inner: i32[] = xs.map((y: i32): i32 => y * 2);
    print(`${inner.filter(keep).length}`);
  });
  for (let i: i32 = 0; i < 2; i++) {
    const body: i32[] = xs.map((x: i32): i32 => x * 2);
    print(`${body.filter(keep).length}`);
  }
}
";
    // Lines 17 and 21: the declaration is in the loop of the call. The
    // other calls run in a loop or a lambda that the declaration is not in.
    assert_eq!(in_place_lines(source), vec![17, 21]);
}

/// `compiler.md` §359 acceptance 3: with rule 3 off, the loop case of
/// `a512` filters its own result in each round.
///
/// Cost: two lowerings and two interpreter runs of `a512`; 0.1 s.
#[test]
fn the_loop_case_prints_another_value_if_the_decision_ignores_rule_3() {
    let source = include_str!("../../../../corpus/accept/a512-in-place-filter.ts");
    let hir = check_program(&[SourceFile::new("a512.ts", source)]).expect("a512 checks");
    let run = |ignore: bool| {
        let mode = if ignore {
            InPlaceMode::IgnoreRule3
        } else {
            InPlaceMode::Normal
        };
        let lowered = lower_module_with_in_place_mode(&hir, mode);
        let output = crate::interpreter::interpret(&lowered.expect("a512 lowers"))
            .expect("a512 runs in the interpreter");
        String::from_utf8(output).expect("UTF-8 output")
    };
    let with_rule = run(false);
    let without_rule = run(true);
    let loop_line = |output: &str| output.lines().nth(12).map(str::to_owned);
    assert_eq!(loop_line(&with_rule).as_deref(), Some("4,4,4"));
    assert_eq!(loop_line(&without_rule).as_deref(), Some("4,2,1"));
    let other_lines = |output: &str| {
        output
            .lines()
            .enumerate()
            .filter(|(index, _)| *index != 12)
            .map(|(_, line)| line.to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(other_lines(&with_rule), other_lines(&without_rule));
}
