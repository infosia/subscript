//! The position table each tier builds for its trap records
//! (`specs/blocks/compiler.md` §112).
//!
//! A trap record carries a position id, and each tier resolves that id
//! through its own table. Id 0 means that no script site exists, so
//! every table holds one reserved entry at index 0 and the ids of
//! script sites start at 1. The reservation is a fact of the table
//! (§112 rule 2): [`PositionTable::new`] is the one constructor, and it
//! places the reserved entry, so no code and no test can build a table
//! without it.

use subscript_compiler::Pos;

/// The reserved entry of every position table (§112 rule 1): an empty
/// file name, line 0, column 0.
///
/// A host reads the empty file name as "the runtime holds no script
/// site for this record".
#[must_use]
pub(crate) fn no_script_site() -> Pos {
    Pos::new(String::new(), 0, 0)
}

/// One tier's map from a recorded position id to a script position.
///
/// Index 0 is the reserved entry of §112 rule 1. A lowering adds each
/// script site with [`PositionTable::add`], which answers the id the
/// generated code records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionTable {
    /// Index 0 is the reserved entry; every later index is a script
    /// site, in the order the lowering reached it.
    entries: Vec<Pos>,
}

impl Default for PositionTable {
    fn default() -> Self {
        Self::new()
    }
}

impl PositionTable {
    /// A new table: the reserved entry at index 0, and nothing else.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: vec![no_script_site()],
        }
    }

    /// Adds one script site and answers the id that names it.
    ///
    /// The first id this answers is 1, because index 0 is reserved.
    pub fn add(&mut self, pos: &Pos) -> u32 {
        self.entries.push(pos.clone());
        (self.entries.len() - 1) as u32
    }

    /// The position of one id, or `None` when the table has no such
    /// entry.
    #[must_use]
    pub fn get(&self, pos_id: u32) -> Option<&Pos> {
        self.entries.get(pos_id as usize)
    }

    /// The position one recorded trap carries in a report.
    ///
    /// Id 0 answers the reserved entry, so a record with no script site
    /// answers the empty position on every tier. An id the table does
    /// not hold answers that same entry.
    #[must_use]
    pub fn report_position(&self, pos_id: u32) -> Pos {
        self.get(pos_id).cloned().unwrap_or_else(no_script_site)
    }

    /// Every entry in id order, for a consumer that renders the whole
    /// table.
    #[must_use]
    pub fn entries(&self) -> &[Pos] {
        &self.entries
    }

    /// The number of entries, the reserved one included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table holds nothing but the reserved entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.len() <= 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §112 rules 1 and 3: index 0 of a table is the reserved entry,
    /// the first script site takes id 1, and every id the table does
    /// not hold answers the reserved entry.
    ///
    /// Cost: under 1 ms. The test builds one table and reads it.
    #[test]
    fn position_id_zero_is_the_reserved_entry_of_every_table() {
        let empty = Pos::new(String::new(), 0, 0);
        let mut table = PositionTable::new();
        assert_eq!(
            table.entries(),
            std::slice::from_ref(&empty),
            "§112 rule 1 reserves id 0"
        );
        assert!(table.is_empty(), "a new table holds no script site");

        let first = table.add(&Pos::new("first.ts", 1, 2));
        let second = table.add(&Pos::new("second.ts", 3, 4));
        assert_eq!(
            (first, second),
            (1, 2),
            "§112 rule 1 starts the ids of script sites at 1"
        );
        assert_eq!(table.len(), 3);
        assert!(!table.is_empty());

        assert_eq!(table.report_position(0), empty);
        // The firing control: a script-site id answers its own
        // position, so the reserved entry is the answer to id 0 alone.
        assert_eq!(
            table.report_position(first),
            Pos::new("first.ts", 1, 2),
            "an id the table holds answers its own site"
        );
        assert_eq!(table.report_position(second), Pos::new("second.ts", 3, 4));
        // §112 rule 3: an id outside the table answers the same entry.
        assert_eq!(table.report_position(9), empty);
        assert_eq!(table.get(9), None);
    }

    /// §112 rule 2: the reservation is a fact of the table, so the
    /// default table is the one the constructor builds, and a lowering
    /// that takes its table out leaves a table with the reserved entry.
    ///
    /// Cost: under 1 ms.
    #[test]
    fn the_default_table_holds_the_reserved_entry() {
        assert_eq!(PositionTable::default(), PositionTable::new());

        let mut table = PositionTable::new();
        table.add(&Pos::new("first.ts", 1, 2));
        let taken = std::mem::take(&mut table);
        assert_eq!(taken.len(), 2, "the taken table keeps both entries");
        assert_eq!(
            table.entries(),
            [no_script_site()],
            "what a take leaves behind still reserves id 0"
        );
    }
}
