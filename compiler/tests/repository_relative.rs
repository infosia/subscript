//! Repository-relative names use the same spelling on every host (§100.3).

#[test]
fn repository_relative_uses_slashes_for_host_paths() {
    let root = std::env::temp_dir().join("subscript-path-spelling");
    let relative = ["docs", "tutorial-c-cpp.md"].join(std::path::MAIN_SEPARATOR_STR);
    let path = root.join(relative);
    assert_eq!(
        subscript_compiler::repository_relative(&root, &path).as_deref(),
        Some("docs/tutorial-c-cpp.md")
    );
    assert!(
        subscript_compiler::repository_relative(&root, &root.with_file_name("outside")).is_none()
    );
}
