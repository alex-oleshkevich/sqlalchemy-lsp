#![allow(dead_code)]

use std::collections::HashMap;

use tree_sitter::{Node, Tree};

use super::python::{node_text, ts_range};
use crate::model::types::{
    Column, ColumnArgs, ForeignKeyRef, MappedType, Model, Range, Relationship, TableArg,
};

// ── Symbol table ─────────────────────────────────────────────────────────────

/// Where a local name was imported from.
#[derive(Clone, Debug)]
pub struct Binding {
    pub module: String,
    pub name: String,
}

/// Per-file import/alias table built before the class walk.
/// REQ-EXTRACT-07: resolves every alias to its canonical (module, name).
#[derive(Default, Debug)]
pub struct SymbolTable {
    /// local_name → canonical binding (from `import X as Y` or `from M import X as Y`)
    pub bindings: HashMap<String, Binding>,
    /// local_name → module path (from `import M as X` or `import M.sub as X`)
    pub module_aliases: HashMap<String, String>,
    /// modules from `from M import *`
    pub star_imports: Vec<String>,
}

impl SymbolTable {
    /// Resolve a local name or dotted path to `"module.name"`, or `None` if unknown.
    pub fn resolve(&self, name: &str) -> Option<String> {
        if let Some(b) = self.bindings.get(name) {
            return Some(format!("{}.{}", b.module, b.name));
        }
        if let Some(dot) = name.find('.') {
            let prefix = &name[..dot];
            let rest = &name[dot + 1..];
            if let Some(module_path) = self.module_aliases.get(prefix) {
                return Some(format!("{}.{}", module_path, rest));
            }
        }
        for m in &self.star_imports {
            if is_known_sa_export(m, name) {
                return Some(format!("{}.{}", m, name));
            }
        }
        None
    }

    pub fn is_relationship(&self, local: &str) -> bool {
        matches!(
            self.resolve(local).as_deref(),
            Some(
                "sqlalchemy.orm.relationship"
                    | "sqlalchemy.orm.relation"
                    | "sqlalchemy.relationship"
            )
        )
    }

    pub fn is_mapped_column(&self, local: &str) -> bool {
        matches!(
            self.resolve(local).as_deref(),
            Some("sqlalchemy.orm.mapped_column" | "sqlalchemy.mapped_column")
        )
    }

    pub fn is_foreign_key(&self, local: &str) -> bool {
        matches!(
            self.resolve(local).as_deref(),
            Some(
                "sqlalchemy.ForeignKey"
                    | "sqlalchemy.orm.ForeignKey"
                    | "sqlalchemy.sql.schema.ForeignKey"
            )
        )
    }

    pub fn is_sa_abstract_base(&self, local: &str) -> bool {
        matches!(
            self.resolve(local).as_deref(),
            Some(
                "sqlalchemy.orm.DeclarativeBase"
                    | "sqlalchemy.orm.DeclarativeBaseNoMeta"
                    | "sqlalchemy.orm.MappedAsDataclass"
                    | "sqlalchemy.ext.declarative.DeclarativeBase"
            )
        )
    }

    pub fn is_mapped(&self, local: &str) -> bool {
        matches!(
            self.resolve(local).as_deref(),
            Some("sqlalchemy.orm.Mapped" | "sqlalchemy.Mapped")
        )
    }

    pub fn module_path(&self, alias: &str) -> Option<&str> {
        self.module_aliases.get(alias).map(String::as_str)
    }
}

fn is_known_sa_export(module: &str, name: &str) -> bool {
    match module {
        "sqlalchemy" | "sqlalchemy.sql" => matches!(
            name,
            "String"
                | "Integer"
                | "Float"
                | "Boolean"
                | "DateTime"
                | "Text"
                | "BigInteger"
                | "SmallInteger"
                | "Numeric"
                | "Date"
                | "Time"
                | "LargeBinary"
                | "JSON"
                | "ARRAY"
                | "UUID"
                | "ForeignKey"
                | "Column"
                | "Table"
                | "MetaData"
        ),
        "sqlalchemy.orm" => matches!(
            name,
            "relationship"
                | "mapped_column"
                | "Mapped"
                | "DeclarativeBase"
                | "DeclarativeBaseNoMeta"
                | "MappedAsDataclass"
                | "registry"
                | "Session"
        ),
        "typing" => matches!(name, "Optional" | "List" | "Union" | "Annotated" | "Any"),
        _ => false,
    }
}

// ── Symbol table construction ─────────────────────────────────────────────────

pub fn build_symbol_table(root: Node, source: &[u8]) -> SymbolTable {
    let mut st = SymbolTable::default();
    walk_for_imports(root, source, &mut st);
    st
}

