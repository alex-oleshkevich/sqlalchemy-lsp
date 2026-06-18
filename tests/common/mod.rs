use std::path::{Path, PathBuf};

/// Path to the shared fixture workspaces under `tests/e2e/fixtures/`.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/e2e/fixtures")
}

/// Path to a named fixture workspace.
pub fn fixture(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

/// Collect all `.py` files under a directory, non-recursively.
pub fn py_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    rd.filter_map(|e| {
        let path = e.ok()?.path();
        if path.extension()?.to_str()? == "py" { Some(path) } else { None }
    })
    .collect()
}

/// Parse `source` with tree-sitter-python and assert no root-level error.
pub fn assert_parses_without_error(source: &str, label: &str) {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("load tree-sitter-python");
    let tree = parser.parse(source, None).expect("tree-sitter parse");
    assert!(
        !tree.root_node().has_error(),
        "{label}: tree-sitter parse produced errors",
    );
}
