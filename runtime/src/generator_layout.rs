//! Native generator header layout shared by the runtime and code generators.

/// State word offset.
pub const STATE_OFFSET: u32 = 0;
/// Reload epoch offset.
pub const EPOCH_OFFSET: u32 = 4;
/// Resume function pointer offset.
pub const RESUME_OFFSET: u32 = 8;
/// Holder count offset.
pub const HOLDERS_OFFSET: u32 = 16;
/// Static cleanup description pointer offset.
pub const CLEANUP_OFFSET: u32 = 24;
/// Temporary result destination during a close invocation.
pub const CLOSE_OUTPUT_OFFSET: u32 = 32;
/// First payload byte offset.
pub const PAYLOAD_OFFSET: u32 = 40;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_the_native_header() {
        #[repr(C)]
        struct Header {
            state: i32,
            epoch: u32,
            resume: usize,
            holders: u32,
            padding: u32,
            cleanup: *const u64,
            close_output: *mut u8,
        }
        assert_eq!(STATE_OFFSET as usize, std::mem::offset_of!(Header, state));
        assert_eq!(EPOCH_OFFSET as usize, std::mem::offset_of!(Header, epoch));
        assert_eq!(RESUME_OFFSET as usize, std::mem::offset_of!(Header, resume));
        assert_eq!(
            HOLDERS_OFFSET as usize,
            std::mem::offset_of!(Header, holders)
        );
        assert_eq!(
            CLEANUP_OFFSET as usize,
            std::mem::offset_of!(Header, cleanup)
        );
        assert_eq!(
            CLOSE_OUTPUT_OFFSET as usize,
            std::mem::offset_of!(Header, close_output)
        );
        assert_eq!(PAYLOAD_OFFSET as usize, std::mem::size_of::<Header>());
    }
}