fn walk_for_imports(node: Node, source: &[u8], st: &mut SymbolTable) {
    match node.kind() {
        "import_statement" => handle_import(node, source, st),
        "import_from_statement" => handle_from_import(node, source, st),
        // Recurse into containers (covers TYPE_CHECKING blocks)
        "if_statement" | "block" | "module" => {
            let mut c = node.walk();
            for child in node.named_children(&mut c) {
                walk_for_imports(child, source, st);
            }
        }
        _ => {}
    }
}

/// In tree-sitter-python 0.25, import_statement exposes names via `name` field (multiple).
fn handle_import(node: Node, source: &[u8], st: &mut SymbolTable) {
    let mut c = node.walk();
    for child in node.children_by_field_name("name", &mut c) {
        match child.kind() {
            "dotted_name" => {
                let name = node_text(child, source);
                st.module_aliases.insert(name.to_string(), name.to_string());
                if let Some(dot) = name.find('.') {
                    st.module_aliases
                        .entry(name[..dot].to_string())
                        .or_insert_with(|| name[..dot].to_string());
                }
            }
            "aliased_import" => {
                if let (Some(n), Some(a)) = (
                    child.child_by_field_name("name"),
                    child.child_by_field_name("alias"),
                ) {
                    st.module_aliases.insert(
                        node_text(a, source).to_string(),
                        node_text(n, source).to_string(),
                    );
                }
            }
            _ => {}
        }
    }
}

/// In tree-sitter-python 0.25, import_from_statement exposes:
///   - module_name field (single)
///   - name field (multiple): dotted_name or aliased_import
///   - wildcard_import as a non-field named child
fn handle_from_import(node: Node, source: &[u8], st: &mut SymbolTable) {
    let module = match node.child_by_field_name("module_name") {
        Some(mn) => node_text(mn, source).to_string(),
        None => return,
    };

    // Check for wildcard: it appears as a non-field named child
    {
        let mut wc = node.walk();
        for child in node.named_children(&mut wc) {
            if child.kind() == "wildcard_import" {
                st.star_imports.push(module.clone());
                return;
            }
        }
    }

    let mut c = node.walk();
    for child in node.children_by_field_name("name", &mut c) {
        match child.kind() {
            "aliased_import" => {
                if let (Some(n), Some(a)) = (
                    child.child_by_field_name("name"),
                    child.child_by_field_name("alias"),
                ) {
                    let orig = node_text(n, source).to_string();
                    let local = node_text(a, source).to_string();
                    st.bindings.insert(
                        local,
                        Binding {
                            module: module.clone(),
                            name: orig,
                        },
                    );
                }
            }
            "dotted_name" => {
                let name = node_text(child, source).to_string();
                st.bindings.insert(
                    name.clone(),
                    Binding {
                        module: module.clone(),
                        name,
                    },
                );
            }
            _ => {}
        }
    }
}

// ── Base class registry ───────────────────────────────────────────────────────

#[derive(Default)]
pub struct BaseRegistry {
    pub base_names: Vec<String>,
}

impl BaseRegistry {
    pub fn register(&mut self, name: &str) {
        if !self.base_names.contains(&name.to_string()) {
            self.base_names.push(name.to_string());
        }
    }

    pub fn is_base(&self, class_name: &str) -> bool {
        self.base_names.iter().any(|b| b == class_name)
    }
}

// ── Main extraction entry point ───────────────────────────────────────────────

pub fn extract_models(source: &str, tree: &Tree) -> Vec<Model> {
    let bytes = source.as_bytes();
    let root = tree.root_node();

    let sym = build_symbol_table(root, bytes);

    // First pass: collect declarative bases defined in this file
    let mut bases = BaseRegistry::default();
    let mut c = root.walk();
    for node in root.named_children(&mut c) {
        if node.kind() == "class_definition" {
            let name = node
                .child_by_field_name("name")
                .map(|n| node_text(n, bytes))
                .unwrap_or("");
            if class_is_declarative_base(&node, bytes, &sym) {
                bases.register(name);
            }
        }
    }

    // Second pass: extract ORM models
    let mut models = Vec::new();
    let mut c = root.walk();
    for node in root.named_children(&mut c) {
        if node.kind() == "class_definition" {
            if let Some(model) = try_extract_model(&node, bytes, &sym, &bases) {
                models.push(model);
            }
        }
    }
    models
}

// ── Class classification ──────────────────────────────────────────────────────

/// A class is a declarative base only if it directly extends an SA abstract
/// base (`DeclarativeBase` etc.). User-defined intermediate bases like `class
/// Base(DeclarativeBase)` are bases; `class User(Base)` is a model, not a base.
fn class_is_declarative_base(class_node: &Node, source: &[u8], sym: &SymbolTable) -> bool {
    let Some(args) = class_node.child_by_field_name("superclasses") else {
        return false;
    };
    let mut c = args.walk();
    for base in args.named_children(&mut c) {
        if sym.is_sa_abstract_base(node_text(base, source)) {
            return true;
        }
    }
    false
}

