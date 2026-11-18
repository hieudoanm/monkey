//! Offset → line/column resolution for source files.

/// Precomputed line-start table for a source file.
///
/// Build once per file, then resolve any byte offset in O(log n).
#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    /// Byte offset of the start of each line (line 0 starts at 0).
    line_starts: Vec<u32>,
}

impl SourceMap {
    pub fn new(src: &str) -> Self {
        let mut line_starts = vec![0u32];
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i as u32 + 1);
            }
        }
        Self { line_starts }
    }

    /// 1-based line number containing `offset` (clamped to EOF).
    pub fn line(&self, offset: u32) -> u32 {
        match self.line_starts.binary_search(&offset) {
            Ok(i) => i as u32 + 1,
            Err(i) => i as u32,
        }
    }

    /// 1-based line and column for `offset`.
    ///
    /// Columns count Unicode scalar values (characters), not bytes, so they
    /// stay correct for non-ASCII source. `offset` is clamped into the file.
    pub fn line_col(&self, offset: u32, src: &str) -> (u32, u32) {
        let line = self.line(offset);
        let line_start = self.line_starts[(line - 1) as usize] as usize;
        let offset = (offset as usize).min(src.len());
        let col = src[line_start..offset].chars().count() as u32 + 1;
        (line, col)
    }

    /// Line start byte offset for a 1-based `line`.
    pub fn line_start(&self, line: u32) -> u32 {
        self.line_starts
            .get((line - 1) as usize)
            .copied()
            .unwrap_or_else(|| *self.line_starts.last().unwrap_or(&0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_positions() {
        let src = "ab\ncd\nef";
        let map = SourceMap::new(src);
        assert_eq!(map.line_col(0, src), (1, 1));
        assert_eq!(map.line_col(2, src), (1, 3));
        assert_eq!(map.line_col(3, src), (2, 1));
        assert_eq!(map.line_col(6, src), (3, 1));
    }

    #[test]
    fn counts_chars_not_bytes() {
        let src = "é\nx";
        let map = SourceMap::new(src);
        // 'é' is 2 bytes; offset 2 is the '\n', offset 3 is 'x'.
        assert_eq!(map.line_col(3, src), (2, 1));
    }
}
