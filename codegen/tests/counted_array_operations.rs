//! Counted mutations preserve values after replaced holders end (§171).

use subscript_codegen::{
    interpreter::interpret, lir::lower_module, run_c_aot, run_jit, ReloadSession,
};
use subscript_compiler::{check_program, SourceFile};

#[test]
fn element_operations_and_borrowed_results_agree_in_three_tiers() {
    let source = r#"
async function work(value: i32): Promise<i32> { return value; }
async function exercise(): Promise<void> {
  const live = work(7);
  await live;
  {
    const other = work(3);
    await other;
    const jobs: Promise<i32>[] = [other, live, live];
    jobs.copyWithin(0, 1, 3);
    jobs.copyWithin(1, 0, 2);
    jobs.fill(live, -2);
    jobs.unshift(live);
    const reversed = jobs.reverse();
    const filled = reversed.fill(live);
    const copied = filled.copyWithin(0, 1);
    const temporary = [...[live]];
    print(`${await temporary[0]}`);
    const read = copied.at(0);
    copied.at(0);
    const sliced = copied.slice(-2);
    const spread = [...sliced];
    const combined = spread.concat(sliced);
    jobs.fill(work(8));
    await jobs[0];
    jobs.pop();
    jobs.shift();
    const removed = combined.splice(0, 1);
    print(`${await removed[0]} ${await read} ${await combined[0]}`);
  }
  print(`${await work(99)} ${await live}`);
}
export async function main(): Promise<void> { await exercise(); }
"#;
    let files = [SourceFile::new("counted-operations.ts", source)];
    let hir = check_program(&files).expect("checked fixture");
    let lir = lower_module(&hir).expect("verified fixture");
    let expected = b"7\n7 7 7\n99 7\n";
    assert_eq!(interpret(&lir).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("C AOT"), expected);
    let mut session = ReloadSession::new(&files).expect("session");
    session.call_main().expect("root starts");
    while session.async_pending() != 0 {
        session.async_step().expect("checkpoint");
    }
    assert!(session.async_tasks().is_empty(), "no element owner remains");
}

#[test]
fn yielded_holders_survive_generator_completion_in_three_tiers() {
    let files = [SourceFile::new(
        "yielded-holders.ts",
        include_str!("../../corpus/accept/a339-counted-generator-holders.ts"),
    )];
    let hir = check_program(&files).expect("checked corpus");
    let lir = lower_module(&hir).expect("verified corpus");
    let expected = include_bytes!("../../corpus/accept/a339-counted-generator-holders.expected");
    assert_eq!(interpret(&lir).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("C AOT"), expected);
    let mut session = ReloadSession::new(&files).expect("generator session");
    session.call_main().expect("root starts");
    while session.async_pending() != 0 {
        session.async_step().expect("checkpoint");
    }
    assert!(session.async_tasks().is_empty(), "no yielded owner remains");
}

#[test]
fn temporary_call_inputs_release_their_owners_after_the_result_copy() {
    let source = r#"
async function work(value: i32): Promise<i32> { return value; }
async function use(): Promise<void> {
  const h11 = work(11); await h11;
  const h12 = work(12); await h12;
  const h13 = work(13); await h13;
  const h14 = work(14); await h14;
  const h15 = work(15); await h15;
  const h16 = work(16); await h16;
  const h17 = work(17); await h17;
  const h18 = work(18); await h18;
  const h19 = work(19); await h19;
  const h20 = work(20); await h20;
  const h21 = work(21); await h21;
  const h22 = work(22); await h22;
  const h23 = work(23); await h23;
  const h24 = work(24); await h24;
  const h25 = work(25); await h25;
  const h26 = work(26); await h26;

  const replacement = work(17);
  const inserted = work(24);
  await replacement;
  await inserted;
  const read = [h11].at(0);
  const reversed = [h12].reverse();
  const sliced = [h13].slice();
  const combined = [h14].concat([h15]);
  const filled = [h16].fill(replacement);
  const copied = [h18].concat([h19]).copyWithin(0, 1);
  const popped = [h20].pop();
  const shifted = [h21].shift();
  const removed = [h22].splice(0, 1);
  [h23].unshift(inserted);
  [h25].at(0);
  [h26].reverse();
  print(`${await read} ${await reversed[0]} ${await sliced[0]}`);
  print(`${await combined[0]} ${await combined[1]} ${await filled[0]}`);
  print(`${await copied[0]} ${await popped} ${await shifted} ${await removed[0]}`);
}
export async function main(): Promise<void> { await use(); }
"#;
    let files = [SourceFile::new("temporary-inputs.ts", source)];
    let hir = check_program(&files).expect("checked inputs");
    let lir = lower_module(&hir).expect("verified inputs");
    let expected = b"11 12 13\n14 15 17\n19 20 21 22\n";
    assert_eq!(interpret(&lir).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("C AOT"), expected);
    let mut session = ReloadSession::new(&files).expect("session");
    session.call_main().expect("root starts");
    while session.async_pending() != 0 {
        session.async_step().expect("checkpoint");
    }
    assert!(
        session.async_tasks().is_empty(),
        "{:?}",
        session.async_tasks()
    );
}