fn class_is_model(class_node: &Node, source: &[u8], bases: &BaseRegistry) -> bool {
    if class_body_has_tablename(class_node, source) {
        return true;
    }
    let Some(args) = class_node.child_by_field_name("superclasses") else {
        return false;
    };
    let mut c = args.walk();
    for base_node in args.named_children(&mut c) {
        if bases.is_base(node_text(base_node, source)) {
            return true;
        }
    }
    false
}

fn class_body_has_tablename(class_node: &Node, source: &[u8]) -> bool {
    let Some(body) = class_node.child_by_field_name("body") else {
        return false;
    };
    let mut c = body.walk();
    for outer in body.named_children(&mut c) {
        let stmt = unwrap_expr_stmt(outer);
        if stmt.kind() == "assignment" {
            if let Some(left) = stmt.child_by_field_name("left") {
                if node_text(left, source) == "__tablename__" {
                    return true;
                }
            }
        }
    }
    false
}

// ── Model extraction ──────────────────────────────────────────────────────────

fn try_extract_model(
    class_node: &Node,
    source: &[u8],
    sym: &SymbolTable,
    bases: &BaseRegistry,
) -> Option<Model> {
    let name_node = class_node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();

    if class_is_declarative_base(class_node, source, sym) {
        return None;
    }
    if !class_is_model(class_node, source, bases) {
        return None;
    }

    let name_range = ts_range(name_node);
    let full_range = ts_range(*class_node);
    let bases_list = extract_base_names(class_node, source);

    let Some(body) = class_node.child_by_field_name("body") else {
        return Some(Model {
            name,
            table_name: None,
            bases: bases_list,
            columns: HashMap::new(),
            relationships: HashMap::new(),
            table_args: Vec::new(),
            duplicate_columns: Vec::new(),
            docstring: None,
            name_range,
            full_range,
        });
    };

    let mut table_name: Option<String> = None;
    let mut table_args: Vec<TableArg> = Vec::new();
    let mut columns: HashMap<String, Column> = HashMap::new();
    let mut relationships: HashMap<String, Relationship> = HashMap::new();
    let mut duplicate_columns: Vec<(String, Range)> = Vec::new();
    let mut docstring: Option<String> = None;
    let mut first_stmt = true;

    let mut c = body.walk();
    for outer in body.named_children(&mut c) {
        // In tree-sitter-python 0.25, all expressions (including assignments)
        // are wrapped in expression_statement. Unwrap to get the real node.
        let stmt = unwrap_expr_stmt(outer);

        if first_stmt && stmt.kind() == "string" {
            let text = strip_string_quotes(node_text(stmt, source)).to_string();
            if !text.is_empty() {
                docstring = Some(text);
            }
            first_stmt = false;
            continue;
        }
        first_stmt = false;

        if stmt.kind() != "assignment" {
            continue;
        }
        if stmt.child_by_field_name("type").is_some() {
            // Annotated assignment: column or relationship
            if let Some(rel) = try_extract_relationship(&stmt, source, sym) {
                relationships.insert(rel.name.clone(), rel);
            } else if let Some(col) = try_extract_column(&stmt, source, sym) {
                if let Some(old) = columns.insert(col.name.clone(), col.clone()) {
                    duplicate_columns.push((old.name, old.name_range));
                }
            }
        } else {
            // Plain assignment: __tablename__ / __table_args__
            if let Some(tn) = try_extract_tablename(&stmt, source) {
                table_name = Some(tn);
            } else if let Some(args) = try_extract_table_args(&stmt, source) {
                table_args = args;
            }
        }
    }

    Some(Model {
        name,
        table_name,
        bases: bases_list,
        columns,
        relationships,
        table_args,
        duplicate_columns,
        docstring,
        name_range,
        full_range,
    })
}

/// Unwrap an `expression_statement` node to its inner expression.
/// In tree-sitter-python 0.25, assignments and other expressions are always
/// wrapped in an `expression_statement` container node.
fn unwrap_expr_stmt(node: Node) -> Node {
    if node.kind() == "expression_statement" {
        node.named_child(0).unwrap_or(node)
    } else {
        node
    }
}

fn extract_base_names(class_node: &Node, source: &[u8]) -> Vec<String> {
    let Some(args) = class_node.child_by_field_name("superclasses") else {
        return vec![];
    };
    let mut c = args.walk();
    args.named_children(&mut c)
        .map(|n| node_text(n, source).to_string())
        .collect()
}

fn extract_docstring(stmt: &Node, source: &[u8]) -> Option<String> {
    let expr = stmt.named_child(0)?;
    if expr.kind() == "string" {
        return Some(strip_string_quotes(node_text(expr, source)).to_string());
    }
    None
}

