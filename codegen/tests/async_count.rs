//! Generated async count sequences (compiler.md §116.2 rule 6).

use subscript_codegen::emit_c;
use subscript_compiler::{check_program, SourceFile};

#[test]
fn async_counts_inline_and_check_pending_only_after_runtime_release() {
    let hir = check_program(&[SourceFile::new(
        "async-count.ts",
        r#"
async function child(): Promise<i32> { return 42; }
export async function main(): Promise<void> {
    const held: Promise<i32> = child();
    const copy: Promise<i32> = held;
    await copy;
    await held;
}
"#,
    )])
    .expect("async count fixture checks");
    let source = emit_c(&hir).expect("async count fixture emits C").source;
    let retain = source
        .lines()
        .find(|line| line.contains("!= UINT32_MAX"))
        .expect("a copied handle has an inline retain");
    let slot = retain
        .trim()
        .strip_prefix("if (")
        .and_then(|s| s.split_once(" != UINT32_MAX"))
        .expect("retain compares the count")
        .0;
    let handle = slot
        .strip_prefix("*(uint32_t*)((uint8_t*)")
        .and_then(|s| s.strip_suffix(" + 4)"))
        .expect("the count is a u32 at offset 4");
    let expected = format!(
        "    if ({handle} != NULL) {{\n        if ({slot} != UINT32_MAX) ++({slot});\n    }}\n"
    );
    assert!(source.contains(&expected), "{source}");

    let release = source
        .lines()
        .find(|line| {
            line.trim_start()
                .starts_with("subscript_rt_async_release(ctx,")
                && !line.contains("_child")
        })
        .expect("a local handle has a runtime release fallback");
    let handle = release
        .trim()
        .strip_prefix("subscript_rt_async_release(ctx, ")
        .and_then(|s| s.split_once(','))
        .expect("runtime release carries its handle")
        .0;
    let slot = format!("*(uint32_t*)((uint8_t*){handle} + 4)");
    let expected = format!(
        "    if ({handle} != NULL && {slot} > 1u) {{\n        --({slot});\n    }} else {{\n{release}\n    if (*(const uint32_t*)ctx != 0u) goto unwind;\n    }}\n"
    );
    assert!(source.contains(&expected), "{source}");
}
