//! The Context file-provider ABI and standard-operation dispatch.
use super::*;

/// Starts a read; borrowed path bytes expire when this callback returns.
/// Form 0 requests UTF-8 text; form 1 requests bytes. Complete the endpoint once on the owner thread.
pub type FileReadCallback = unsafe extern "C" fn(
    ctx: *mut Context,
    userdata: *mut c_void,
    path: *const u8,
    length: usize,
    form: u32,
    endpoint: CompletionEndpoint,
);
/// Starts a write; borrowed path and data bytes expire when this callback returns.
/// Form 0 supplies UTF-8 text; form 1 supplies bytes. Complete the endpoint once on the owner thread.
/// `data` can be NULL when `data_length` is 0.
pub type FileWriteCallback = unsafe extern "C" fn(
    ctx: *mut Context,
    userdata: *mut c_void,
    path: *const u8,
    length: usize,
    data: *const u8,
    data_length: usize,
    form: u32,
    endpoint: CompletionEndpoint,
);

/// Host callbacks for the enabled file module (§185).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct FileProvider {
    /// Size of this provider record in bytes; a smaller size disables the provider.
    pub size: usize,
    /// Host state that remains valid while the provider is installed.
    pub userdata: *mut c_void,
    /// Optional read callback; an absent callback completes with an Error.
    pub read: Option<FileReadCallback>,
    /// Optional write callback; an absent callback completes with an Error.
    /// Its `data` can be NULL when `data_length` is 0.
    pub write: Option<FileWriteCallback>,
}
impl Default for FileProvider {
    fn default() -> Self {
        Self {
            size: std::mem::size_of::<Self>(),
            userdata: std::ptr::null_mut(),
            read: None,
            write: None,
        }
    }
}
impl Context {
    /// Copies or removes the provider before script initialization or between host steps.
    /// Replacing a provider does not cancel pending requests.
    ///
    /// # Safety
    /// The callback and userdata must remain valid through their requests. Call on the Context owner thread.
    pub unsafe fn set_file_provider(&mut self, provider: Option<FileProvider>) {
        self.file_provider = provider.filter(|p| p.size >= std::mem::size_of::<FileProvider>());
    }
}

/// Copies the provider record onto the Context; the runtime keeps no pointer to it.
/// A null pointer removes the provider.
/// A record whose `size` is smaller than the current record disables the provider.
/// Set the provider before `subscript_init`, so that the module initializer sees it.
/// Replacing or removing a provider does not cancel pending requests.
/// A Worker has no provider: it runs on its own thread.
///
/// # Safety
/// The Context is live and owner-thread access is exclusive. A non-null provider points to a readable size field.
/// If its size is sufficient, the full record and callbacks must satisfy Context::set_file_provider.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_set_file_provider(
    ctx: *mut Context,
    provider: *const FileProvider,
) {
    let value = if provider.is_null()
        || unsafe { provider.cast::<usize>().read() } < std::mem::size_of::<FileProvider>()
    {
        None
    } else {
        Some(unsafe { provider.read() })
    };
    unsafe {
        (*ctx).set_file_provider(value);
    }
}

/// Dispatches one verified standard operation after source creation.
/// Operation 0=read text, 1=read bytes, 2=write text, 3=write bytes.
///
/// # Safety
/// The Context, managed path, write value, and endpoint follow the verified operation signature.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_file_operation(
    ctx: *mut Context,
    operation: u32,
    path: *const u8,
    data: *const u8,
    endpoint: *const CompletionEndpoint,
) {
    let roots = [path as usize, data as usize];
    unsafe {
        (*ctx).shadow_push(roots.as_ptr() as usize, roots.len());
    }
    unsafe {
        dispatch(ctx, operation, path, data, endpoint);
    }
    unsafe {
        (*ctx).shadow_pop();
    }
}
unsafe fn dispatch(
    ctx: *mut Context,
    operation: u32,
    path: *const u8,
    data: *const u8,
    endpoint: *const CompletionEndpoint,
) {
    let endpoint = unsafe { endpoint.read() };
    let error = |message: &[u8]| {
        unsafe {
            subscript_rt_complete_error(ctx, endpoint, message.as_ptr().cast(), message.len())
        };
    };
    let Some(provider) = (unsafe { (*ctx).file_provider }) else {
        error(b"missing file provider");
        return;
    };
    let path_bytes = unsafe { (*ctx).str_bytes(path) };
    if std::str::from_utf8(path_bytes).is_err() {
        error(b"file path is not UTF-8");
        return;
    }
    let path_pointer = path_bytes.as_ptr();
    let path_length = path_bytes.len();
    if operation < 2 {
        if let Some(read) = provider.read {
            unsafe {
                read(
                    ctx,
                    provider.userdata,
                    path_pointer,
                    path_length,
                    operation,
                    endpoint,
                );
            }
        } else {
            error(b"missing file provider read callback");
        }
    } else if operation < 4 {
        let (pointer, length) = if operation == 2 {
            let text = unsafe { (*ctx).str_bytes(data) };
            if std::str::from_utf8(text).is_err() {
                error(b"file text is not UTF-8");
                return;
            }
            (text.as_ptr(), text.len())
        } else {
            (
                unsafe { (*ctx).array_data(data) },
                unsafe { (*ctx).array_len(data) } as usize,
            )
        };
        if let Some(write) = provider.write {
            unsafe {
                write(
                    ctx,
                    provider.userdata,
                    path_pointer,
                    path_length,
                    pointer,
                    length,
                    operation - 2,
                    endpoint,
                );
            }
        } else {
            error(b"missing file provider write callback");
        }
    } else {
        error(b"unknown file operation");
    }
}
