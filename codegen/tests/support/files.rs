//! Scratch-file providers for the standard-module corpus and reload tests.
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use subscript_codegen::{NativeLibrary, RunConfig};
use subscript_runtime::context::{CompletionEndpoint, CompletionStatus};
use subscript_runtime::{ffi, Context};

struct State {
    root: PathBuf,
    pending: Option<(PathBuf, u32, CompletionEndpoint)>,
}

pub struct Fixture {
    state: Box<State>,
}
impl Fixture {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "subscript-files-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).expect("create file scratch directory");
        Self {
            state: Box::new(State {
                root,
                pending: None,
            }),
        }
    }
    pub fn config(&mut self) -> RunConfig<'static> {
        let mut config = RunConfig::default().with_enabled_modules(&["node:fs/promises"]);
        let mut provider = ffi::FileProvider::default();
        provider.userdata = (&mut *self.state as *mut State).cast();
        provider.read = Some(read);
        provider.write = Some(write);
        config.file_provider = Some(provider);
        config
    }
    pub fn root(&self) -> &std::path::Path {
        &self.state.root
    }
    pub fn library(&self) -> NativeLibrary {
        let root = self.state.root.to_string_lossy();
        let source = self.state.root.join("root.c");
        std::fs::write(
            &source,
            format!(
                "const char* subscript_test_file_root(void) {{ return {:?}; }}\n",
                root.as_ref()
            ),
        )
        .expect("write scratch root C source");
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        // SAFETY: this library defines only a pre-init setup hook; scripts call the runtime dispatch entry.
        unsafe {
            NativeLibrary::new(
                vec![repo.join("runtime/include")],
                vec![repo.join("corpus/interop/files.c"), source],
                Vec::new(),
            )
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.state.root);
    }
}

unsafe fn error(ctx: *mut Context, endpoint: CompletionEndpoint, message: &[u8]) {
    let status = unsafe {
        ffi::subscript_rt_complete_error(ctx, endpoint, message.as_ptr().cast(), message.len())
    };
    assert_eq!(status, CompletionStatus::Ok);
}
unsafe fn finish(ctx: *mut Context, path: PathBuf, form: u32, endpoint: CompletionEndpoint) {
    match std::fs::read(path) {
        Ok(bytes) => {
            let status = if form == 0 {
                unsafe {
                    ffi::subscript_rt_complete_string(
                        ctx,
                        endpoint,
                        bytes.as_ptr().cast(),
                        bytes.len(),
                    )
                }
            } else {
                unsafe {
                    ffi::subscript_rt_complete_bytes(ctx, endpoint, bytes.as_ptr(), bytes.len())
                }
            };
            if matches!(
                status,
                CompletionStatus::InvalidUtf8 | CompletionStatus::TooLarge
            ) {
                unsafe {
                    error(ctx, endpoint, b"invalid file result");
                }
            } else {
                assert_eq!(status, CompletionStatus::Ok);
            }
        }
        Err(_) => unsafe {
            error(ctx, endpoint, b"missing file");
        },
    }
}
unsafe extern "C" fn read(
    ctx: *mut Context,
    userdata: *mut c_void,
    path: *const u8,
    length: usize,
    form: u32,
    endpoint: CompletionEndpoint,
) {
    let state = unsafe { &mut *userdata.cast::<State>() };
    let path = std::str::from_utf8(unsafe { std::slice::from_raw_parts(path, length) })
        .expect("UTF-8 path");
    let filename = state.root.join(path);
    if path == "pending.txt" {
        state.pending = Some((filename, form, endpoint));
    } else {
        unsafe {
            finish(ctx, filename, form, endpoint);
        }
    }
}
unsafe extern "C" fn write(
    ctx: *mut Context,
    userdata: *mut c_void,
    path: *const u8,
    length: usize,
    data: *const u8,
    data_length: usize,
    _form: u32,
    endpoint: CompletionEndpoint,
) {
    let state = unsafe { &mut *userdata.cast::<State>() };
    let path = std::str::from_utf8(unsafe { std::slice::from_raw_parts(path, length) })
        .expect("UTF-8 path");
    // The data pointer can be NULL when the length is 0.
    let data = if data_length == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(data, data_length) }
    };
    match std::fs::write(state.root.join(path), data) {
        Ok(()) => {
            assert_eq!(
                unsafe { ffi::subscript_rt_complete_void(ctx, endpoint) },
                CompletionStatus::Ok
            );
        }
        Err(_) => unsafe {
            error(ctx, endpoint, b"write failed");
        },
    }
    if let Some((path, form, endpoint)) = state.pending.take() {
        unsafe {
            finish(ctx, path, form, endpoint);
        }
    }
}
