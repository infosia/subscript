//! The statistics of the collections of a Context (`compiler.md` §353).

use super::*;

// The host header declares this record as `subscript_rt_collection_stats`,
// with the same fields in the same sequence (`compiler.md` §353 rule 2).

/// The record of the collections of one Context (`compiler.md` §353 rule 2).
///
/// The record starts with its Context. Each field is for the span from the
/// start of the record or from the last reset. `max_ns` and
/// `max_words_read` are each the largest of one collection in that span.
/// Before the first collection each field is zero.
///
/// A collection whose mark runs in steps is a cycle of pauses: the start,
/// the mark steps, and the end (`compiler.md` §356 rule 17). For a cycle,
/// `last_ns` and `max_ns` are of one pause, and `total_ns` is the sum of
/// the pauses. `automatic_collections` counts the cycles that ended.
/// `last_words_read` and `max_words_read` are of one whole cycle. The dev
/// tier and the interpreter run each collection in one pause, so there
/// `mark_steps` and `forced_ends` stay zero.
#[repr(C)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CollectionStats {
    /// The automatic collections: at a poll, at an outermost exit, and the
    /// detection collection. A cycle in steps counts when it ends.
    pub automatic_collections: u64,
    /// The explicit collections: `Context.collect()` and
    /// `subscript_rt_ctx_collect`.
    pub explicit_collections: u64,
    /// The sum of the times of the collections, in nanoseconds. For a cycle
    /// in steps, the sum of its pauses.
    pub total_ns: u64,
    /// The largest time of one collection, in nanoseconds. For a cycle in
    /// steps, the time of one pause.
    pub max_ns: u64,
    /// The time of the last collection, in nanoseconds. For a cycle in
    /// steps, the time of the last pause.
    pub last_ns: u64,
    /// The words that the mark of the last collection tested as a possible
    /// reference, in the roots and in the payloads.
    pub last_words_read: u64,
    /// The largest count of words that the mark of one collection read. It
    /// can come from a collection other than the one of `max_ns`.
    pub max_words_read: u64,
    /// The live bytes after the last collection.
    pub last_live_bytes_after: u64,
    /// The mark steps of the cycles in steps: each pause that is not the
    /// start or the end of a cycle.
    pub mark_steps: u64,
    /// The cycles that became late: the bytes requested since the start of
    /// the cycle exceeded one limit, and from there each poll and each
    /// outermost exit ran one step with the budget (`compiler.md` §356 rule
    /// 7).
    pub forced_ends: u64,
    /// The largest count of words that one pause read. A collection with no
    /// steps is one pause.
    pub max_pause_words_read: u64,
}

/// The state of the statistics in the collection state of a Context.
#[derive(Debug, Default)]
pub(super) struct StatsState {
    record: CollectionStats,
    // compiler.md §353 rule 5: the words that the mark tested since the
    // start of the collection that is timed.
    mark_words: u64,
    // compiler.md §356 rule 17: the words that the earlier pauses of a
    // cycle and the stores of rule 10 gave. The end of the cycle adds them
    // to its own words for the words of the whole cycle.
    carried_words: u64,
    // compiler.md §356 rule 5: the time and the words of the mark of an
    // end whose releases and sweep follow in the same pause. The next start
    // continues them.
    continued: Option<(std::time::Instant, u64)>,
}

/// A timed collection that started (`compiler.md` §353 rule 4).
pub(super) struct StatsStart(std::time::Instant);

impl StatsState {
    /// Starts the time and the word count of one collection.
    pub(super) fn start(&mut self) -> StatsStart {
        if let Some((started, words)) = self.continued.take() {
            self.mark_words = words;
            return StatsStart(started);
        }
        self.mark_words = 0;
        StatsStart(std::time::Instant::now())
    }

    /// `compiler.md` §356 rule 5: the next start continues the pause that
    /// `start` started, with the `words` that it read.
    pub(super) fn continue_pause(&mut self, start: StatsStart, words: u64) {
        self.continued = Some((start.0, words));
    }

    /// `compiler.md` §353 rule 5: adds the words that one drain of the mark
    /// tested.
    #[inline]
    pub(super) fn add_mark_words(&mut self, words: usize) {
        self.mark_words = self.mark_words.saturating_add(words as u64);
    }

    /// The words that the end of a cycle carries from the earlier pauses
    /// (`compiler.md` §356 rule 17).
    pub(super) fn carry_words(&mut self, words: u64) {
        self.carried_words = self.carried_words.saturating_add(words);
    }

    /// Removes words that the drain counted and an earlier pause of the
    /// cycle counted already (`compiler.md` §356 rule 17).
    pub(super) fn remove_mark_words(&mut self, words: usize) {
        self.mark_words = self.mark_words.saturating_sub(words as u64);
    }

    /// Records one collection that ended (`compiler.md` §353 rules 1 and 4).
    /// For the end of a cycle, the words of the whole cycle are the carried
    /// words and the words of the end (`compiler.md` §356 rule 17).
    pub(super) fn finish(&mut self, start: StatsStart, automatic: bool, live_bytes: usize) {
        let ns = Self::pause_ns(&start);
        let words = self.mark_words;
        let cycle_words = words.saturating_add(std::mem::take(&mut self.carried_words));
        let record = &mut self.record;
        if automatic {
            record.automatic_collections = record.automatic_collections.saturating_add(1);
        } else {
            record.explicit_collections = record.explicit_collections.saturating_add(1);
        }
        Self::add_pause(record, ns, words);
        record.last_words_read = cycle_words;
        record.max_words_read = record.max_words_read.max(cycle_words);
        record.last_live_bytes_after = live_bytes as u64;
    }

    /// Records one pause of a cycle that is not its end: the start or a mark
    /// step (`compiler.md` §356 rule 17). `words` is the count that the
    /// pause read.
    pub(super) fn finish_pause(&mut self, start: StatsStart, words: u64, step: bool) {
        let ns = Self::pause_ns(&start);
        let record = &mut self.record;
        Self::add_pause(record, ns, words);
        if step {
            record.mark_steps = record.mark_steps.saturating_add(1);
        }
        self.carried_words = self.carried_words.saturating_add(words);
    }

    /// Counts one cycle that became late (`compiler.md` §356 rule 7).
    pub(super) fn count_forced_end(&mut self) {
        self.record.forced_ends = self.record.forced_ends.saturating_add(1);
    }

    fn pause_ns(start: &StatsStart) -> u64 {
        u64::try_from(start.0.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }

    fn add_pause(record: &mut CollectionStats, ns: u64, words: u64) {
        record.total_ns = record.total_ns.saturating_add(ns);
        record.max_ns = record.max_ns.max(ns);
        record.last_ns = ns;
        record.max_pause_words_read = record.max_pause_words_read.max(words);
    }
}

impl Context {
    /// The statistics of the collections of this Context (`compiler.md`
    /// §353 rule 2).
    #[must_use]
    pub fn collection_stats(&self) -> CollectionStats {
        self.collection.stats.record
    }

    /// Sets each field of the statistics to zero (`compiler.md` §353 rule
    /// 3). The trigger state and the other counters do not change.
    pub fn reset_collection_stats(&mut self) {
        self.collection.stats.record = CollectionStats::default();
    }
}
