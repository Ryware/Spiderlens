//! Filter bit layout (plan D3): every (tab, filter) of the results table
//! gets a bit position for this comparison, stored in INTEGER columns of 62
//! bits so the sign bit is never used.

use spiderlens_results::{FilterDef, TabDef, tabs};

pub(crate) const BITS: usize = 62;

pub(crate) struct Slot {
    pub tab: &'static TabDef,
    pub filter: &'static FilterDef,
}

pub(crate) struct Layout {
    pub slots: Vec<Slot>,
}

impl Layout {
    pub fn new() -> Self {
        let slots = tabs()
            .iter()
            .flat_map(|tab| tab.filters.iter().map(move |filter| Slot { tab, filter }))
            .collect();
        Self { slots }
    }

    pub fn columns(&self) -> usize {
        self.slots.len().div_ceil(BITS)
    }

    pub fn find(&self, tab: &str, filter: &str) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| s.tab.id == tab && s.filter.id == filter)
    }

    /// `(column name, bit)` of slot `k`.
    pub fn bit(k: usize) -> (String, usize) {
        (format!("fb{}", k / BITS), k % BITS)
    }

    /// SQL testing slot `k` on side rows aliased `alias`: 1 or 0.
    pub fn test(alias: &str, k: usize) -> String {
        let (col, b) = Self::bit(k);
        format!("(({alias}.{col} >> {b}) & 1)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_filter_has_a_distinct_bit() {
        let l = Layout::new();
        assert!(l.slots.len() > 40);
        assert_eq!(l.columns(), l.slots.len().div_ceil(BITS));
        assert_eq!(
            l.find("page-titles", "missing").map(|k| &*l.slots[k].filter.id),
            Some("missing")
        );
        assert_eq!(Layout::bit(BITS), ("fb1".into(), 0));
    }
}
