//! Text edit types for code fixes.

use crate::OffsetRange;

/// A text edit representing a change to apply to source code.
///
/// Text edits use byte offsets and are converted to line/column ranges
/// when presenting to users or LSP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// Byte offset range to replace
    pub offset_range: OffsetRange,
    /// The text to replace the range with (empty string means deletion)
    pub new_text: String,
}

impl TextEdit {
    /// Create a new text edit that replaces a range with new text.
    #[must_use]
    pub fn new(start: usize, end: usize, new_text: impl Into<String>) -> Self {
        Self {
            offset_range: OffsetRange::new(start, end),
            new_text: new_text.into(),
        }
    }

    /// Create a deletion edit (replace range with empty string).
    #[must_use]
    pub fn delete(start: usize, end: usize) -> Self {
        Self {
            offset_range: OffsetRange::new(start, end),
            new_text: String::new(),
        }
    }

    /// Create an insertion edit (insert text at position without removing anything).
    #[must_use]
    pub fn insert(position: usize, text: impl Into<String>) -> Self {
        Self {
            offset_range: OffsetRange::at(position),
            new_text: text.into(),
        }
    }

    /// Returns `true` if this edit is a deletion (empty `new_text`).
    #[must_use]
    pub fn is_deletion(&self) -> bool {
        self.new_text.is_empty() && !self.offset_range.is_empty()
    }

    /// Returns `true` if this edit is an insertion (zero-width range).
    #[must_use]
    pub fn is_insertion(&self) -> bool {
        self.offset_range.is_empty() && !self.new_text.is_empty()
    }
}

/// A code fix that can be applied to resolve a diagnostic.
///
/// Code fixes have a human-readable label and one or more text edits
/// that should be applied together atomically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeFix {
    /// Human-readable description of what the fix does
    pub label: String,
    /// The text edits to apply (in order)
    pub edits: Vec<TextEdit>,
}

impl CodeFix {
    /// Create a new code fix with a label and edits.
    #[must_use]
    pub fn new(label: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self {
            label: label.into(),
            edits,
        }
    }

    /// Create a simple deletion fix.
    #[must_use]
    pub fn delete(label: impl Into<String>, start: usize, end: usize) -> Self {
        Self {
            label: label.into(),
            edits: vec![TextEdit::delete(start, end)],
        }
    }

    /// Create a simple replacement fix.
    #[must_use]
    pub fn replace(
        label: impl Into<String>,
        start: usize,
        end: usize,
        new_text: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            edits: vec![TextEdit::new(start, end, new_text)],
        }
    }

    /// Create a simple insertion fix.
    #[must_use]
    pub fn insert(label: impl Into<String>, position: usize, text: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            edits: vec![TextEdit::insert(position, text)],
        }
    }

    /// Returns `true` if this fix has any edits.
    #[must_use]
    pub fn has_edits(&self) -> bool {
        !self.edits.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_edit_creation() {
        let edit = TextEdit::new(10, 20, "replacement");
        assert_eq!(edit.offset_range.start, 10);
        assert_eq!(edit.offset_range.end, 20);
        assert_eq!(edit.new_text, "replacement");
        assert!(!edit.is_deletion());
        assert!(!edit.is_insertion());
    }

    #[test]
    fn test_text_edit_delete() {
        let edit = TextEdit::delete(5, 15);
        assert_eq!(edit.offset_range.start, 5);
        assert_eq!(edit.offset_range.end, 15);
        assert_eq!(edit.new_text, "");
        assert!(edit.is_deletion());
        assert!(!edit.is_insertion());
    }

    #[test]
    fn test_text_edit_insert() {
        let edit = TextEdit::insert(10, "inserted text");
        assert_eq!(edit.offset_range.start, 10);
        assert_eq!(edit.offset_range.end, 10);
        assert_eq!(edit.new_text, "inserted text");
        assert!(!edit.is_deletion());
        assert!(edit.is_insertion());
    }

    #[test]
    fn test_code_fix_creation() {
        let fix = CodeFix::new("Remove unused variable", vec![TextEdit::delete(10, 20)]);
        assert_eq!(fix.label, "Remove unused variable");
        assert_eq!(fix.edits.len(), 1);
        assert!(fix.has_edits());
    }

    #[test]
    fn test_code_fix_delete() {
        let fix = CodeFix::delete("Remove variable", 10, 20);
        assert_eq!(fix.label, "Remove variable");
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].new_text, "");
    }

    #[test]
    fn test_code_fix_replace() {
        let fix = CodeFix::replace("Replace with new", 10, 20, "new");
        assert_eq!(fix.label, "Replace with new");
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].new_text, "new");
    }

    #[test]
    fn test_code_fix_insert() {
        let fix = CodeFix::insert("Add import", 0, "import { foo } from 'bar';\n");
        assert_eq!(fix.label, "Add import");
        assert_eq!(fix.edits.len(), 1);
        assert!(fix.edits[0].is_insertion());
    }

    #[test]
    fn test_code_fix_no_edits() {
        let fix = CodeFix::new("Empty fix", vec![]);
        assert!(!fix.has_edits());
    }
}
