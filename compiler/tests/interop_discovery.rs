//! Checks that a new generated mirror needs no harness update.

#[path = "corpus/interop.rs"]
mod interop;

#[test]
fn new_mirrors_and_transitive_ambient_dependencies_are_discovered() {
    let directory = std::env::temp_dir().join(format!(
        "subscript-interop-discovery-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).expect("create mirror fixture");
    std::fs::write(
        directory.join("owner.generated.d.ts"),
        "declare interface NewHandle {}\n",
    )
    .expect("write owner mirror");
    std::fs::write(
        directory.join("future.generated.d.ts"),
        "declare function futureOperation(handle: NewHandle): FutureMode;\n",
    )
    .expect("write new mirror");
    std::fs::write(
        directory.join("future-aliases.d.ts"),
        "type FutureMode = i32;\n",
    )
    .expect("write ambient companion");
    std::fs::write(
        directory.join("unused.generated.d.ts"),
        "declare function unusedOperation(): void;\n",
    )
    .expect("write unused mirror");
    std::fs::write(
        directory.join("ignored.ts"),
        "declare function futureOperation(): void;\n",
    )
    .expect("write non-mirror");
    let names = interop::mirrors_for_in(&directory, "futureOperation(handle)", |name, _| name);
    let unreferenced =
        interop::mirrors_for_in(&directory, "// futureOperation\nprint(1)", |name, _| name);
    std::fs::remove_dir_all(&directory).expect("remove mirror fixture");
    assert_eq!(
        names,
        [
            "future-aliases.d.ts",
            "future.generated.d.ts",
            "owner.generated.d.ts"
        ]
    );
    assert!(unreferenced.is_empty());
}
