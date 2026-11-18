//! Byte-offset source spans shared by all Monkey crates.

/// A half-open byte range `[start, end)` into a source file.
///
/// Spans are cheap `Copy` values; they deliberately carry no line/column
/// information. Use [`crate::SourceMap`] to resolve offsets to positions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Span {
    /// Byte offset of the first byte.
    pub start: u32,
    /// Byte offset one past the last byte.
    pub end: u32,
}

impl Span {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Length in bytes.
    pub fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(self) -> bool {
        self.start >= self.end
    }

    /// Span covering `self` extended to include `other`.
    pub fn join(self, other: Span) -> Span {
        Span::new(self.start.min(other.start), self.end.max(other.end))
    }

    /// Span from `start` up to (but not including) `end`.
    pub fn between(start: u32, end: u32) -> Span {
        Span::new(start, end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_basics() {
        let s = Span::new(2, 5);
        assert_eq!(s.len(), 3);
        assert!(!s.is_empty());
        assert!(Span::new(5, 5).is_empty());
        assert_eq!(s.join(Span::new(0, 9)), Span::new(0, 9));
    }
}
