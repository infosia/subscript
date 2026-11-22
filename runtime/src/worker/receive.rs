//! The spawning side of `Worker.receive()` (`compiler.md` §349).
//!
//! A receive that waits is a frame handle in the FIFO of its Worker (rule
//! 3). The Context delivers to those frames at a scheduler step (rule 4).
//! The Worker thread touches only the outbox queue and the signal of the
//! Context (rule 10); it never reads or writes the Context.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use super::{QueueDescriptor, Receive, Worker, WorkerOutcome, WorkerSet};

/// The one signal of a Context (`compiler.md` §349 rule 10).
///
/// Each outbox of the Workers of the Context sets it at a post, at the end
/// of the Worker, and at a trap, under the lock of that outbox. A step
/// clears it before it reads the outboxes, so a post after the clear sets
/// it again. The lock order is an outbox lock, then the signal lock.
#[derive(Default)]
pub(crate) struct ReceiveSignal {
    set: AtomicBool,
    lock: Mutex<()>,
    ready: Condvar,
}

impl ReceiveSignal {
    fn lock(&self) -> MutexGuard<'_, ()> {
        match self.lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Sets the signal and wakes the thread that waits on it (rule 11).
    /// The flag changes under the lock, so a wait that tested the flag
    /// under the same lock cannot miss the wake.
    pub(super) fn set(&self) {
        let _guard = self.lock();
        self.set.store(true, Ordering::SeqCst);
        self.ready.notify_all();
    }

    fn is_set(&self) -> bool {
        self.set.load(Ordering::SeqCst)
    }

    /// Sets the flag with no wake. A step that ends on a trap uses it, so
    /// that the first step after the trap is cleared reads the outboxes
    /// again (rule 6).
    fn mark(&self) {
        self.set.store(true, Ordering::SeqCst);
    }

    /// Clears the signal and returns whether it was set.
    fn take(&self) -> bool {
        self.set.swap(false, Ordering::SeqCst)
    }

    /// Blocks until the signal is set. The thread sleeps on the condition
    /// variable; it does not test the flag in a loop of its own.
    fn wait(&self) {
        let mut guard = self.lock();
        while !self.is_set() {
            guard = match self.ready.wait(guard) {
                Ok(next) => next,
                Err(poisoned) => poisoned.into_inner(),
            };
        }
    }
}

/// What a receive does at its call (`compiler.md` §349 rules 2, 5, and 6).
pub(crate) enum ReceiveAtCall {
    /// Rule 2: the oldest message, which completes the receive.
    Message(Receive),
    /// Rule 5: the Worker ended and its outbox is empty.
    Ended,
    /// Rule 6: the Worker trapped and its outbox is empty.
    Trapped(WorkerOutcome),
    /// The receive waits in the FIFO of its Worker.
    Wait,
}

/// One delivery of a step (`compiler.md` §349 rules 3 to 6).
pub(crate) enum ReceiveDelivery {
    /// The oldest message for the oldest receive that waits.
    Message {
        /// The receive frame that the message completes.
        frame: *mut u8,
        /// The message record, which the Context materializes (rule 13).
        record: Box<[u8]>,
        /// The layout of the record.
        descriptor: Arc<QueueDescriptor>,
    },
    /// The Worker ended and its outbox is empty: the frame completes with null.
    Ended(*mut u8),
    /// The Worker trapped and its outbox is empty: the Context traps.
    /// The receive stays in the FIFO.
    Trapped(WorkerOutcome),
}

impl Worker {
    /// Rule 2: a receive completes at the call only if no earlier receive
    /// of this Worker waits.
    fn receive_at_call(&self) -> ReceiveAtCall {
        if !self.receives.is_empty() {
            return ReceiveAtCall::Wait;
        }
        let mut state = self.output.lock();
        if let Some(record) = state.messages.pop_front() {
            return ReceiveAtCall::Message(Receive::Message {
                record,
                descriptor: Arc::clone(&self.output.descriptor),
            });
        }
        if !state.closed {
            return ReceiveAtCall::Wait;
        }
        match closed_outcome(state.outcome.as_ref()) {
            WorkerOutcome::Clean => ReceiveAtCall::Ended,
            outcome => ReceiveAtCall::Trapped(outcome),
        }
    }
}