fn strip_string_quotes(s: &str) -> &str {
    for delim in &["\"\"\"", "'''"] {
        if s.starts_with(delim) && s.ends_with(delim) {
            return s[delim.len()..s.len() - delim.len()].trim();
        }
    }
    s.trim_matches('"').trim_matches('\'')
}

// ── __tablename__ and __table_args__ ─────────────────────────────────────────

fn try_extract_tablename(assign: &Node, source: &[u8]) -> Option<String> {
    let lhs = assign.child_by_field_name("left")?;
    if node_text(lhs, source) != "__tablename__" {
        return None;
    }
    let rhs = assign.child_by_field_name("right")?;
    Some(strip_string_quotes(node_text(rhs, source)).to_string())
}

fn try_extract_table_args(assign: &Node, source: &[u8]) -> Option<Vec<TableArg>> {
    let lhs = assign.child_by_field_name("left")?;
    if node_text(lhs, source) != "__table_args__" {
        return None;
    }
    let rhs = assign.child_by_field_name("right")?;
    Some(parse_table_args(&rhs, source))
}

fn parse_table_args(node: &Node, source: &[u8]) -> Vec<TableArg> {
    let mut args = Vec::new();
    match node.kind() {
        "tuple" => {
            let mut c = node.walk();
            for item in node.named_children(&mut c) {
                if item.kind() == "call" {
                    if let Some(arg) = parse_constraint_call(&item, source) {
                        args.push(arg);
                    }
                }
            }
        }
        "call" => {
            if let Some(arg) = parse_constraint_call(node, source) {
                args.push(arg);
            }
        }
        _ => {}
    }
    args
}

fn parse_constraint_call(call: &Node, source: &[u8]) -> Option<TableArg> {
    let func = call.child_by_field_name("function")?;
    let kind = node_text(func, source);
    if !matches!(
        kind,
        "UniqueConstraint" | "Index" | "PrimaryKeyConstraint" | "CheckConstraint"
    ) {
        return None;
    }
    let full_range = ts_range(*call);
    let args_node = call.child_by_field_name("arguments")?;
    let mut column_names = Vec::new();
    let mut column_ranges = Vec::new();
    let mut name: Option<String> = None;
    let mut c = args_node.walk();
    for arg in args_node.named_children(&mut c) {
        if arg.kind() == "string" {
            let col = strip_string_quotes(node_text(arg, source)).to_string();
            column_ranges.push(ts_range(arg));
            column_names.push(col);
        } else if arg.kind() == "keyword_argument" {
            let key = arg
                .child_by_field_name("name")
                .map(|n| node_text(n, source))
                .unwrap_or("");
            if key == "name" {
                let val = arg
                    .child_by_field_name("value")
                    .map(|n| node_text(n, source))
                    .unwrap_or("");
                name = Some(strip_string_quotes(val).to_string());
            }
        }
    }
    Some(TableArg {
        kind: kind.to_string(),
        columns: column_names,
        column_ranges,
        full_range,
        name,
    })
}

// ── Relationship extraction ───────────────────────────────────────────────────

fn try_extract_relationship(
    assign: &Node,
    source: &[u8],
    sym: &SymbolTable,
) -> Option<Relationship> {
    let name_node = assign.child_by_field_name("left")?;
    let name = node_text(name_node, source).to_string();
    let name_range = ts_range(name_node);

    let rhs = assign.child_by_field_name("right")?;
    if rhs.kind() != "call" {
        return None;
    }
    let func = rhs.child_by_field_name("function")?;
    let func_name = node_text(func, source);

    if !sym.is_relationship(func_name) && func_name != "relationship" {
        return None;
    }

    let annotation = assign.child_by_field_name("type")?;
    let ann_text = node_text(annotation, source);
    let mapped_type = parse_mapped_type(ann_text, sym);
    let is_list = matches!(&mapped_type, MappedType::List(_));

    let args_node = rhs.child_by_field_name("arguments")?;
    let RelArgs {
        target,
        explicit_target,
        target_range,
        back_populates,
        back_populates_range,
        lazy,
        uselist,
        secondary,
        cascade,
        cascade_range,
        backref,
        remote_side,
        has_foreign_keys,
        viewonly,
    } = parse_relationship_args(&args_node, source);

    let target_model = target
        .or_else(|| extract_model_from_mapped_type(&mapped_type))
        .unwrap_or_default();

    Some(Relationship {
        name,
        target_model,
        explicit_target,
        back_populates,
        lazy,
        uselist,
        secondary,
        cascade,
        is_list,
        backref,
        remote_side,
        has_foreign_keys,
        viewonly,
        name_range,
        full_range: ts_range(*assign),
        target_range,
        back_populates_range,
        cascade_range,
    })
}

fn extract_model_from_mapped_type(mt: &MappedType) -> Option<String> {
    match mt {
        MappedType::List(name) | MappedType::ForwardRef(name) => Some(name.clone()),
        MappedType::Optional(inner) => extract_model_from_mapped_type(inner),
        _ => None,
    }
}

