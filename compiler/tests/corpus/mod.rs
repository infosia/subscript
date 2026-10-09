//! Shared corpus-test helpers.

#[path = "../../../codegen/tests/corpus/mod.rs"]
#[allow(dead_code)]
mod directory_corpus;

#[allow(unused_imports)]
pub(crate) use directory_corpus::check_program;
pub(crate) use directory_corpus::interop;

/// All directory programs, in stable order.
#[allow(dead_code)]
pub fn directories(accept: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut paths = std::fs::read_dir(accept)
        .expect("accept directory")
        .map(|entry| entry.expect("corpus entry").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

/// Loads a directory program and the ambient mirrors that it uses.
#[allow(dead_code)]
pub fn directory_sources(directory: &std::path::Path) -> Vec<subscript_compiler::SourceFile> {
    directory_corpus::entry_sources(
        directory.parent().expect("corpus root"),
        directory
            .file_name()
            .expect("entry name")
            .to_str()
            .expect("entry UTF-8"),
    )
}
