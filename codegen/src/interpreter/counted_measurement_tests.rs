//! Each measurement group runs once per tier and checks counts before teardown.

use super::*;
use crate::{
    emit_c, host_c_compiler, runtime_staticlib_path, runtime_system_libraries, ReloadSession,
};
use subscript_compiler::{check_program, SourceFile};

fn zero_in_three_tiers(label: &str, source: &str) {
    outcome_in_three_tiers(label, source, 0, false);
}

fn outcome_in_three_tiers(label: &str, source: &str, expected: usize, traps: bool) {
    let files = [SourceFile::new("counted-measurement.ts", source)];
    let hir = check_program(&files).unwrap_or_else(|errors| panic!("{label}: {errors:?}"));
    let module = crate::lir::lower_module(&hir).expect("verified measurement");
    let mut interpreter = Interpreter::new(&module).expect("interpreter");
    if let Some(initializer) = module.initializer {
        interpreter
            .call_function(initializer, Vec::new())
            .expect("initializer");
    }
    let Value::Coroutine(root) = interpreter
        .call_function(module.entry.expect("entry"), Vec::new())
        .expect("root")
    else {
        panic!("async root");
    };
    let mut error = interpreter.async_kick(&root).err();
    while error.is_none() && interpreter.async_pending() != 0 {
        error = interpreter.async_step().err();
    }
    if traps {
        assert!(
            matches!(
                error.map(InterpretError::settled),
                Some(InterpretError::Trap {
                    runtime_kind: Some(RuntimeTrapKind::UncaughtException),
                    ..
                })
            ),
            "{label}: interpreter trap"
        );
    } else {
        assert!(error.is_none(), "{label}: {error:?}\n{source}");
    }
    assert_eq!(
        interpreter.async_tasks().len(),
        expected,
        "{label}: interpreter"
    );
    let mut jit = ReloadSession::new(&files).expect("reload session");
    let mut error = jit.call_main().err();
    while error.is_none() && jit.async_pending() != 0 {
        error = jit.async_step().err();
    }
    if traps {
        assert!(
            matches!(error, Some(crate::RunError::Trap(ref record)) if record.rule == RuntimeTrapKind::UncaughtException),
            "{label}: {error:?}"
        );
    } else {
        assert!(error.is_none(), "{label}: {error:?}");
    }
    assert_eq!(jit.async_tasks().len(), expected, "{label}: JIT");
    let emitted = emit_c(&hir).expect("C emission");
    let dir = std::env::temp_dir().join(format!(
        "subscript-counted-measurement-{}-{label}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("fixture directory");
    std::fs::write(dir.join("program.c"), &emitted.source).expect("C source");
    std::fs::write(dir.join("program.h"), &emitted.host_header).expect("C header");
    std::fs::write(
        dir.join("program.alloc.h"),
        &emitted.allocation_metadata_header,
    )
    .expect("position header");
    let initialize = if module.initializer.is_some() {
        "subscript_init(ctx);"
    } else {
        ""
    };
    let host = crate::host_entry(r#"
static void visit(void* data, const subscript_rt_async_task_info* task) {
 (void)task; ++*(uint64_t*)data;
}
int main(void) {
 subscript_rt_context* ctx=subscript_rt_ctx_new();
 subscript_rt_ctx_enter_script(ctx);INITIALIZE;subscript_export_main(ctx);subscript_rt_ctx_exit_script(ctx);
 while (subscript_rt_ctx_async_pending(ctx) && !subscript_rt_ctx_trap_kind(ctx)) subscript_rt_ctx_async_step(ctx);
 uint64_t count=0;subscript_rt_ctx_visit_async_tasks(ctx,visit,&count);
 int result=(count != EXPECTED || subscript_rt_ctx_trap_kind(ctx) != TRAP);
 subscript_rt_ctx_release(ctx);return result;
}
"#, &emitted.host_header).expect("host entry").replace("INITIALIZE", initialize)
        .replace("EXPECTED", &expected.to_string()).replace("TRAP", if traps { "29" } else { "0" });
    std::fs::write(dir.join("host.c"), host).expect("C host");
    let compiler = host_c_compiler().expect("host compiler");
    let mut command = compiler.command();
    crate::add_c11_optimized_flags(&mut command, compiler.style());
    crate::add_object_directory(&mut command, &dir, compiler.style());
    command
        .arg(dir.join("program.c"))
        .arg(dir.join("host.c"))
        .arg(runtime_staticlib_path().expect("runtime"))
        .args(runtime_system_libraries(compiler.style()));
    let exe = dir.join(format!("measurement{}", std::env::consts::EXE_SUFFIX));
    crate::add_executable_output(&mut command, &exe, compiler.style());
    let built = command.output().expect("C build");
    assert!(
        built.status.success(),
        "{label}: {}",
        crate::tool_output_report(&built)
    );
    let ran = std::process::Command::new(&exe).output().expect("C run");
    assert!(
        ran.status.success(),
        "{label}: C retained a task or trapped: {}",
        crate::tool_output_report(&ran)
    );
    std::fs::remove_dir_all(dir).expect("remove fixture");
}

#[test]
fn array_alias_measurement_rows_and_zero_controls_release_every_task() {
    let operations = [
        "",
        "b.pop();",
        "b.shift();",
        "b.splice(0,1);",
        "b.push(h);",
        "b[0]=h;",
        "b[0]=b[0];",
        "b.unshift(h);",
        "b.fill(b[0]);",
        "b.reverse();",
        "b.copyWithin(0,1);",
        "const removed=b.shift(); await removed;",
        "const removed=b.splice(0,1); await removed[0];",
    ];
    for alias in [
        "single",
        "const",
        "let",
        "conditional",
        "parameter",
        "async",
        "return",
        "global",
        "field",
        "nested",
    ] {
        let mut source = String::from(
            r#"
async function work(): Promise<void> { return; }
let stored: Promise<void>[] = [];
class Box { jobs: Promise<void>[]; constructor(jobs: Promise<void>[]) { this.jobs=jobs; } }
function identity(jobs: Promise<void>[]): Promise<void>[] { return jobs; }
"#,
        );
        let mut main = String::from("export async function main(): Promise<void> {\n");
        for (index, operation) in operations.iter().enumerate() {
            for control in [false, true] {
                let name = format!("probe{index}_{}", u8::from(control));
                let operation = if control { "" } else { operation };
                // The synchronous parameter returns both inputs to satisfy S013.
                let parameter = alias == "parameter" && !operation.contains("await");
                let asynchronous = alias == "async";
                let prelude = if parameter {
                    format!("function mutate{name}(b:Promise<void>[],h:Promise<void>):Promise<void>[] {{ const keep=b[0]; {operation} return [keep,h]; }}\n")
                } else if asynchronous {
                    format!("async function mutate{name}(b:Promise<void>[],h:Promise<void>):Promise<void> {{ await b[0]; await h; {operation} }}\n")
                } else {
                    String::new()
                };
                source.push_str(&prelude);
                source.push_str(&format!("async function {name}():Promise<void> {{\nconst h=work(); await h; const a:Promise<void>[]=[work(),h]; await a[0];\n"));
                let (setup, cleanup) = match alias {
                    "single" => ("", ""),
                    "const" => ("const b=a;", ""),
                    "let" => ("let b=a;", ""),
                    "conditional" => ("const b=true?a:a;", ""),
                    "return" => ("const b=identity(a);", ""),
                    "global" => ("stored=a; const b=stored;", "stored=[];"),
                    "field" => (
                        "const box=new Box(a); const b=box.jobs;",
                        "Context.free(box);",
                    ),
                    "nested" => ("const n:Promise<void>[][]=[a]; const b=n[0];", "n[0]=[];"),
                    _ => ("const b=a;", ""),
                };
                source.push_str(setup);
                source.push_str(if alias == "single" {
                    "await a[0];"
                } else {
                    "await b[0];"
                });
                if parameter {
                    source.push_str(&format!(
                        "const result=mutate{name}(b,h); await result[0]; await result[1];"
                    ));
                } else if asynchronous {
                    source.push_str(&format!("await mutate{name}(b,h);"));
                } else {
                    source.push_str(&if alias == "single" {
                        operation.replace("b.", "a.").replace("b[", "a[")
                    } else {
                        operation.to_string()
                    });
                }
                source.push_str(cleanup);
                source.push_str("}\n");
                main.push_str(&format!("await {name}();\n"));
            }
        }
        main.push_str("}\n");
        source.push_str(&main);
        zero_in_three_tiers(alias, &source);
    }
}

#[test]
fn inline_holders_and_completion_measurement_rows_release_every_task() {
    for shape in [
        "Promise<void>",
        "Promise<void>[]",
        "Promise<void>[][]",
        "FixedArray<Promise<void>,1>",
    ] {
        let payload = match shape {
            "Promise<void>" => "h",
            "Promise<void>[][]" => "[[h]]",
            _ => "[h]",
        };
        let inner = match shape {
            "Promise<void>" => "v",
            "Promise<void>[][]" => "v[0][0]",
            _ => "v[0]",
        };
        let mut source = format!("async function work():Promise<void> {{return;}}\nasync function make():Promise<{shape}> {{const h=work(); await h; return {payload};}}\nlet stored:Promise<void>[]=[];
function sync(h:Promise<void>):{shape} {{stored.push(h);stored.pop();return {payload};}}\n");
        let mut main = String::from("export async function main():Promise<void> {\n");
        for reads in 0..=2 {
            for control in [false, true] {
                let name = format!("probe{reads}_{}", u8::from(control));
                source.push_str(&format!("async function {name}():Promise<void> {{\n"));
                if control {
                    source.push_str("const h=work();await h;const outer=sync(h);\n");
                    source.push_str(&format!("{{const v=outer;if(false){{await {inner};}}}}\n"));
                } else {
                    source.push_str(
                        "const outer=make();if(false){await outer;}await Context.suspend();\n",
                    );
                }
                for _ in 0..reads {
                    source.push_str(if control {
                        "{const v=outer;"
                    } else {
                        "{const v=await outer;"
                    });
                    source.push_str(&format!("await {inner};}}\n"));
                }
                source.push_str("}\n");
                main.push_str(&format!("await {name}();\n"));
            }
        }
        source.push_str("async function discard():Promise<void>{await make();}\n");
        main.push_str("await discard();}\n");
        source.push_str(&main);
        let label = shape.replace(['<', '>', '[', ']', ','], "_");
        zero_in_three_tiers(&label, &source);
    }
}

#[test]
fn exact_holder_rows_and_their_zero_controls_release_every_task() {
    let bodies = [
        "const h=work();await h;const a:Promise<void>[][][]=[[[h]]];await a[0][0][0];",
        "const h=work();await h;const a:Promise<void>[][][][]=[[[[h]]]];await a[0][0][0][0];",
        "const n:Promise<void>[][]=[[work()]];for(const a of n){await a[0];}",
        "const h=work();await h;const a:FixedArray<Promise<void>,1>=[h];await a[0];",
        "const a:Promise<void>[]=[work()];await a[0];const b=identity(a);await b[0];",
        "const a:Promise<void>[]=[work()];await a[0];const o=new Methods();const b=o.identity(a);await b[0];Context.free(o);",
        "const a:Promise<void>[]=[work()];await a[0];const o=new Methods();await o.consume(a);Context.free(o);",
        "const a:Promise<void>[]=[work()];await a[0];const o=new Holder(a);const b=o.jobs;await b[0];Context.free(o);",
        "const h=work();await h;const o=new Nested([[h]]);await o.jobs[0][0];Context.free(o);",
        "const h=work();await h;Static.jobs=[h];await Static.jobs[0];Static.jobs=[];",
        "const h=work();await h;const a:Promise<void>[]=[h];const copy=():Promise<void>[]=>a;const b=copy();await b[0];",
        "const h=work();await h;const a:Promise<void>[]=[h];const it=gen(a);it.next();it.next();",
        "const h=work();await h;const a:Promise<void>[]=[work(),h];const b=a;await a[0];b.copyWithin(0,1);await h;",
        "const h=work();await h;const a:Promise<void>[]=[work()];const b=a;await a[0];b.push(h);await h;",
        "const a:Promise<void>[]=[work()];await a[0];a.push(work());await a[a.length-1];",
        "const a:Promise<void>[]=[work()];await a[0];a[0]=work();await a[0];",
        "const a:Promise<void>[]=[work()];await a[0];a.unshift(work());await a[0];",
        "const a:Promise<void>[]=[work()];await a[0];const b=[...a];await b[0];const c=a.slice();await c[0];const d=a.concat(b);await d[0];await d[1];",
    ];
    let mut source = String::from(
        r#"
async function work():Promise<void>{return;}
function identity(a:Promise<void>[]):Promise<void>[]{return a;}
class Methods {
 identity(a:Promise<void>[]):Promise<void>[]{return a;}
 async consume(a:Promise<void>[]):Promise<void>{await a[0];}
}
class Holder {
 a:Promise<void>[];
 constructor(a:Promise<void>[]){this.a=a;}
 get jobs():Promise<void>[]{return this.a;}
}
class Nested { jobs:Promise<void>[][];constructor(a:Promise<void>[][]){this.jobs=a;} }
class Static { static jobs:Promise<void>[]=[]; }
let transfer:Promise<void>[]=[];
function consume(h:Promise<void>):void{transfer.push(h);}
function* gen(a:Promise<void>[]):Generator<i32>{const local=a;consume(local[0]);transfer.pop();yield 1;}
"#,
    );
    let mut main = String::from("export async function main():Promise<void>{");
    for (index, body) in bodies.iter().enumerate() {
        source.push_str(&format!(
            "async function row{index}(execute:boolean):Promise<void>{{if(execute){{{body}}}}}\n"
        ));
        main.push_str(&format!("await row{index}(false);await row{index}(true);"));
    }
    main.push('}');
    source.push_str(&main);
    zero_in_three_tiers("exact-holders", &source);
}

#[test]
fn failed_completion_rows_trap_and_observed_controls_release_every_task() {
    for shape in ["handle", "array"] {
        let (ty, value, inner) = if shape == "handle" {
            ("Promise<void>", "work()", "v")
        } else {
            ("Promise<void>[]", "[work()]", "v[0]")
        };
        for asynchronous in [true, false] {
            let prefix = if asynchronous { "async " } else { "" };
            let make_ty = if asynchronous {
                format!("Promise<{ty}>")
            } else {
                ty.into()
            };
            let read = if asynchronous {
                "await outer"
            } else {
                "make()"
            };
            for observed in [false, true] {
                let body = if observed {
                    format!("try{{await {inner};}}catch(e){{}}")
                } else {
                    format!("if(false){{await {inner};}}")
                };
                let hold = if asynchronous {
                    "const outer=make();"
                } else {
                    ""
                };
                let source = format!("async function work():Promise<void>{{throw new Error(\"inner\");}}\n{prefix}function make():{make_ty}{{return {value};}}\nasync function use():Promise<void>{{{hold}const v={read};{body}}}\nexport async function main():Promise<void>{{await use();}}");
                outcome_in_three_tiers(
                    &format!("failure-{shape}-{asynchronous}-{observed}"),
                    &source,
                    if observed { 0 } else { 2 },
                    !observed,
                );
            }
        }
    }
}

#[test]
fn fresh_mutation_rows_and_same_element_parameter_control_release_every_task() {
    for alias in ["const", "async", "return", "global", "field", "nested"] {
        let mut source = String::from(
            r#"
async function work():Promise<void>{return;}
let stored:Promise<void>[]=[];
class Box {jobs:Promise<void>[];constructor(a:Promise<void>[]){this.jobs=a;}}
function identity(a:Promise<void>[]):Promise<void>[]{return a;}
function same(a:Promise<void>[]):Promise<void>{const keep=a[0];a[0]=keep;return keep;}
"#,
        );
        let mut main = String::from("export async function main():Promise<void>{");
        for (index, operation) in [
            "b.push(work());await b[b.length-1];",
            "b.unshift(work());await b[0];",
            "b[0]=work();await b[0];",
        ]
        .iter()
        .enumerate()
        {
            // Item 2 records S013 for fresh index stores through these positions.
            if index == 2 && matches!(alias, "global" | "field" | "nested") {
                continue;
            }
            for control in [false, true] {
                let name = format!("row{index}_{}", u8::from(control));
                let operation = if control { "" } else { operation };
                if alias == "async" {
                    source.push_str(&format!("async function mutate{name}(b:Promise<void>[]):Promise<void>{{await b[0];{operation}}}\n"));
                }
                source.push_str(&format!("async function {name}():Promise<void>{{const a:Promise<void>[]=[work()];await a[0];"));
                let (setup, cleanup) = match alias {
                    "global" => ("stored=a;const b=stored;", "stored=[];"),
                    "field" => ("const o=new Box(a);const b=o.jobs;", "Context.free(o);"),
                    "nested" => ("const n:Promise<void>[][]=[a];const b=n[0];", "n[0]=[];"),
                    "return" => ("const b=identity(a);", ""),
                    _ => ("const b=a;", ""),
                };
                source.push_str(setup);
                source.push_str("await b[0];");
                source.push_str(if alias == "async" { "" } else { operation });
                if alias == "async" {
                    source.push_str(&format!("await mutate{name}(b);"));
                }
                source.push_str(cleanup);
                source.push_str("}\n");
                main.push_str(&format!("await {name}();"));
            }
        }
        source.push_str("async function parameter():Promise<void>{const a:Promise<void>[]=[work()];await a[0];const h=same(a);await h;}\n");
        main.push_str("await parameter();}");
        source.push_str(&main);
        zero_in_three_tiers(&format!("fresh-{alias}"), &source);
    }
}

#[test]
fn measured_fresh_index_positions_still_have_no_accepted_form() {
    for (label, setup, receiver, cleanup) in [
        ("global", "stored=a;", "stored", "stored=[];"),
        ("field", "const o=new Box(a);", "o.jobs", "Context.free(o);"),
        (
            "nested",
            "const n:Promise<void>[][]=[a];",
            "n[0]",
            "n[0]=[];",
        ),
    ] {
        let source = |value| {
            format!("async function work():Promise<void>{{return;}}\nlet stored:Promise<void>[]=[];\nclass Box{{jobs:Promise<void>[];constructor(a:Promise<void>[]){{this.jobs=a;}}}}\nasync function use():Promise<void>{{const h=work();await h;const a:Promise<void>[]=[work()];await a[0];{setup}await {receiver}[0];{receiver}[0]={value};await {receiver}[0];{cleanup}}}\nexport async function main():Promise<void>{{await use();}}")
        };
        let control = source("h");
        check_program(&[SourceFile::new("fresh-index.ts", control.clone())])
            .expect("observed-local control");
        let errors = check_program(&[SourceFile::new("fresh-index.ts", source("work()"))])
            .expect_err("measured S013 form");
        assert!(
            errors.iter().any(|error| error.code.to_string() == "S013"),
            "{label}: {errors:?}"
        );
        zero_in_three_tiers(&format!("observed-index-{label}"), &control);
    }
}

#[test]
fn deferred_reference_holders_and_dropped_generators_keep_the_measured_count() {
    let prelude = r#"
async function work():Promise<void>{return;}
class Box{jobs:Promise<void>[];constructor(a:Promise<void>[]){this.jobs=a;}}
let transfer:Promise<void>[]=[];
function consume(h:Promise<void>):void{transfer.push(h);}
function* gen(a:Promise<void>[]):Generator<i32>{const local=a;consume(local[0]);transfer.pop();yield 1;}
"#;
    for (label, body, control) in [
        (
            "field-scope",
            "const o=new Box([h]);await o.jobs[0];",
            "const o=new Box([h]);await o.jobs[0];Context.free(o);",
        ),
        (
            "map-array",
            "const m=new Map<i32,Promise<void>[]>();m.set(1,[h]);",
            "const m=new Map<i32,Promise<void>[]>();if(false){m.set(1,[h]);}",
        ),
        (
            "dropped-generator-array",
            "const it=gen([h]);it.next();",
            "const it=gen([h]);it.next();it.next();",
        ),
    ] {
        for (body, expected) in [(body, 1), (control, 0)] {
            let source = format!("{prelude}\nasync function use():Promise<void>{{const h=work();await h;{body}}}\nexport async function main():Promise<void>{{await use();}}");
            outcome_in_three_tiers(&format!("{label}-{expected}"), &source, expected, false);
        }
    }
}

#[test]
fn temporary_builtin_receivers_and_controls_release_every_task() {
    for method in ["pop", "push"] {
        for temporary in [false, true] {
            let setup = if temporary {
                ""
            } else {
                "const batch = [h, h];"
            };
            let receiver = if temporary { "[h, h]" } else { "batch" };
            let operation = if method == "pop" {
                format!("const last = {receiver}.pop(); await last;")
            } else {
                format!("{receiver}.push(h);")
            };
            let source = format!(
                r#"
async function work(): Promise<i32> {{ return 7; }}
async function use(): Promise<void> {{
 const h = work(); await h;
 {setup} {operation}
}}
export async function main(): Promise<void> {{ await use(); }}
"#
            );
            zero_in_three_tiers(&format!("builtin-{method}-{temporary}"), &source);
        }
    }
}

#[test]
fn counted_loop_subject_holds_end_on_every_exit() {
    for exit in ["", "break;", "return;", r#"throw new Error("exit");"#] {
        let source = format!(
            r#"
let queue: Promise<i32>[] = [];
async function work(): Promise<i32> {{ return 7; }}
function reset(): void {{ queue = []; }}
async function use(): Promise<void> {{
 const h = work(); await h; queue = [h,h];
 for (const job of queue) {{ reset(); await job; {exit} }}
}}
export async function main(): Promise<void> {{ try {{ await use(); }} catch {{}} }}
"#
        );
        zero_in_three_tiers(&format!("loop-exit-{}", exit.len()), &source);
    }
}

#[test]
fn counted_inputs_survive_user_code_in_later_operands() {
    let operations = [
        "const kept = queue.fill(resetHandle(h)); await kept[0];",
        "const kept = [queue, resetArray()]; await kept[0][0];",
        "const kept = [...queue, ...resetArray()]; await kept[0];",
        "const kept = queue[resetIndex()]; await kept;",
        "queue[0] = (resetIndex() === 0 ? h : h); await h;",
        "try { queue.fill(throwHandle()); await queue[0]; } catch {} queue = [];",
    ];
    for (index, operation) in operations.iter().enumerate() {
        let source = format!(
            r#"
let queue: Promise<i32>[] = [];
async function work(): Promise<i32> {{ return 7; }}
function resetHandle(h: Promise<i32>): Promise<i32> {{ queue = []; return h; }}
function resetArray(): Promise<i32>[] {{ queue = []; return []; }}
function resetIndex(): i32 {{ queue = []; return 0; }}
function throwHandle(): Promise<i32> {{ throw new Error("exit"); }}
async function use(): Promise<void> {{
 const h = work(); await h; queue = [h];
 {operation}
 queue = [];
}}
export async function main(): Promise<void> {{ await use(); }}
"#
        );
        zero_in_three_tiers(&format!("input-hold-{index}"), &source);
    }
}