struct RelArgs {
    target: Option<String>,
    explicit_target: Option<String>,
    target_range: Option<Range>,
    back_populates: Option<String>,
    back_populates_range: Option<Range>,
    lazy: Option<String>,
    uselist: Option<bool>,
    secondary: Option<String>,
    cascade: Option<String>,
    cascade_range: Option<Range>,
    backref: Option<String>,
    remote_side: bool,
    has_foreign_keys: bool,
    viewonly: Option<bool>,
}

fn parse_relationship_args(args: &Node, source: &[u8]) -> RelArgs {
    let mut target: Option<String> = None;
    let mut target_range: Option<Range> = None;
    let mut back_populates: Option<String> = None;
    let mut back_populates_range: Option<Range> = None;
    let mut lazy: Option<String> = None;
    let mut uselist: Option<bool> = None;
    let mut secondary: Option<String> = None;
    let mut cascade: Option<String> = None;
    let mut cascade_range: Option<Range> = None;
    let mut backref: Option<String> = None;
    let mut remote_side = false;
    let mut has_foreign_keys = false;
    let mut viewonly: Option<bool> = None;
    let mut positional = 0usize;

    let mut c = args.walk();
    for arg in args.named_children(&mut c) {
        match arg.kind() {
            "keyword_argument" => {
                let key = arg
                    .child_by_field_name("name")
                    .map(|n| node_text(n, source))
                    .unwrap_or("");
                let val_node = arg.child_by_field_name("value");
                let val = val_node.map(|n| node_text(n, source)).unwrap_or("");
                match key {
                    "back_populates" => {
                        back_populates = Some(strip_string_quotes(val).to_string());
                        back_populates_range = val_node.map(|n| ts_range(n));
                    }
                    "lazy" => lazy = Some(strip_string_quotes(val).to_string()),
                    "uselist" => uselist = Some(val == "True"),
                    "secondary" => secondary = Some(strip_string_quotes(val).to_string()),
                    "cascade" => {
                        cascade = Some(strip_string_quotes(val).to_string());
                        cascade_range = val_node.map(|n| ts_range(n));
                    }
                    "backref" => backref = Some(strip_string_quotes(val).to_string()),
                    "remote_side" => remote_side = true,
                    "foreign_keys" => has_foreign_keys = true,
                    "viewonly" => viewonly = Some(val == "True"),
                    _ => {}
                }
            }
            "string" if positional == 0 => {
                let t = strip_string_quotes(node_text(arg, source)).to_string();
                target_range = Some(ts_range(arg));
                target = Some(t);
                positional += 1;
            }
            "identifier" if positional == 0 => {
                target = Some(node_text(arg, source).to_string());
                target_range = Some(ts_range(arg));
                positional += 1;
            }
            _ => {
                if arg.kind() != "keyword_argument" {
                    positional += 1;
                }
            }
        }
    }

    let explicit_target = target.clone();
    RelArgs {
        target,
        explicit_target,
        target_range,
        back_populates,
        back_populates_range,
        lazy,
        uselist,
        secondary,
        cascade,
        cascade_range,
        backref,
        remote_side,
        has_foreign_keys,
        viewonly,
    }
}

// ── Column extraction ─────────────────────────────────────────────────────────

fn try_extract_column(assign: &Node, source: &[u8], sym: &SymbolTable) -> Option<Column> {
    let name_node = assign.child_by_field_name("left")?;
    let name = node_text(name_node, source);
    if name.starts_with("__") {
        return None;
    }
    let name_range = ts_range(name_node);
    let full_range = ts_range(*assign);

    let annotation = assign.child_by_field_name("type")?;
    let ann_text = node_text(annotation, source);
    let mapped_type = parse_mapped_type(ann_text, sym);

    let (mut args, fk) = match assign.child_by_field_name("right") {
        Some(rhs) if rhs.kind() == "call" => {
            let func_name = rhs
                .child_by_field_name("function")
                .map(|n| node_text(n, source))
                .unwrap_or("");
            if sym.is_mapped_column(func_name) || func_name == "mapped_column" {
                rhs.child_by_field_name("arguments")
                    .map(|a| parse_mapped_column_args(&a, source, sym))
                    .unwrap_or_default()
            } else {
                (ColumnArgs::default(), None)
            }
        }
        _ => (ColumnArgs::default(), None),
    };

    args.nullable = infer_nullable(&mapped_type, args.nullable);

    Some(Column {
        name: name.to_string(),
        key: None,
        mapped_type,
        args,
        foreign_key: fk,
        doc: None,
        name_range,
        full_range,
    })
}

