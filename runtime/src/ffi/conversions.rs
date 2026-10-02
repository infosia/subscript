macro_rules! format_integer {
    ($name:ident, $ty:ty, $formatter:ident) => {
        #[doc = concat!("Formats one `", stringify!($ty), "` (Q14).")]
        ///
        /// # Safety
        ///
        /// Shared contract.
        #[no_mangle]
        pub unsafe extern "C" fn $name(ctx: *mut Context, value: $ty, pos_id: u32) -> *mut u8 {
            let mut storage = [0; crate::fmt::INTEGER_BUFFER_SIZE];
            let bytes = crate::fmt::$formatter(value, &mut storage);
            // SAFETY: shared contract.
            unsafe { &mut *ctx }.alloc_str(bytes, pos_id)
        }
    };
}

/// Declares the C entry of a pure unary `Math` member: `f(ctx, x)`
/// forwarding to [`crate::math`].
macro_rules! math_ffi_unary {
    ($( $(#[$doc:meta])* $sym:ident => $imp:ident ),* $(,)?) => {
        $(
            $(#[$doc])*
            #[no_mangle]
            pub extern "C" fn $sym(ctx: *mut Context, x: f64) -> f64 {
                let _ = ctx; // uniform signature; the operation is pure
                crate::math::$imp(x)
            }
        )*
    };
}

/// Declares the C entry of a pure binary `Math` member: `f(ctx, a, b)`
/// forwarding to [`crate::math`].
macro_rules! math_ffi_binary {
    ($( $(#[$doc:meta])* $sym:ident => $imp:ident ),* $(,)?) => {
        $(
            $(#[$doc])*
            #[no_mangle]
            pub extern "C" fn $sym(ctx: *mut Context, a: f64, b: f64) -> f64 {
                let _ = ctx; // uniform signature; the operation is pure
                crate::math::$imp(a, b)
            }
        )*
    };
}

macro_rules! json_integer {
    ($name:ident, $ty:ty, $method:ident) => {
        #[doc = concat!("Appends one JSON integer through the shared Q14 formatter.")]
        ///
        /// # Safety
        ///
        /// Shared contract; `builder` is live.
        #[no_mangle]
        pub unsafe extern "C" fn $name(ctx: *mut Context, builder: u64, value: $ty, pos_id: u32) {
            // SAFETY: shared contract.
            let ctx = unsafe { &mut *ctx };
            let ok = ctx.json_builders().$method(builder, value);
            json_builder_result(ctx, ok, stringify!($name), pos_id);
        }
    };
}
