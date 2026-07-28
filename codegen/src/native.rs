//! Caller-supplied native-library inputs shared by the development and
//! ship runners.

use std::path::{Path, PathBuf};

use cranelift_jit::JITBuilder;

/// One native library available to a script run.
///
/// Include directories and C sources are passed to the ship tier's C
/// compiler. Symbol names and addresses are registered with the
/// development-tier JIT. Keeping both representations in one value makes
/// the two runners consume the same host boundary.
#[derive(Debug, Clone)]
#[non_exhaustive]
#[must_use = "a native library has no effect until it is passed to a runner"]
pub struct NativeLibrary {
    include_directories: Vec<PathBuf>,
    c_sources: Vec<PathBuf>,
    symbols: Vec<(String, *const u8)>,
}

impl NativeLibrary {
    /// Constructs one caller-supplied native library.
    ///
    /// # Safety
    ///
    /// Every symbol address must remain valid for every run that receives
    /// this value, and each address must implement the C signature declared
    /// for its name by the ingested mirror.
    pub unsafe fn new(
        include_directories: Vec<PathBuf>,
        c_sources: Vec<PathBuf>,
        symbols: Vec<(String, *const u8)>,
    ) -> Self {
        Self {
            include_directories,
            c_sources,
            symbols,
        }
    }

    pub(crate) fn include_directories(&self) -> impl Iterator<Item = &Path> {
        self.include_directories.iter().map(PathBuf::as_path)
    }

    pub(crate) fn c_sources(&self) -> impl Iterator<Item = &Path> {
        self.c_sources.iter().map(PathBuf::as_path)
    }

    pub(crate) fn symbols(&self) -> impl Iterator<Item = (&str, *const u8)> + '_ {
        self.symbols
            .iter()
            .map(|(name, address)| (name.as_str(), *address))
    }
}

pub(crate) fn register_symbols(builder: &mut JITBuilder, libraries: &[NativeLibrary]) {
    for library in libraries {
        for (name, address) in library.symbols() {
            builder.symbol(name, address);
        }
    }
}

pub(crate) fn missing_symbol<'a>(
    required: &'a [String],
    libraries: &[NativeLibrary],
) -> Option<&'a str> {
    required.iter().find_map(|required_name| {
        let supplied = libraries.iter().any(|library| {
            library
                .symbols()
                .any(|(name, _)| name == required_name.as_str())
        });
        (!supplied).then_some(required_name.as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn probe() {}

    #[test]
    fn constructor_preserves_all_three_link_surfaces() {
        // SAFETY: `probe` has static lifetime and this test does not declare
        // or call a foreign signature through its address.
        let library = unsafe {
            NativeLibrary::new(
                vec![PathBuf::from("include")],
                vec![PathBuf::from("source.c")],
                vec![("probe".to_string(), probe as *const u8)],
            )
        };
        assert_eq!(
            library.include_directories().collect::<Vec<_>>(),
            [Path::new("include")]
        );
        assert_eq!(
            library.c_sources().collect::<Vec<_>>(),
            [Path::new("source.c")]
        );
        assert_eq!(
            library.symbols().map(|(name, _)| name).collect::<Vec<_>>(),
            ["probe"]
        );
    }
}
