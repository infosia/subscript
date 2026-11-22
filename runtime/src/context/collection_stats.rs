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
#[repr(C)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CollectionStats {
    /// The automatic collections: at a poll, at an outermost exit, and the
    /// detection collection.
    pub automatic_collections: u64,
    /// The explicit collections: `Context.collect()` and
    /// `subscript_rt_ctx_collect`.
    pub explicit_collections: u64,
    /// The sum of the times of the collections, in nanoseconds.
    pub total_ns: u64,
    /// The largest time of one collection, in nanoseconds.
    pub max_ns: u64,
    /// The time of the last collection, in nanoseconds.
    pub last_ns: u64,
    /// The words that the mark of the last collection tested as a possible
    /// reference, in the roots and in the payloads.
    pub last_words_read: u64,
    /// The largest count of words that the mark of one collection read. It
    /// can come from a collection other than the one of `max_ns`.
    pub max_words_read: u64,
    /// The live bytes after the last collection.
    pub last_live_bytes_after: u64,
}

/// The state of the statistics in the collection state of a Context.
#[derive(Debug, Default)]
pub(super) struct StatsState {
    record: CollectionStats,
    // compiler.md §353 rule 5: the words that the mark tested since the
    // start of the collection that is timed.
    mark_words: u64,
}

/// A timed collection that started (`compiler.md` §353 rule 4).
pub(super) struct StatsStart(std::time::Instant);

impl StatsState {
    /// Starts the time and the word count of one collection.
    pub(super) fn start(&mut self) -> StatsStart {
        self.mark_words = 0;
        StatsStart(std::time::Instant::now())
    }

    /// `compiler.md` §353 rule 5: adds the words that one drain of the mark
    /// tested.
    #[inline]
    pub(super) fn add_mark_words(&mut self, words: usize) {
        self.mark_words = self.mark_words.saturating_add(words as u64);
    }

    /// Records one collection that ended (`compiler.md` §353 rules 1 and 4).
    pub(super) fn finish(&mut self, start: StatsStart, automatic: bool, live_bytes: usize) {
        let ns = u64::try_from(start.0.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let record = &mut self.record;
        if automatic {
            record.automatic_collections = record.automatic_collections.saturating_add(1);
        } else {
            record.explicit_collections = record.explicit_collections.saturating_add(1);
        }
        record.total_ns = record.total_ns.saturating_add(ns);
        record.max_ns = record.max_ns.max(ns);
        record.last_ns = ns;
        record.last_words_read = self.mark_words;
        record.max_words_read = record.max_words_read.max(self.mark_words);
        record.last_live_bytes_after = live_bytes as u64;
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