/// Rule 8: the outcome that a closed outbox carries. An outbox that closed
/// with no outcome closed when its thread ended without a runtime outcome.
fn closed_outcome(outcome: Option<&WorkerOutcome>) -> WorkerOutcome {
    outcome.cloned().unwrap_or(WorkerOutcome::ThreadFailed)
}

impl WorkerSet {
    /// The signal that each outbox of this set shares (rule 10).
    pub(super) fn receive_signal(&self) -> Arc<ReceiveSignal> {
        Arc::clone(&self.signal)
    }

    /// What a receive of `handle` does at its call. `None` reports a
    /// handle that this set does not own.
    pub(crate) fn receive_at_call(&self, handle: *mut Worker) -> Option<ReceiveAtCall> {
        Some(self.find(handle)?.receive_at_call())
    }

    /// Appends `frame` to the FIFO of `handle` (rule 3). Returns `false`
    /// for a handle that this set does not own.
    pub(crate) fn receive_wait(&mut self, handle: *mut Worker, frame: *mut u8) -> bool {
        let Some(worker) = self.find_mut(handle) else {
            return false;
        };
        worker.receives.push_back(frame);
        self.waiting += 1;
        true
    }

    /// Rule 15: the one test of a count that a step with no receive pays.
    pub(crate) fn has_waiting_receive(&self) -> bool {
        self.waiting != 0
    }

    /// Rule 10: a receive waits and the signal is set.
    pub(crate) fn receive_can_complete(&self) -> bool {
        self.waiting != 0 && self.signal.is_set()
    }

    /// Clears the signal before a step reads the outboxes (rule 10).
    pub(crate) fn take_receive_signal(&self) -> bool {
        self.signal.take()
    }

    /// Sets the signal again with no wake, after a step that read the
    /// outboxes ended on a trap (rule 6).
    pub(crate) fn restore_receive_signal(&self) {
        self.signal.mark();
    }

    /// Rule 11: blocks until the signal is set. The caller tests first
    /// that a receive waits.
    pub(crate) fn wait_receive_signal(&self) {
        self.signal.wait();
    }

    /// The number of Workers, in spawn order.
    pub(crate) fn worker_count(&self) -> usize {
        self.workers.len()
    }

    /// The next delivery to the receives of the Worker at `index`, in
    /// spawn order (rule 4). It locks the outbox only if a receive waits.
    pub(crate) fn next_receive_delivery(&mut self, index: usize) -> Option<ReceiveDelivery> {
        let worker = self.workers.get_mut(index)?;
        let frame = *worker.receives.front()?;
        let delivery = {
            let mut state = worker.output.lock();
            if let Some(record) = state.messages.pop_front() {
                ReceiveDelivery::Message {
                    frame,
                    record,
                    descriptor: Arc::clone(&worker.output.descriptor),
                }
            } else if !state.closed {
                return None;
            } else {
                match closed_outcome(state.outcome.as_ref()) {
                    WorkerOutcome::Clean => ReceiveDelivery::Ended(frame),
                    outcome => return Some(ReceiveDelivery::Trapped(outcome)),
                }
            }
        };
        worker.receives.pop_front();
        self.waiting -= 1;
        Some(delivery)
    }

    /// Puts `frame` back at the head of the FIFO of the Worker at `index`,
    /// and `record` back at the head of its outbox. A delivery whose message
    /// the Context could not materialize uses it: the Context traps, the
    /// frame stays a receive that waits, and the message stays the oldest.
    pub(crate) fn restore_receive(&mut self, index: usize, frame: *mut u8, record: Box<[u8]>) {
        if let Some(worker) = self.workers.get_mut(index) {
            worker.receives.push_front(frame);
            worker.output.lock().messages.push_front(record);
            self.waiting += 1;
        }
    }
}