fn parse_mapped_column_args(
    args: &Node,
    source: &[u8],
    sym: &SymbolTable,
) -> (ColumnArgs, Option<ForeignKeyRef>) {
    let mut ca = ColumnArgs::default();
    let mut fk: Option<ForeignKeyRef> = None;

    let mut c = args.walk();
    for arg in args.named_children(&mut c) {
        match arg.kind() {
            "keyword_argument" => {
                let key = arg
                    .child_by_field_name("name")
                    .map(|n| node_text(n, source))
                    .unwrap_or("");
                let val = arg
                    .child_by_field_name("value")
                    .map(|n| node_text(n, source))
                    .unwrap_or("");
                match key {
                    "primary_key" => ca.primary_key = val == "True",
                    "nullable" => ca.nullable = val == "True",
                    "unique" => ca.unique = val == "True",
                    "index" => ca.index = val == "True",
                    "default" => ca.default = Some(val.to_string()),
                    "server_default" => ca.server_default = Some(val.to_string()),
                    _ => {}
                }
            }
            "call" => {
                let func_name = arg
                    .child_by_field_name("function")
                    .map(|n| node_text(n, source))
                    .unwrap_or("");
                if sym.is_foreign_key(func_name) || func_name == "ForeignKey" {
                    if let Some(fkr) = parse_foreign_key(&arg, source) {
                        fk = Some(fkr);
                    }
                }
            }
            _ => {}
        }
    }
    (ca, fk)
}

fn parse_foreign_key(fk_call: &Node, source: &[u8]) -> Option<ForeignKeyRef> {
    let args = fk_call.child_by_field_name("arguments")?;
    let mut c = args.walk();
    let first = args.named_children(&mut c).next()?;
    let raw = node_text(first, source);
    let raw_text = strip_string_quotes(raw);
    let dot = raw_text.rfind('.')?;
    let table = raw_text[..dot].to_string();
    let column = raw_text[dot + 1..].to_string();
    let range = ts_range(first);
    Some(ForeignKeyRef {
        table,
        column,
        raw_text: raw_text.to_string(),
        range,
    })
}

fn infer_nullable(mt: &MappedType, explicit: bool) -> bool {
    match mt {
        MappedType::Optional(_) => true,
        MappedType::Int
        | MappedType::Str
        | MappedType::Float
        | MappedType::Bool
        | MappedType::DateTime
        | MappedType::SqlType { .. } => false,
        _ => explicit,
    }
}

// ── Type annotation parsing ───────────────────────────────────────────────────

/// Parse a `Mapped[...]` annotation text into a `MappedType`.
pub fn parse_mapped_type(annotation: &str, sym: &SymbolTable) -> MappedType {
    let inner = strip_mapped_wrapper(annotation);
    parse_inner_type(inner.trim(), sym)
}

fn strip_mapped_wrapper(annotation: &str) -> &str {
    if let Some(open) = annotation.find('[') {
        let name = annotation[..open].trim();
        if name == "Mapped" || name.ends_with(".Mapped") {
            let close = annotation.rfind(']').unwrap_or(annotation.len());
            return &annotation[open + 1..close];
        }
    }
    annotation
}

pub fn parse_inner_type(type_str: &str, sym: &SymbolTable) -> MappedType {
    let s = type_str.trim();

    if let Some(inner) = strip_generic_wrapper(s, &["Optional", "Opt"]) {
        return MappedType::Optional(Box::new(parse_inner_type(inner, sym)));
    }
    if let Some(inner) = strip_union_none(s) {
        return MappedType::Optional(Box::new(parse_inner_type(inner, sym)));
    }
    if let Some(inner) = strip_generic_wrapper(s, &["List", "list"]) {
        let model = strip_quotes(inner.trim());
        return MappedType::List(model.to_string());
    }
    if let Some(inner) = strip_annotated(s) {
        return parse_inner_type(inner, sym);
    }
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        return MappedType::ForwardRef(strip_quotes(s).to_string());
    }
    match s {
        "int" => MappedType::Int,
        "str" => MappedType::Str,
        "float" => MappedType::Float,
        "bool" => MappedType::Bool,
        "datetime" | "datetime.datetime" => MappedType::DateTime,
        _ => {
            if let Some(paren) = s.find('(') {
                let type_name = &s[..paren];
                let args_text = &s[paren + 1..s.rfind(')').unwrap_or(s.len())];
                let type_args: Vec<String> = if args_text.trim().is_empty() {
                    vec![]
                } else {
                    args_text.split(',').map(|a| a.trim().to_string()).collect()
                };
                MappedType::SqlType {
                    name: type_name.to_string(),
                    args: type_args,
                }
            } else if SA_TYPE_NAMES.contains(&s) {
                MappedType::SqlType {
                    name: s.to_string(),
                    args: vec![],
                }
            } else if let Some(canonical) = sym.resolve(s) {
                let base = canonical.rsplit('.').next().unwrap_or(s);
                if SA_TYPE_NAMES.contains(&base) {
                    MappedType::SqlType {
                        name: base.to_string(),
                        args: vec![],
                    }
                } else {
                    MappedType::Unknown(s.to_string())
                }
            } else {
                MappedType::Unknown(s.to_string())
            }
        }
    }
}

