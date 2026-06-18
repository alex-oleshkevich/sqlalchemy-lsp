// Types are defined here for all renderers and future feature handlers; none
// are wired up yet, so dead_code is expected until the feature beads land.
#![allow(dead_code)]

use tower_lsp_server::ls_types::{DiagnosticSeverity, DiagnosticTag, Uri};

/// A byte-offset span in a source file (from tree-sitter, not line/col).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

impl ByteRange {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// The stable `SQLA-<SEV><CLASS><NN>` code plus its category name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticCode {
    pub code: String,
    pub category: String,
}

impl DiagnosticCode {
    pub fn new(code: impl Into<String>, category: impl Into<String>) -> Self {
        Self { code: code.into(), category: category.into() }
    }
}

/// The severity of a finding, after any per-code config override.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl Severity {
    pub fn to_lsp(self) -> DiagnosticSeverity {
        match self {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Info => DiagnosticSeverity::INFORMATION,
            Severity::Hint => DiagnosticSeverity::HINT,
        }
    }
}

/// A file URI plus a byte-offset span pinning a finding to its source.
#[derive(Clone, Debug)]
pub struct Location {
    pub uri: Uri,
    pub range: ByteRange,
}

/// Structured detail attached to a finding — each renderer maps these its own way.
#[derive(Clone, Debug)]
pub enum Advice {
    /// A source excerpt with a caret span marking the exact characters.
    CodeFrame { source: ByteRange, caret: ByteRange },
    /// A plain explanatory remark.
    Note(String),
    /// A before/after pair showing the change a fix would make.
    Diff { before: String, after: String },
    /// A recommended next step phrased for the developer.
    Suggestion(String),
}

/// Metadata a renderer or tooling reads alongside severity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticTags {
    /// A quick-fix exists. CLI summary counts these; editor shows the lightbulb.
    pub fixable: bool,
    /// Maps to `lsp_types::DiagnosticTag::DEPRECATED` (struck-through in editor).
    pub deprecated: bool,
    /// Maps to `lsp_types::DiagnosticTag::UNNECESSARY` (greyed-out in editor).
    pub unnecessary: bool,
}

impl DiagnosticTags {
    pub fn is_empty(self) -> bool {
        !self.fixable && !self.deprecated && !self.unnecessary
    }

    /// Convert to the LSP `DiagnosticTag` list the protocol requires.
    pub fn to_lsp(self) -> Vec<DiagnosticTag> {
        let mut tags = Vec::new();
        if self.deprecated {
            tags.push(DiagnosticTag::DEPRECATED);
        }
        if self.unnecessary {
            tags.push(DiagnosticTag::UNNECESSARY);
        }
        tags
    }
}

/// Whether an automatic fix exists and how trustworthy it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FixKind {
    /// Unambiguous correction: no schema or runtime change.
    Safe,
    /// Schema change, behavior change, or requires human judgment.
    Unsafe,
    /// No automatic fix; finding is informational only.
    #[default]
    None,
}

/// A single finding — the unified model every renderer consumes.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub message: String,
    pub location: Location,
    pub advices: Vec<Advice>,
    pub tags: DiagnosticTags,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri() -> Uri {
        "file:///tmp/models.py".parse().unwrap()
    }

    fn code(c: &str) -> DiagnosticCode {
        DiagnosticCode::new(c, "correctness")
    }

    fn loc() -> Location {
        Location { uri: uri(), range: ByteRange::new(10, 25) }
    }

    #[test]
    fn diagnostic_code_new() {
        let c = DiagnosticCode::new("SQLA-E101", "correctness");
        assert_eq!(c.code, "SQLA-E101");
        assert_eq!(c.category, "correctness");
    }

    #[test]
    fn diagnostic_tags_is_empty() {
        assert!(DiagnosticTags::default().is_empty());
        assert!(!DiagnosticTags { fixable: true, ..Default::default() }.is_empty());
        assert!(!DiagnosticTags { deprecated: true, ..Default::default() }.is_empty());
        assert!(!DiagnosticTags { unnecessary: true, ..Default::default() }.is_empty());
    }

    #[test]
    fn diagnostic_tags_to_lsp_mapping() {
        let tags = DiagnosticTags { deprecated: true, unnecessary: true, ..Default::default() };
        let lsp = tags.to_lsp();
        assert!(lsp.contains(&DiagnosticTag::DEPRECATED));
        assert!(lsp.contains(&DiagnosticTag::UNNECESSARY));

        let fixable_only = DiagnosticTags { fixable: true, ..Default::default() };
        assert!(fixable_only.to_lsp().is_empty(), "fixable has no LSP DiagnosticTag");
    }

    #[test]
    fn fix_kind_default_is_none() {
        assert_eq!(FixKind::default(), FixKind::None);
    }

    #[test]
    fn severity_to_lsp() {
        use tower_lsp_server::ls_types::DiagnosticSeverity;
        assert_eq!(Severity::Error.to_lsp(), DiagnosticSeverity::ERROR);
        assert_eq!(Severity::Warning.to_lsp(), DiagnosticSeverity::WARNING);
        assert_eq!(Severity::Info.to_lsp(), DiagnosticSeverity::INFORMATION);
        assert_eq!(Severity::Hint.to_lsp(), DiagnosticSeverity::HINT);
    }

    #[test]
    fn diagnostic_construction() {
        let d = Diagnostic {
            code: code("SQLA-W101"),
            severity: Severity::Warning,
            message: "missing __tablename__".to_string(),
            location: loc(),
            advices: vec![Advice::Note("Add a __tablename__ attribute.".to_string())],
            tags: DiagnosticTags { fixable: true, ..Default::default() },
        };
        assert_eq!(d.severity, Severity::Warning);
        assert!(d.tags.fixable);
        assert_eq!(d.advices.len(), 1);
    }
}
