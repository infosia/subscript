//! compiler.md §265 rules 1–4: source paths preserve quoted includes and mirror bytes.
//! Cost: four CLI processes and four libclang parses; no C compiler or tsc.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct TestDir(PathBuf);

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn bind(directory: &Path, header: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_subscript"))
        .current_dir(directory)
        .arg("bind")
        .arg(header)
        .output()
        .expect("run bind")
}

#[test]
fn header_directory_preserves_includes_and_mirror_bytes() {
    let directory = TestDir(
        std::env::temp_dir().join(format!("subscript-header-directory-{}", std::process::id())),
    );
    let headers = directory.0.join("headers");
    let caller = directory.0.join("caller");
    std::fs::create_dir_all(&headers).unwrap();
    std::fs::create_dir_all(&caller).unwrap();
    std::fs::write(
        headers.join("types.h"),
        "typedef int SiblingInt;\nvoid sibling_only(void);\n",
    )
    .unwrap();
    std::fs::write(
        headers.join("api.h"),
        "#include \"types.h\"\n/* @subscript-external SiblingInt */\nSiblingInt api(SiblingInt value);\n",
    ).unwrap();
    let baseline = bind(&headers, Path::new("api.h"));
    let error = String::from_utf8_lossy(&baseline.stderr);
    assert!(baseline.status.success(), "{error}");
    let mirror = String::from_utf8_lossy(&baseline.stdout);
    assert!(mirror.contains("api"), "{mirror}");
    assert!(!mirror.contains("type SiblingInt"), "{mirror}");
    assert!(!mirror.contains("sibling_only"), "{mirror}");
    for header in [PathBuf::from("../headers/api.h"), headers.join("api.h")] {
        let result = bind(&caller, &header);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(result.stdout, baseline.stdout);
    }
    std::fs::write(headers.join("missing.h"), "#include \"absent-s265.h\"\n").unwrap();
    let missing = bind(&caller, &headers.join("missing.h"));
    assert_eq!(missing.status.code(), Some(1));
    let error = String::from_utf8_lossy(&missing.stderr);
    assert!(
        error.contains("libclang:") && error.contains("'absent-s265.h' file not found"),
        "{error}"
    );
    assert!(missing.stdout.is_empty());
}