const SA_TYPE_NAMES: &[&str] = &[
    "String",
    "Integer",
    "Float",
    "Boolean",
    "DateTime",
    "Text",
    "BigInteger",
    "SmallInteger",
    "Numeric",
    "Date",
    "Time",
    "LargeBinary",
    "JSON",
    "ARRAY",
    "UUID",
    "Interval",
];

fn strip_generic_wrapper<'a>(s: &'a str, names: &[&str]) -> Option<&'a str> {
    for name in names {
        let prefix = format!("{name}[");
        if s.starts_with(prefix.as_str()) && s.ends_with(']') {
            return Some(&s[prefix.len()..s.len() - 1]);
        }
    }
    None
}

fn strip_union_none(s: &str) -> Option<&str> {
    for suffix in &[" | None]", " | None"] {
        if let Some(idx) = s.rfind(suffix) {
            let remainder = &s[idx + suffix.len()..];
            if remainder.is_empty() || remainder == "]" {
                return Some(s[..idx].trim());
            }
        }
    }
    if let Some(rest) = s.strip_prefix("None | ") {
        return Some(rest.trim());
    }
    None
}

fn strip_annotated(s: &str) -> Option<&str> {
    for prefix in &["Annotated[", "Ann["] {
        if s.starts_with(prefix) && s.ends_with(']') {
            let inner = &s[prefix.len()..s.len() - 1];
            return Some(
                find_top_level_comma(inner)
                    .map(|i| inner[..i].trim())
                    .unwrap_or(inner.trim()),
            );
        }
    }
    None
}

