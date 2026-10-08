use super::{exception_message, PendingException};
use crate::{Context, TrapKind};

// Generated metadata: payload size, class id, kind offset, name offset,
// message offset, and the Error kind tag. The producer verifies the Error class.
#[derive(Clone, Copy)]
pub(crate) struct HostErrorLayout(pub(crate) [u64; 6]);

impl HostErrorLayout {
    pub(crate) fn valid(self) -> bool {
        let [size, class, kind, name, message, tag] = self.0;
        let fields = [
            (kind, 4),
            (name, std::mem::size_of::<usize>() as u64),
            (message, std::mem::size_of::<usize>() as u64),
        ];
        name.is_multiple_of(std::mem::align_of::<usize>() as u64)
            && message.is_multiple_of(std::mem::align_of::<usize>() as u64)
            && kind.is_multiple_of(std::mem::align_of::<u32>() as u64)
            && class <= u32::MAX as u64
            && tag <= u32::MAX as u64
            && usize::try_from(size).is_ok()
            && fields
                .iter()
                .all(|&(offset, width)| offset.checked_add(width).is_some_and(|end| end <= size))
            && fields.iter().enumerate().all(|(i, &(offset, width))| {
                fields[i + 1..].iter().all(|&(other, other_width)| {
                    offset + width <= other || other + other_width <= offset
                })
            })
    }
}

impl Context {
    pub(crate) fn host_error(
        &mut self,
        layout: HostErrorLayout,
        message: &[u8],
        pos_id: u32,
    ) -> Option<PendingException> {
        if !layout.valid() {
            self.trap(TrapKind::Internal, "invalid Error metadata", pos_id);
            return None;
        }
        let [size, class, kind_offset, name_offset, message_offset, tag] = layout.0;
        let object = self.alloc(size as usize, class as u32, pos_id);
        if object.is_null() {
            return None;
        }
        let name = self.alloc_str(b"Error", pos_id);
        if name.is_null() {
            self.delete(object as usize, pos_id);
            return None;
        }
        let text = self.alloc_str(message, pos_id);
        if text.is_null() {
            self.delete(name as usize, pos_id);
            self.delete(object as usize, pos_id);
            return None;
        }
        // SAFETY: the metadata check bounds each field within the fresh allocation.
        unsafe {
            object
                .add(kind_offset as usize)
                .cast::<u32>()
                .write_unaligned(tag as u32);
            object
                .add(name_offset as usize)
                .cast::<*mut u8>()
                .write_unaligned(name);
            object
                .add(message_offset as usize)
                .cast::<*mut u8>()
                .write_unaligned(text);
        }
        Some(PendingException {
            object: object as usize,
            message: exception_message(b"Error", message),
            pos_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_allocation_failure_releases_each_earlier_allocation() {
        for failure in 1..=3 {
            let mut context = Context::new();
            let before = context.live_bytes();
            context.fail_alloc_after(failure);
            assert!(context
                .host_error(HostErrorLayout([24, 0, 0, 8, 16, 0]), b"failure", 9)
                .is_none());
            assert_eq!(context.live_bytes(), before);
            assert_eq!(
                context.trap_record().expect("allocation trap").kind,
                TrapKind::AllocationFailure
            );
        }
        let mut context = Context::new();
        assert!(context
            .host_error(HostErrorLayout([24, 0, 0, 8, 16, 0]), b"success", 9)
            .is_some());
        assert!(context.live_bytes() > 0);
    }
}
