#![allow(dead_code)]

use std::{collections::HashMap, fmt};

use tower_lsp_server::ls_types::Uri;

/// The severity of a diagnostic finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// Whether an automatic fix exists and how trustworthy it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FixKind {
    Safe,
    Unsafe,
    #[default]
    None,
}

/// Metadata flags a renderer or tooling reads alongside severity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticTags {
    pub fixable: bool,
    pub deprecated: bool,
    pub unnecessary: bool,
}

impl DiagnosticTags {
    pub fn is_empty(self) -> bool {
        !self.fixable && !self.deprecated && !self.unnecessary
    }
}

/// A half-open source span, zero-based, end columns exclusive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Range {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

/// A mapped ORM class with its columns, relationships, and table metadata.
#[derive(Clone, Debug)]
pub struct Model {
    pub name: String,
    pub table_name: Option<String>,
    pub bases: Vec<String>,
    pub columns: HashMap<String, Column>,
    pub relationships: HashMap<String, Relationship>,
    pub table_args: Vec<TableArg>,
    pub duplicate_columns: Vec<(String, Range)>,
    pub docstring: Option<String>,
    pub name_range: Range,
    pub full_range: Range,
}

/// A `mapped_column(...)` attribute: its type, args, and optional foreign key.
#[derive(Clone, Debug)]
pub struct Column {
    pub name: String,
    pub key: Option<String>,
    pub mapped_type: MappedType,
    pub args: ColumnArgs,
    pub foreign_key: Option<ForeignKeyRef>,
    pub doc: Option<String>,
    pub name_range: Range,
    pub full_range: Range,
}

/// Boolean flags and the default expression from `mapped_column(...)`.
#[derive(Clone, Debug)]
pub struct ColumnArgs {
    pub primary_key: bool,
    pub nullable: bool,
    pub unique: bool,
    pub index: bool,
    pub default: Option<String>,
    /// Source text of the `server_default=` argument, if present.
    pub server_default: Option<String>,
}

impl Default for ColumnArgs {
    fn default() -> Self {
        Self {
            primary_key: false,
            nullable: true,
            unique: false,
            index: false,
            default: None,
            server_default: None,
        }
    }
}

/// A `ForeignKey("table.col")` reference split into its halves.
#[derive(Clone, Debug)]
pub struct ForeignKeyRef {
    pub table: String,
    pub column: String,
    pub raw_text: String,
    pub range: Range,
}

/// A `relationship(...)` attribute wiring two models together.
#[derive(Clone, Debug)]
pub struct Relationship {
    pub name: String,
    pub target_model: String,
    pub explicit_target: Option<String>,
    pub back_populates: Option<String>,
    pub lazy: Option<String>,
    pub uselist: Option<bool>,
    pub secondary: Option<String>,
    pub cascade: Option<String>,
    pub is_list: bool,
    /// Source text of the `backref=` argument (legacy; prefer `back_populates`).
    pub backref: Option<String>,
    /// True when `remote_side=` is present (needed for self-referential relationships).
    pub remote_side: bool,
    /// True when `foreign_keys=` is present (disambiguates multi-FK relationships).
    pub has_foreign_keys: bool,
    /// True when `viewonly=True` is set.
    pub viewonly: Option<bool>,
    pub name_range: Range,
    pub full_range: Range,
    pub target_range: Option<Range>,
    pub back_populates_range: Option<Range>,
    pub cascade_range: Option<Range>,
}

/// One entry in `__table_args__`: an Index, UniqueConstraint, or PrimaryKeyConstraint.
#[derive(Clone, Debug)]
pub struct TableArg {
    pub kind: String,
    pub columns: Vec<String>,
    pub column_ranges: Vec<Range>,
    pub full_range: Range,
    /// The `name=` argument value, if given.
    pub name: Option<String>,
}

/// The type a `Mapped[...]` annotation resolves to.
#[derive(Clone, Debug)]
pub enum MappedType {
    Int,
    Str,
    Float,
    Bool,
    DateTime,
    Optional(Box<MappedType>),
    List(String),
    ForwardRef(String),
    SqlType { name: String, args: Vec<String> },
    Unknown(String),
}

impl fmt::Display for MappedType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MappedType::Int => write!(f, "int"),
            MappedType::Str => write!(f, "str"),
            MappedType::Float => write!(f, "float"),
            MappedType::Bool => write!(f, "bool"),
            MappedType::DateTime => write!(f, "datetime"),
            MappedType::Optional(inner) => write!(f, "Optional[{inner}]"),
            MappedType::List(model) => write!(f, "List[{model}]"),
            MappedType::ForwardRef(name) => write!(f, "\"{name}\""),
            MappedType::SqlType { name, args } => {
                if args.is_empty() {
                    write!(f, "{name}")
                } else {
                    write!(f, "{name}({})", args.join(", "))
                }
            }
            MappedType::Unknown(s) => write!(f, "{s}"),
        }
    }
}

/// Where a model lives in the workspace index.
#[derive(Clone, Debug)]
pub struct ModelLocation {
    pub uri: Uri,
    pub model_name: String,
    pub range: Range,
}
