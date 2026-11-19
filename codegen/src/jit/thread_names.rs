//! Thread details for compiler.md §340 rule 5. Counts stay in `isolation`.

pub(super) fn other_threads() -> String {
    let names = platform_names();
    if names.is_empty() {
        "unnamed (thread details unavailable)".into()
    } else {
        names.join(", ")
    }
}

#[cfg(target_os = "linux")]
fn platform_names() -> Vec<String> {
    // SAFETY: gettid reads the calling thread's ID and takes no pointers.
    let caller = unsafe { libc::syscall(libc::SYS_gettid) }.to_string();
    let Ok(tasks) = std::fs::read_dir("/proc/self/task") else {
        return Vec::new();
    };
    tasks
        .flatten()
        .filter(|task| task.file_name() != caller.as_str())
        .map(|task| {
            let name = std::fs::read_to_string(task.path().join("comm")).unwrap_or_default();
            let name = name.trim_end_matches('\n');
            format!(
                "{} ({})",
                if name.is_empty() { "unnamed" } else { name },
                task.file_name().to_string_lossy()
            )
        })
        .collect()
}

#[cfg(not(any(target_vendor = "apple", target_os = "linux")))]
fn platform_names() -> Vec<String> {
    Vec::new()
}

#[cfg(target_vendor = "apple")]
fn platform_names() -> Vec<String> {
    extern "C" {
        static mach_task_self_: libc::mach_port_t;
        fn mach_port_deallocate(
            task: libc::mach_port_t,
            name: libc::mach_port_t,
        ) -> libc::kern_return_t;
    }
    // SAFETY: the loader initializes the task port before this function runs.
    let task = unsafe { mach_task_self_ };
    // SAFETY: pthread_self identifies the caller; the conversion returns its borrowed Mach port.
    let caller = unsafe { libc::pthread_mach_thread_np(libc::pthread_self()) };
    let mut list: libc::thread_act_array_t = std::ptr::null_mut();
    let mut count = 0;
    // SAFETY: both out-pointers refer to live storage.
    if unsafe { libc::task_threads(task, &mut list, &mut count) } != 0 {
        return Vec::new();
    }
    let mut names = Vec::new();
    for index in 0..count as usize {
        // SAFETY: task_threads returned count initialized ports in list.
        let port = unsafe { *list.add(index) };
        if port != caller {
            // SAFETY: the retained port belongs to this task. An exited thread can give zero.
            let pthread = unsafe { libc::pthread_from_mach_thread_np(port) };
            let mut detail = format!("{} (Mach thread {port})", pthread_name(pthread));
            if let Some(label) = queue_label(task, port) {
                detail.push_str(&format!(" [dispatch queue: {label}]"));
            }
            names.push(detail);
        }
        // SAFETY: release the send right that task_threads returned for this port.
        unsafe { mach_port_deallocate(task, port) };
    }
    // SAFETY: task_threads allocated this exact array in this task. All reads are complete.
    unsafe {
        libc::vm_deallocate(
            task,
            list as libc::vm_address_t,
            count as usize * std::mem::size_of::<libc::thread_act_t>(),
        )
    };
    names
}

#[cfg(target_vendor = "apple")]
fn pthread_name(pthread: libc::pthread_t) -> String {
    use std::ffi::CStr;

    if pthread == 0 {
        return "unnamed".into();
    }
    let mut name = [0; 64];
    // SAFETY: pthread_getname_np checks this handle against the live thread list
    // under its lock before it reads the name. A concurrent exit returns ESRCH.
    // https://github.com/apple-oss-distributions/libpthread/blob/main/src/pthread.c
    let status = unsafe { libc::pthread_getname_np(pthread, name.as_mut_ptr(), name.len()) };
    if status != 0 {
        return "unnamed".into();
    }
    // SAFETY: the zeroed buffer stays terminated; getname writes a bounded C string.
    let name = unsafe { CStr::from_ptr(name.as_ptr()) }.to_string_lossy();
    if name.is_empty() {
        "unnamed".into()
    } else {
        name.into_owned()
    }
}

