mod common;

use std::fs;

/// Verify every .py file in the clean-blog fixture workspace parses cleanly
/// under tree-sitter-python (no ERROR nodes at root).
#[test]
fn clean_blog_models_parse_without_errors() {
    let models = common::fixture("clean_blog").join("models");
    let files = common::py_files(&models);
    assert!(!files.is_empty(), "no .py files found in clean_blog/models");

    for path in &files {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let label = path.file_name().unwrap().to_string_lossy();
        common::assert_parses_without_error(&source, &label);
    }
}

#[test]
fn clean_blog_migrations_parse_without_errors() {
    let versions = common::fixture("clean_blog").join("migrations/versions");
    let files = common::py_files(&versions);
    assert!(!files.is_empty(), "no migration files found in clean_blog/migrations/versions");

    for path in &files {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let label = path.file_name().unwrap().to_string_lossy();
        common::assert_parses_without_error(&source, &label);
    }
}