fn find_top_level_comma(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

fn strip_quotes(s: &str) -> &str {
    s.trim_matches('"').trim_matches('\'')
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("load python grammar");
        parser.parse(source, None).expect("parse")
    }

    fn sym() -> SymbolTable {
        SymbolTable::default()
    }

    // ── Symbol table ──────────────────────────────────────────────────────────

    #[test]
    fn symbol_table_bare_import() {
        let src = "import sqlalchemy\n";
        let tree = parse(src);
        let st = build_symbol_table(tree.root_node(), src.as_bytes());
        assert_eq!(st.module_path("sqlalchemy"), Some("sqlalchemy"));
    }

    #[test]
    fn symbol_table_aliased_module() {
        let src = "import sqlalchemy as sa\n";
        let tree = parse(src);
        let st = build_symbol_table(tree.root_node(), src.as_bytes());
        assert_eq!(st.module_path("sa"), Some("sqlalchemy"));
    }

    #[test]
    fn symbol_table_from_import() {
        let src = "from sqlalchemy.orm import relationship, Mapped\n";
        let tree = parse(src);
        let st = build_symbol_table(tree.root_node(), src.as_bytes());
        assert!(st.is_relationship("relationship"));
        assert!(st.is_mapped("Mapped"));
    }

    #[test]
    fn symbol_table_aliased_from_import() {
        let src = "from sqlalchemy.orm import relationship as rel\n";
        let tree = parse(src);
        let st = build_symbol_table(tree.root_node(), src.as_bytes());
        assert!(st.is_relationship("rel"));
        assert!(
            !st.is_relationship("relationship"),
            "unimported name must not match"
        );
    }

    #[test]
    fn symbol_table_module_attr_resolve() {
        let src = "import sqlalchemy as sa\n";
        let tree = parse(src);
        let st = build_symbol_table(tree.root_node(), src.as_bytes());
        assert_eq!(
            st.resolve("sa.String").as_deref(),
            Some("sqlalchemy.String")
        );
    }

    #[test]
    fn symbol_table_type_checking_block() {
        let src = "from __future__ import annotations\nif TYPE_CHECKING:\n    from .users import User as SomeUser\n";
        let tree = parse(src);
        let st = build_symbol_table(tree.root_node(), src.as_bytes());
        assert_eq!(st.resolve("SomeUser").as_deref(), Some(".users.User"));
    }

    // ── Type annotation parsing ───────────────────────────────────────────────

    #[test]
    fn parse_mapped_int() {
        assert!(matches!(
            parse_mapped_type("Mapped[int]", &sym()),
            MappedType::Int
        ));
    }

    #[test]
    fn parse_mapped_optional_str() {
        assert!(matches!(
            parse_mapped_type("Mapped[Optional[str]]", &sym()),
            MappedType::Optional(inner) if matches!(*inner, MappedType::Str)
        ));
    }

    #[test]
    fn parse_mapped_union_none() {
        assert!(matches!(
            parse_mapped_type("Mapped[str | None]", &sym()),
            MappedType::Optional(inner) if matches!(*inner, MappedType::Str)
        ));
    }

    #[test]
    fn parse_mapped_list() {
        let mt = parse_mapped_type("Mapped[List[Post]]", &sym());
        assert!(matches!(mt, MappedType::List(ref n) if n == "Post"));
    }

    #[test]
    fn parse_mapped_forward_ref() {
        let mt = parse_mapped_type("Mapped[\"User\"]", &sym());
        assert!(matches!(mt, MappedType::ForwardRef(ref n) if n == "User"));
    }

    #[test]
    fn parse_mapped_sql_type() {
        let mt = parse_mapped_type("Mapped[String(120)]", &sym());
        assert!(matches!(mt, MappedType::SqlType { ref name, .. } if name == "String"));
    }

    #[test]
    fn annotated_unwrap() {
        let mt = parse_mapped_type(
            "Mapped[Annotated[int, mapped_column(primary_key=True)]]",
            &sym(),
        );
        assert!(matches!(mt, MappedType::Int));
    }

    // ── Model extraction ──────────────────────────────────────────────────────

    const SIMPLE_MODEL: &str = r#"
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column
from sqlalchemy import Integer, String

class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "users"
    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    name: Mapped[str] = mapped_column(String(120))
"#;

    #[test]
    fn extract_simple_model() {
        let tree = parse(SIMPLE_MODEL);
        let models = extract_models(SIMPLE_MODEL, &tree);
        assert_eq!(models.len(), 1);
        let m = &models[0];
        assert_eq!(m.name, "User");
        assert_eq!(m.table_name.as_deref(), Some("users"));
        assert!(m.columns.contains_key("id"));
        assert!(m.columns.contains_key("name"));
    }

    #[test]
    fn extract_base_not_a_model() {
        let tree = parse(SIMPLE_MODEL);
        let models = extract_models(SIMPLE_MODEL, &tree);
        assert!(!models.iter().any(|m| m.name == "Base"));
    }

    #[test]
    fn column_primary_key_flag() {
        let tree = parse(SIMPLE_MODEL);
        let models = extract_models(SIMPLE_MODEL, &tree);
        let col = &models[0].columns["id"];
        assert!(col.args.primary_key);
    }

    #[test]
    fn column_nullability_inferred() {
        let src = r#"
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column
from sqlalchemy import Integer, String
class Base(DeclarativeBase): pass
class Post(Base):
    __tablename__ = "posts"
    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    title: Mapped[str] = mapped_column(String)
    body: Mapped[Optional[str]] = mapped_column(String)
"#;
        let tree = parse(src);
        let models = extract_models(src, &tree);
        let m = &models[0];
        assert!(!m.columns["id"].args.nullable, "int is non-nullable");
        assert!(!m.columns["title"].args.nullable, "str is non-nullable");
        assert!(m.columns["body"].args.nullable, "Optional[str] is nullable");
    }

    #[test]
    fn relationship_extracted() {
        let src = r#"
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column, relationship
from typing import List
class Base(DeclarativeBase): pass
class User(Base):
    __tablename__ = "users"
    id: Mapped[int] = mapped_column(primary_key=True)
    posts: Mapped[List["Post"]] = relationship("Post", back_populates="author")
"#;
        let tree = parse(src);
        let models = extract_models(src, &tree);
        let rel = models[0]
            .relationships
            .get("posts")
            .expect("posts relationship");
        assert_eq!(rel.target_model, "Post");
        assert_eq!(rel.back_populates.as_deref(), Some("author"));
        assert!(rel.is_list);
    }

    #[test]
    fn foreign_key_extracted() {
        let src = r#"
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column
from sqlalchemy import Integer, ForeignKey
class Base(DeclarativeBase): pass
class Post(Base):
    __tablename__ = "posts"
    author_id: Mapped[int] = mapped_column(Integer, ForeignKey("users.id"))
"#;
        let tree = parse(src);
        let models = extract_models(src, &tree);
        let col = &models[0].columns["author_id"];
        let fk = col.foreign_key.as_ref().expect("foreign key");
        assert_eq!(fk.table, "users");
        assert_eq!(fk.column, "id");
    }

    #[test]
    fn duplicate_column_tracked() {
        let src = r#"
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column
from sqlalchemy import String
class Base(DeclarativeBase): pass
class Post(Base):
    __tablename__ = "posts"
    title: Mapped[str] = mapped_column(String)
    title: Mapped[str] = mapped_column(String(200))
"#;
        let tree = parse(src);
        let models = extract_models(src, &tree);
        assert_eq!(models[0].duplicate_columns.len(), 1);
        assert_eq!(models[0].duplicate_columns[0].0, "title");
    }

    #[test]
    fn no_models_on_plain_python() {
        let src = "x = 1\ndef foo(): pass\n";
        let tree = parse(src);
        let models = extract_models(src, &tree);
        assert!(models.is_empty());
    }
}