#[cfg(target_vendor = "apple")]
fn queue_label(task: libc::mach_port_t, thread: libc::mach_port_t) -> Option<String> {
    // The prefix comes from libdispatch's private/layout_private.h debugger ABI.
    // https://github.com/apple-oss-distributions/libdispatch
    #[repr(C)]
    struct QueueOffsets {
        version: u16,
        label: u16,
        label_size: u16,
    }
    extern "C" {
        fn mach_vm_read_overwrite(
            task: libc::mach_port_t,
            address: u64,
            size: u64,
            data: u64,
            outsize: *mut u64,
        ) -> libc::kern_return_t;
    }
    fn read(task: libc::mach_port_t, address: u64, bytes: &mut [u8]) -> Option<()> {
        if address == 0 {
            return None;
        }
        let mut size = 0;
        // SAFETY: the kernel copies readable memory to the live byte slice. Invalid addresses
        // return an error, including when another thread releases a queue during this read.
        let result = unsafe {
            mach_vm_read_overwrite(
                task,
                address,
                bytes.len() as u64,
                bytes.as_mut_ptr() as u64,
                &mut size,
            )
        };
        (result == 0 && size == bytes.len() as u64).then_some(())
    }
    fn pointer(task: libc::mach_port_t, address: u64) -> Option<u64> {
        let mut bytes = [0; std::mem::size_of::<usize>()];
        read(task, address, &mut bytes)?;
        Some(usize::from_ne_bytes(bytes) as u64)
    }
    // SAFETY: zero is valid for the integer-only Mach info structure.
    let mut info: libc::thread_identifier_info = unsafe { std::mem::zeroed() };
    let mut count = libc::THREAD_IDENTIFIER_INFO_COUNT;
    // SAFETY: info is live storage of the requested flavor and count.
    if unsafe {
        libc::thread_info(
            thread,
            libc::THREAD_IDENTIFIER_INFO as libc::thread_flavor_t,
            (&mut info as *mut libc::thread_identifier_info).cast(),
            &mut count,
        )
    } != 0
    {
        return None;
    }
    let queue = pointer(task, info.dispatch_qaddr)?;
    if queue == 0 {
        return None;
    }
    // SAFETY: dlsym takes a static terminated symbol name and returns its address or null.
    let offsets = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"dispatch_queue_offsets".as_ptr()) };
    let mut prefix = [0; std::mem::size_of::<QueueOffsets>()];
    read(task, offsets as u64, &mut prefix)?;
    let version = u16::from_ne_bytes([prefix[0], prefix[1]]);
    let offset = u16::from_ne_bytes([prefix[2], prefix[3]]);
    let size = u16::from_ne_bytes([prefix[4], prefix[5]]);
    if version == 0 {
        return None;
    }
    let address = queue.checked_add(offset as u64)?;
    let label = if version >= 4 {
        if size as usize != std::mem::size_of::<usize>() {
            return None;
        }
        pointer(task, address)?
    } else {
        address
    };
    let mut bytes = Vec::new();
    let mut index = 0;
    loop {
        let mut byte = [0];
        read(task, label.checked_add(index)?, &mut byte)?;
        if byte[0] == 0 {
            return Some(String::from_utf8_lossy(&bytes).into_owned());
        }
        bytes.push(byte[0]);
        index = index.checked_add(1)?;
    }
}

#[cfg(all(test, target_vendor = "apple"))]
mod tests {
    use super::*;
    use std::sync::mpsc::{channel, Receiver, Sender};

    // compiler.md §340 rule 5: debug 7.6 ms; 0 C builds.
    #[test]
    fn a_dispatch_thread_names_its_queue_and_an_unnamed_thread() {
        let measured = std::time::Instant::now();
        // compiler.md §340 rule 5: a vanished pthread handle gives the unnamed fallback.
        assert_eq!(pthread_name(0), "unnamed");
        extern "C" {
            fn dispatch_queue_create(
                label: *const libc::c_char,
                attr: *const libc::c_void,
            ) -> *mut libc::c_void;
            fn dispatch_async_f(
                queue: *mut libc::c_void,
                context: *mut libc::c_void,
                callback: extern "C" fn(*mut libc::c_void),
            );
            fn dispatch_release(queue: *mut libc::c_void);
        }
        struct State {
            ready: Sender<()>,
            stop: Receiver<()>,
            done: Sender<()>,
        }
        extern "C" fn callback(context: *mut libc::c_void) {
            // SAFETY: the test transfers one Box<State> to this callback exactly once.
            let state = unsafe { Box::from_raw(context.cast::<State>()) };
            // SAFETY: an empty terminated string clears the name of this calling pthread.
            unsafe { libc::pthread_setname_np(c"".as_ptr()) };
            let _ = state.ready.send(());
            let _ = state.stop.recv();
            let _ = state.done.send(());
        }
        let (ready, started) = channel();
        let (stop, wait) = channel();
        let (done, finished) = channel();
        let state = Box::new(State {
            ready,
            stop: wait,
            done,
        });
        // SAFETY: the label is static and null selects a serial queue's default attributes.
        let queue = unsafe { dispatch_queue_create(c"s340-test-queue".as_ptr(), std::ptr::null()) };
        assert!(!queue.is_null());
        // SAFETY: queue stays retained through completion. The callback owns the transferred Box.
        unsafe { dispatch_async_f(queue, Box::into_raw(state).cast(), callback) };
        started.recv().unwrap();
        let details = other_threads();
        stop.send(()).unwrap();
        finished.recv().unwrap();
        // SAFETY: the test owns queue's creation reference; dispatch retains active work itself.
        unsafe { dispatch_release(queue) };
        assert!(
            details
                .split(", ")
                .any(|thread| thread.starts_with("unnamed ")
                    && thread.contains("dispatch queue: s340-test-queue")),
            "{details}"
        );
        eprintln!(
            "a_dispatch_thread_names_its_queue_and_an_unnamed_thread cost: {:?}",
            measured.elapsed()
        );
    }
}
