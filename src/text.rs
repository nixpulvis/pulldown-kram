//! Small questions about the lines of a Markdown text, asked both by the
//! rewrites before pulldown-cmark reads it and by the reader after.

/// A text, for finding where its lines start and end.
#[derive(Clone, Copy)]
pub(crate) struct Lines<'t>(pub &'t str);

impl<'t> Lines<'t> {
    /// Where the line that `at` is on starts.
    pub fn start(self, at: usize) -> usize {
        self.0[..at].rfind('\n').map_or(0, |i| i + 1)
    }

    /// Where the line after the one `at` is on starts, or the end of the
    /// text if there's none.
    pub fn next(self, at: usize) -> usize {
        self.0[at..].find('\n').map_or(self.0.len(), |i| at + i + 1)
    }

    /// Where the line before the one starting at `line` starts, if there's
    /// one.
    pub fn previous(self, line: usize) -> Option<usize> {
        (line > 0).then(|| self.start(line - 1))
    }

    /// The text from `at` to the end of its line, without the line break.
    pub fn rest_of_line(self, at: usize) -> &'t str {
        let end = self.0[at..].find('\n').map_or(self.0.len(), |i| at + i);
        &self.0[at..end]
    }
}

/// The quote markers and indentation a line starts with.
pub(crate) struct Lead {
    /// How many bytes they take.
    pub len: usize,
    /// How many spaces come after the last quote marker, or from the start
    /// of the line if there's none.
    pub indent: usize,
    /// Whether there's a quote marker.
    pub quoted: bool,
}

impl Lead {
    /// The spaces and quote markers `line` starts with.
    pub fn of(line: &str) -> Self {
        let len = line.len() - line.trim_start_matches([' ', '>']).len();
        let last_marker = line[..len].rfind('>');
        Self {
            len,
            indent: len - last_marker.map_or(0, |i| i + 1),
            quoted: last_marker.is_some(),
        }
    }
}

/// Whether `text` holds nothing but whitespace and quote markers, as a
/// blank line inside a quote does.
pub(crate) fn is_blank(text: &str) -> bool {
    text.trim_matches(|c: char| c.is_whitespace() || c == '>')
        .is_empty()
}
