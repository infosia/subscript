//! A bounded worker pool for the corpus sweeps.
//!
//! A work item returns its lines and its findings, and the test thread
//! prints them and asserts. The print order is then the corpus order,
//! which is the order the sequential loop printed.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Runs `work` over every item on a bounded pool of worker threads and
/// returns the results in the order of `items`.
///
/// The pool holds `std::thread::available_parallelism()` workers, with a
/// floor of 1 and a ceiling of the item count. One shared index hands the
/// items out in order, so the work order is stable.
///
/// A work item must not depend on process-wide state that another item
/// changes: an environment variable, the current directory, or a fixed
/// file name. The sweeps keep their host-hook entries off the pool for
/// that reason.
///
/// Every worker is joined. The first worker panic resumes on the calling
/// thread after the pool finishes, so the panic reaches the caller.
pub fn map_in_order<T, R, F>(items: &[T], work: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    if items.is_empty() {
        return Vec::new();
    }
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(items.len());
    let next = AtomicUsize::new(0);
    let work = &work;
    let next = &next;
    let (mut indexed, panic) = std::thread::scope(|scope| {
        let handles = (0..workers)
            .map(|_| {
                scope.spawn(move || {
                    let mut mine: Vec<(usize, R)> = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else {
                            break;
                        };
                        mine.push((index, work(item)));
                    }
                    mine
                })
            })
            .collect::<Vec<_>>();
        let mut indexed: Vec<(usize, R)> = Vec::with_capacity(items.len());
        let mut panic = None;
        for handle in handles {
            match handle.join() {
                Ok(part) => indexed.extend(part),
                Err(payload) => {
                    if panic.is_none() {
                        panic = Some(payload);
                    }
                }
            }
        }
        (indexed, panic)
    });
    if let Some(payload) = panic {
        std::panic::resume_unwind(payload);
    }
    indexed.sort_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, result)| result).collect()
}
