use subscript_runtime::{context::Context, ffi};

#[test]
fn runtime_flag_accessors_read_each_accepted_flag_set_without_allocation() {
    let accessors: [(
        unsafe extern "C" fn(*mut Context, *const u8, u32) -> i32,
        u8,
    ); 7] = [
        (ffi::subscript_rt_regex_global, b'g'),
        (ffi::subscript_rt_regex_ignore_case, b'i'),
        (ffi::subscript_rt_regex_multiline, b'm'),
        (ffi::subscript_rt_regex_dot_all, b's'),
        (ffi::subscript_rt_regex_unicode, b'u'),
        (ffi::subscript_rt_regex_has_indices, b'd'),
        (ffi::subscript_rt_regex_sticky, b'y'),
    ];
    for mask in 0..128 {
        let flags: String = b"dgimsuv"
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, flag)| char::from(*flag))
            .collect();
        if flags.contains('u') && flags.contains('v') {
            continue;
        }
        let mut ctx = Context::new();
        let pattern = ctx.alloc_str(b"a", 0);
        let flag_handle = ctx.alloc_str(flags.as_bytes(), 0);
        // SAFETY: all handles belong to the live Context.
        let regex = unsafe { ffi::subscript_rt_regex_new(&mut *ctx, pattern, flag_handle, 0) };
        assert!(!regex.is_null(), "{flags}");
        let count = ctx.live_count();
        ctx.fail_alloc_after(1);
        for (accessor, flag) in accessors {
            // SAFETY: regex belongs to the live Context.
            assert_eq!(
                unsafe { accessor(&mut *ctx, regex, 0) },
                i32::from(flags.as_bytes().contains(&flag)),
                "{flags}: {flag}"
            );
        }
        assert_eq!(ctx.live_count(), count);
        // The pending fault fires on the next allocation.
        assert!(ctx.alloc_str(b"allocation control", 0).is_null());
    }
}

#[test]
fn runtime_to_string_reuses_source_and_canonical_flags() {
    for (pattern, flags, expected) in [
        ("a/b", "mi", "/a\\/b/im"),
        ("", "", "/(?:)/"),
        ("[/]", "d", "/[/]/d"),
        ("a\\/b", "", "/a\\/b/"),
        ("a\nb", "", "/a\\nb/"),
    ] {
        let mut ctx = Context::new();
        let pattern = ctx.alloc_str(pattern.as_bytes(), 0);
        let flags = ctx.alloc_str(flags.as_bytes(), 0);
        // SAFETY: all handles belong to the live Context.
        unsafe {
            let regex = ffi::subscript_rt_regex_new(&mut *ctx, pattern, flags, 0);
            let text = ffi::subscript_rt_regex_to_string(&mut *ctx, regex, 0);
            assert!(!text.is_null());
            assert_eq!(ctx.str_bytes(text), expected.as_bytes());
        }
    }
}
