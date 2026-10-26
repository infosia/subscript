//! compiler.md §195.7 rule 3: a program with the `undefined` spelling
//! lowers to the LIR of the same program with `null` in each place.
//!
//! The `null` program replaces each `undefined` with `null` and five
//! spaces, so every position in the two LIR texts is the same. Each
//! comparison with a `Map.get` or `find` result is loose, because those
//! results have the `undefined` spelling in both programs (§195.5 rule 1).

use subscript_codegen::lir::lower_module;
use subscript_compiler::{check_program, lir_text::print_module, SourceFile};

const UNDEFINED_PROGRAM: &str = r#"class Box {
  v: i32;
  constructor(v: i32) {
    this.v = v;
  }
}

class Store<V> {
  items: V[] = [];
  add(item: V): void {
    this.items.push(item);
  }
  get(i: i32): V | undefined {
    if (i < 0 || i >= this.items.length) {
      return undefined;
    }
    return this.items[i];
  }
}

function lookup(m: Map<string, Box>, key: string): Box | undefined {
  const hit = m.get(key);
  if (hit == undefined) {
    return undefined;
  }
  return hit;
}

export function main(): void {
  const s = new Store<Box>();
  s.add(new Box(1));
  const a = s.get(0);
  if (a !== undefined) {
    print(`hit ${a.v}`);
  }
  const miss = s.get(5);
  print(`${miss === undefined} ${miss == null} ${miss != undefined}`);
  print(`${(s.get(9) ?? new Box(-1)).v}`);
  let cur = s.get(0);
  cur = s.get(7);
  print(`${cur === undefined}`);
  const fallback: Box | null = null;
  const either: Box | null = s.get(8) ?? fallback;
  print(`${either == null}`);
  const m = new Map<string, Box>();
  m.set("k", new Box(3));
  const found = lookup(m, "k");
  if (found !== undefined) {
    print(`found ${found.v}`);
  }
  const boxes: Box[] = [new Box(4)];
  const f = boxes.find((b: Box): boolean => b.v === 4);
  print(`${f != undefined} ${m.get("z") == undefined}`);
}
"#;

fn lir_text(source: &str) -> String {
    let files = [SourceFile::entry("main.ts", source)];
    let module = check_program(&files).expect("the program checks");
    print_module(&lower_module(&module).expect("the program lowers"))
}

#[test]
fn the_undefined_spelling_lowers_to_the_null_spelling() {
    let null_program = UNDEFINED_PROGRAM.replace("undefined", "null     ");
    assert_ne!(null_program, UNDEFINED_PROGRAM);
    let undefined_text = lir_text(UNDEFINED_PROGRAM);
    let null_text = lir_text(&null_program);
    assert!(
        !undefined_text.contains("Undefined") && !undefined_text.contains("undefined"),
        "the LIR text names the `undefined` spelling"
    );
    assert_eq!(undefined_text, null_text);

    // Control: one changed operand gives a different LIR text.
    let changed = UNDEFINED_PROGRAM.replace("s.get(5)", "s.get(6)");
    assert_ne!(changed, UNDEFINED_PROGRAM);
    assert_ne!(lir_text(&changed), undefined_text);
}
