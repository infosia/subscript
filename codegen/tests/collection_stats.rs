//! The statistics of the collections in the two native tiers
//! (`compiler.md` §353 acceptance 5).
//!
//! The ship tier reads the record in a post-run host hook through the C
//! functions. The dev JIT reads it through `ReloadSession`. Cost: one
//! ship-tier C build and one dev JIT compile.

use std::path::PathBuf;

use subscript_codegen::{run_c_aot_configured, NativeLibrary, ReloadSession, RunConfig};
use subscript_compiler::SourceFile;
use subscript_runtime::context::CollectionMode;

/// The minimum headroom: the loop requests about 40 times this.
const MIN_HEADROOM: usize = 64 << 10;

/// A script that allocates in a loop and keeps nothing.
const SOURCE: &str = r#"
class Node {
  next: Node | null;
  v: i32;
  constructor(v: i32, next: Node | null) { this.v = v; this.next = next; }
}
export function main(): void {
  let total: i32 = 0;
  for (let i: i32 = 0; i < 40000; i += 1) {
    const n: Node = new Node(i % 7, null);
    total += n.v;
  }
  print(`${total}`);
}
export function collectNow(): void {
  Context.collect();
}
"#;

/// The value of the line `name=<value>` in `text`.
fn field(text: &str, name: &str) -> u64 {
    text.lines()
        .find_map(|line| line.strip_prefix(&format!("{name}=")))
        .unwrap_or_else(|| panic!("the hook printed {name}: {text}"))
        .parse()
        .expect("a number")
}

/// The ship tier: a pre-init hook sets mode 1 with a small headroom. The
/// post-run hook reads the record, collects one time, and reads the record
/// and the live bytes again.
#[test]
fn the_ship_tier_reports_the_automatic_collections_of_a_run() {
    let scratch = std::env::temp_dir().join(format!(
        "subscript-collection-stats-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&scratch).expect("create scratch directory");
    let hooks = scratch.join("stats_hooks.c");
    std::fs::write(
        &hooks,
        format!(
            "#include <stdio.h>\n\
             #include \"subscript_runtime.h\"\n\
             void subscript_test_stats_setup(subscript_rt_context* ctx) {{\n\
               if (subscript_rt_ctx_set_collection(ctx, 1, {MIN_HEADROOM}u) != 0) {{ printf(\"setup-refused\\n\"); }}\n\
             }}\n\
             void subscript_test_stats_report(subscript_rt_context* ctx) {{\n\
               subscript_rt_collection_stats stats;\n\
               subscript_rt_ctx_collection_stats(ctx, &stats);\n\
               printf(\"automatic=%llu\\n\", (unsigned long long)stats.automatic_collections);\n\
               printf(\"explicit=%llu\\n\", (unsigned long long)stats.explicit_collections);\n\
               printf(\"max-ns=%llu\\n\", (unsigned long long)stats.max_ns);\n\
               subscript_rt_ctx_collect(ctx);\n\
               subscript_rt_ctx_collection_stats(ctx, &stats);\n\
               printf(\"explicit-after=%llu\\n\", (unsigned long long)stats.explicit_collections);\n\
               printf(\"live-after=%llu\\n\", (unsigned long long)stats.last_live_bytes_after);\n\
               printf(\"live-bytes=%llu\\n\", (unsigned long long)subscript_rt_ctx_live_bytes(ctx));\n\
               fflush(stdout);\n\
             }}\n"
        ),
    )
    .expect("write hook source");
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    // SAFETY: the library defines only the two host hooks; no script calls it.
    let library = [unsafe {
        NativeLibrary::new(vec![repo.join("runtime/include")], vec![hooks], Vec::new())
    }];
    let mut config = RunConfig::default().with_native_libraries(&library);
    config.pre_init_hook = Some("subscript_test_stats_setup");
    config.post_run_hook = Some("subscript_test_stats_report");
    let files = [SourceFile::new("stats.ts", SOURCE)];
    let output = run_c_aot_configured(&files, config).expect("ship run");
    let _ = std::fs::remove_dir_all(&scratch);
    let text = String::from_utf8(output.stdout).expect("utf-8 output");
    assert!(text.starts_with("119995\n"), "{text}");
    assert!(field(&text, "automatic") > 0, "{text}");
    assert_eq!(field(&text, "explicit"), 0, "{text}");
    assert!(field(&text, "max-ns") > 0, "{text}");
    assert_eq!(field(&text, "explicit-after"), 1, "{text}");
    assert_eq!(
        field(&text, "live-after"),
        field(&text, "live-bytes"),
        "{text}"
    );
}

/// The dev JIT through `ReloadSession`, with the same mode and headroom.
#[test]
fn the_dev_tier_session_reports_the_automatic_collections_of_a_run() {
    let files = [SourceFile::new("stats.ts", SOURCE)];
    let mut config = RunConfig::default();
    config.collection = Some((CollectionMode::Automatic, MIN_HEADROOM));
    let mut session = ReloadSession::new_configured(&files, config).expect("session");
    session.call_main().expect("main");
    assert_eq!(session.take_output(), b"119995\n");
    let stats = session.collection_stats();
    assert!(stats.automatic_collections > 0, "{stats:?}");
    assert_eq!(stats.explicit_collections, 0, "{stats:?}");
    assert!(stats.max_ns > 0, "{stats:?}");
    session.call_export("collectNow").expect("collect");
    let stats = session.collection_stats();
    assert_eq!(stats.explicit_collections, 1, "{stats:?}");
    assert_eq!(stats.last_live_bytes_after, session.live_bytes() as u64);
}
