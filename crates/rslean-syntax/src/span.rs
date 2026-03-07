// Placeholder - Span and SourceInfo types
// Will be populated in Wave 2

/// Byte offset in source file
pub type BytePos = u32;

/// A span of source text
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Span {
    /// Start byte offset (inclusive)
    pub start: BytePos,
    /// End byte offset (exclusive)
    pub end: BytePos,
}

impl Span {
    pub fn new(start: BytePos, end: BytePos) -> Self {
        Self { start, end }
    }

    pub fn dummy() -> Self {
        Self { start: 0, end: 0 }
    }

    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Get the source text for this span from the full source
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start as usize..self.end as usize]
    }
}

/// Source information for syntax nodes
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct SourceInfo {
    /// The leading whitespace/comments before this token
    pub leading: Span,
    /// The actual token span
    pub span: Span,
    /// The trailing whitespace/comments after this token
    pub trailing: Span,
}

impl SourceInfo {
    pub fn new(span: Span) -> Self {
        Self {
            leading: Span::dummy(),
            span,
            trailing: Span::dummy(),
        }
    }

    pub fn with_leading(mut self, leading: Span) -> Self {
        self.leading = leading;
        self
    }

    pub fn with_trailing(mut self, trailing: Span) -> Self {
        self.trailing = trailing;
        self
    }

    pub fn dummy() -> Self {
        Self {
            leading: Span::dummy(),
            span: Span::dummy(),
            trailing: Span::dummy(),
        }
    }

    /// Full span including leading/trailing trivia
    pub fn full_span(&self) -> Span {
        if self.leading.is_empty() && self.trailing.is_empty() {
            self.span
        } else {
            let start = if self.leading.is_empty() {
                self.span.start
            } else {
                self.leading.start
            };
            let end = if self.trailing.is_empty() {
                self.span.end
            } else {
                self.trailing.end
            };
            Span::new(start, end)
        }
    }
}