#[test]
fn counted_array_corpus_fits_one_reload_reservation() {
    let files = [SourceFile::new(
        "a338.ts",
        include_str!("../../corpus/accept/a338-counted-array-holders.ts"),
    )];
    let mut session = ReloadSession::new(&files).expect("counted corpus session");
    session.call_main().expect("root starts");
    while session.async_pending() != 0 {
        session.async_step().expect("checkpoint");
    }
    assert!(
        session.async_tasks().is_empty(),
        "{:?}",
        session.async_tasks()
    );
}

#[test]
fn conditional_holders_copy_borrowed_branches_and_transfer_fresh_branches() {
    let source = r#"
async function work():Promise<i32> {return 31;}
async function use(flag:boolean):Promise<void> {
  const h=work(); await h;
  const a:Promise<i32>[]=[h];
  const alias=flag?a:a;
  const mixed=flag?a:[h];
  const fresh=flag?[h]:[h];
  const nested=flag?[a]:[mixed];
  print(`${await alias[0]} ${await mixed[0]} ${await fresh[0]} ${await nested[0][0]}`);
  alias.pop();
}
export async function main():Promise<void> {await use(true);await use(false);}
"#;
    let files = [SourceFile::new("conditional-holders.ts", source)];
    let hir = check_program(&files).expect("checked branches");
    let lir = lower_module(&hir).expect("verified branches");
    let expected = b"31 31 31 31\n31 31 31 31\n";
    assert_eq!(interpret(&lir).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("C AOT"), expected);
    let mut session = ReloadSession::new(&files).expect("session");
    session.call_main().expect("root");
    while session.async_pending() != 0 {
        session.async_step().expect("checkpoint");
    }
    assert!(session.async_tasks().is_empty());
}

#[test]
fn temporary_reads_stores_and_loop_sources_release_every_owner() {
    let source = r#"
async function work():Promise<i32> {return 42;}
let stored:Promise<i32>[]=[];
function* gen(h:Promise<i32>):Generator<Promise<i32>> {
  stored.push(h); stored.pop(); yield h;
}
async function use(flag:boolean):Promise<void> {
  const h=work(); await h;
  const read=[h][0];
  const nested=[[h]][0][0];
  const deeper=[[[[h]]]][0][0][0][0]; await deeper;
  const assigned=([h][0]=h);
  const length=[h].length;
  [h][0]; [h].length; [h][0]=h;
  for (const job of [h,h]) { await job; if(flag) { break; } }
  try {
    for (const jobs of [[h]]) { await jobs[0]; throw new Error("exit"); }
  } catch {}
  const iterator=gen(h);
  const yielded=iterator.next().value;
  iterator.next();
  print(`${length} ${await read} ${await nested} ${await assigned} ${await yielded}`);
}
export async function main():Promise<void> {await use(true); await use(false);}
"#;
    let files = [SourceFile::new("temporary-reads.ts", source)];
    let hir = check_program(&files).expect("checked temporary reads");
    let lir = lower_module(&hir).expect("verified temporary reads");
    let expected = b"1 42 42 42 42\n1 42 42 42 42\n";
    assert_eq!(interpret(&lir).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("C AOT"), expected);
    let mut session = ReloadSession::new(&files).expect("session");
    session.call_main().expect("root");
    while session.async_pending() != 0 {
        session.async_step().expect("checkpoint");
    }
    assert!(session.async_tasks().is_empty());
}
