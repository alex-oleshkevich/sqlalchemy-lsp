#![allow(dead_code)] // indicators and helpers called by server wiring not yet landed

use crate::model::types::Range;

/// Returns `true` if the source is likely a SQLAlchemy model file.
/// Deliberately broad — a false positive means one extra parse; a false
/// negative means a model goes un-indexed.
pub fn has_sqlalchemy_indicators(source: &str) -> bool {
    source.contains("from sqlalchemy")
        || source.contains("import sqlalchemy")
        || source.contains("Mapped[")
        || source.contains("mapped_column")
        || source.contains("DeclarativeBase")
}

/// Returns `true` if the source is likely an Alembic migration file.
pub fn has_alembic_indicators(source: &str) -> bool {
    source.contains("from alembic") || source.contains("import alembic")
}

/// Extract the UTF-8 text of a tree-sitter node from the original source bytes.
pub fn node_text<'a>(node: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).unwrap_or("")
}

/// Convert a tree-sitter node's span to our line/col `Range`.
pub fn ts_range(node: tree_sitter::Node) -> Range {
    Range {
        start_line: node.start_position().row as u32,
        start_col: node.start_position().column as u32,
        end_line: node.end_position().row as u32,
        end_col: node.end_position().column as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sa_indicators_detect_common_patterns() {
        assert!(has_sqlalchemy_indicators("from sqlalchemy import String"));
        assert!(has_sqlalchemy_indicators("import sqlalchemy as sa"));
        assert!(has_sqlalchemy_indicators("id: Mapped[int]"));
        assert!(has_sqlalchemy_indicators("mapped_column(Integer)"));
        assert!(has_sqlalchemy_indicators("class Base(DeclarativeBase): pass"));
        assert!(!has_sqlalchemy_indicators("x = 1\nprint(x)"));
    }

    #[test]
    fn alembic_indicators_detect_imports() {
        assert!(has_alembic_indicators("from alembic import op"));
        assert!(has_alembic_indicators("import alembic"));
        assert!(!has_alembic_indicators("# just a comment"));
    }
}
