#![allow(dead_code)]

pub mod extractor;

use crate::model::types::Range;

/// All facts extracted from one Alembic migration file.
#[derive(Clone, Debug)]
pub struct MigrationFile {
    pub revision: Option<String>,
    pub down_revision: DownRevision,
    pub message: Option<String>,
    pub revision_range: Option<Range>,
    pub down_revision_range: Option<Range>,
    pub op_calls: Vec<OpCall>,
}

/// The parent-revision pointer: none, one, or many (merge migration).
#[derive(Clone, Debug)]
pub enum DownRevision {
    None,
    Single(String),
    Multiple(Vec<String>),
}

/// One `op.*` call inside `upgrade()` or `downgrade()`.
#[derive(Clone, Debug)]
pub struct OpCall {
    pub operation: String,
    pub full_range: Range,
    pub table_name: Option<TableRef>,
    pub column_name: Option<ColumnRef>,
}

/// A table name reference inside an op call.
#[derive(Clone, Debug)]
pub struct TableRef {
    pub name: String,
    pub range: Range,
}

/// A column name reference inside an op call.
#[derive(Clone, Debug)]
pub struct ColumnRef {
    pub name: String,
    pub range: Range,
}
