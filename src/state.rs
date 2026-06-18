use dashmap::DashMap;
use tower_lsp_server::ls_types::{Diagnostic, Uri};
use tree_sitter::Tree;

use crate::alembic::MigrationFile;
use crate::model::types::{Model, ModelLocation};

pub type FileModels = Vec<Model>;

/// The workspace index: keyed for one-step lookup by every feature.
pub struct WorkspaceState {
    pub file_models: DashMap<Uri, FileModels>,
    pub model_index: DashMap<String, ModelLocation>,
    pub table_index: DashMap<String, String>,
    pub file_sources: DashMap<Uri, String>,
    pub parse_trees: DashMap<Uri, Tree>,
    pub migration_files: DashMap<Uri, MigrationFile>,
    pub revision_index: DashMap<String, Uri>,
    /// Published diagnostics, keyed by URI. Cleared on file delete; empty vec on no findings.
    pub diagnostics: DashMap<Uri, Vec<Diagnostic>>,
    /// URIs of files currently open in the editor (unsaved overlay takes precedence over disk).
    pub open_uris: DashMap<Uri, ()>,
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self {
            file_models: DashMap::new(),
            model_index: DashMap::new(),
            table_index: DashMap::new(),
            file_sources: DashMap::new(),
            parse_trees: DashMap::new(),
            migration_files: DashMap::new(),
            revision_index: DashMap::new(),
            diagnostics: DashMap::new(),
            open_uris: DashMap::new(),
        }
    }

    /// Replace a file's model facts atomically: purge old reverse-index entries,
    /// insert new ones, then swap the per-file record.
    pub fn update_file(&self, uri: &Uri, models: FileModels) {
        if let Some(old) = self.file_models.get(uri) {
            for model in old.iter() {
                self.model_index.remove(&model.name);
                if let Some(ref table) = model.table_name {
                    self.table_index.remove(table);
                }
            }
        }
        for model in &models {
            self.model_index.insert(
                model.name.clone(),
                ModelLocation {
                    uri: uri.clone(),
                    model_name: model.name.clone(),
                    range: model.name_range,
                },
            );
            if let Some(ref table) = model.table_name {
                self.table_index.insert(table.clone(), model.name.clone());
            }
        }
        self.file_models.insert(uri.clone(), models);
    }

    /// Replace a file's migration facts atomically, keeping revision_index in sync.
    pub fn update_migration(&self, uri: &Uri, mf: MigrationFile) {
        if let Some(old) = self.migration_files.get(uri) {
            if let Some(ref rev) = old.revision {
                self.revision_index.remove(rev);
            }
        }
        if let Some(ref rev) = mf.revision {
            self.revision_index.insert(rev.clone(), uri.clone());
        }
        self.migration_files.insert(uri.clone(), mf);
    }

    /// Remove a file's facts from every map and clear its diagnostics entry.
    /// Caller is responsible for publishing an empty diagnostics list to the client.
    pub fn remove_file(&self, uri: &Uri) {
        if let Some((_, old_models)) = self.file_models.remove(uri) {
            for model in &old_models {
                self.model_index.remove(&model.name);
                if let Some(ref table) = model.table_name {
                    self.table_index.remove(table);
                }
            }
        }
        self.file_sources.remove(uri);
        self.parse_trees.remove(uri);
        if let Some((_, mf)) = self.migration_files.remove(uri) {
            if let Some(ref rev) = mf.revision {
                self.revision_index.remove(rev);
            }
        }
        self.diagnostics.remove(uri);
    }
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alembic::DownRevision;
    use crate::model::types::{MappedType, Range};
    use std::collections::HashMap;

    fn make_uri(s: &str) -> Uri {
        s.parse().expect("valid URI")
    }

    fn make_model(name: &str, table: &str) -> Model {
        Model {
            name: name.to_string(),
            table_name: Some(table.to_string()),
            bases: vec![],
            columns: HashMap::new(),
            relationships: HashMap::new(),
            table_args: vec![],
            duplicate_columns: vec![],
            docstring: None,
            name_range: Range::default(),
            full_range: Range::default(),
        }
    }

    fn make_migration(revision: &str, down: Option<&str>) -> MigrationFile {
        MigrationFile {
            revision: Some(revision.to_string()),
            down_revision: match down {
                Some(r) => DownRevision::Single(r.to_string()),
                None => DownRevision::None,
            },
            message: None,
            revision_range: None,
            down_revision_range: None,
            op_calls: vec![],
        }
    }

    #[test]
    fn update_file_populates_reverse_indexes() {
        let state = WorkspaceState::new();
        let uri = make_uri("file:///tmp/models.py");
        state.update_file(&uri, vec![make_model("User", "users")]);

        assert!(state.model_index.contains_key("User"));
        assert_eq!(
            state
                .table_index
                .get("users")
                .as_deref()
                .map(String::as_str),
            Some("User")
        );
        assert_eq!(state.file_models.get(&uri).map(|m| m.len()), Some(1));
    }

    #[test]
    fn update_file_purges_old_reverse_index_entries() {
        let state = WorkspaceState::new();
        let uri = make_uri("file:///tmp/models.py");

        state.update_file(&uri, vec![make_model("User", "users")]);
        state.update_file(&uri, vec![make_model("Post", "posts")]);

        assert!(
            !state.model_index.contains_key("User"),
            "old model must be purged"
        );
        assert!(
            !state.table_index.contains_key("users"),
            "old table must be purged"
        );
        assert!(state.model_index.contains_key("Post"));
        assert!(state.table_index.contains_key("posts"));
    }

    #[test]
    fn remove_file_clears_all_entries() {
        let state = WorkspaceState::new();
        let uri = make_uri("file:///tmp/models.py");
        state.update_file(&uri, vec![make_model("User", "users")]);
        state.remove_file(&uri);

        assert!(!state.file_models.contains_key(&uri));
        assert!(!state.model_index.contains_key("User"));
        assert!(!state.table_index.contains_key("users"));
    }

    #[test]
    fn update_migration_tracks_revision_index() {
        let state = WorkspaceState::new();
        let uri = make_uri("file:///tmp/v1.py");
        state.update_migration(&uri, make_migration("abc123", None));

        assert_eq!(state.revision_index.get("abc123").as_deref(), Some(&uri));
    }

    #[test]
    fn update_migration_purges_old_revision() {
        let state = WorkspaceState::new();
        let uri = make_uri("file:///tmp/v1.py");
        state.update_migration(&uri, make_migration("abc123", None));
        state.update_migration(&uri, make_migration("def456", Some("abc123")));

        assert!(
            !state.revision_index.contains_key("abc123"),
            "old revision must be purged"
        );
        assert!(state.revision_index.contains_key("def456"));
    }

    #[test]
    fn remove_file_clears_migration_revision() {
        let state = WorkspaceState::new();
        let uri = make_uri("file:///tmp/v1.py");
        state.update_migration(&uri, make_migration("abc123", None));
        state.remove_file(&uri);

        assert!(!state.migration_files.contains_key(&uri));
        assert!(!state.revision_index.contains_key("abc123"));
    }

    #[test]
    fn mapped_type_display() {
        assert_eq!(MappedType::Int.to_string(), "int");
        assert_eq!(MappedType::Str.to_string(), "str");
        assert_eq!(
            MappedType::Optional(Box::new(MappedType::Str)).to_string(),
            "Optional[str]"
        );
        assert_eq!(
            MappedType::List("Post".to_string()).to_string(),
            "List[Post]"
        );
        assert_eq!(
            MappedType::ForwardRef("User".to_string()).to_string(),
            "\"User\""
        );
        assert_eq!(
            MappedType::SqlType {
                name: "String".to_string(),
                args: vec!["120".to_string()]
            }
            .to_string(),
            "String(120)"
        );
        assert_eq!(MappedType::Unknown("Any".to_string()).to_string(), "Any");
    }
}
