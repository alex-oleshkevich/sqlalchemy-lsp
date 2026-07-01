use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// REQ-EDIT-05: extension.toml registers the server for Python with required fields.
#[test]
fn zed_extension_toml_registers_for_python() {
    let path = repo_root().join("editors/zed/extension.toml");
    assert!(path.exists(), "editors/zed/extension.toml must exist");

    let content = std::fs::read_to_string(&path).expect("read extension.toml");
    let doc: toml::Value = toml::from_str(&content).expect("parse extension.toml as TOML");

    assert_eq!(doc["id"].as_str().unwrap(), "sqlalchemy-lsp");
    assert!(doc.get("name").is_some(), "name field required");
    assert!(doc.get("version").is_some(), "version field required");
    assert_eq!(doc["schema_version"].as_integer().unwrap(), 1);

    let lang_servers = doc
        .get("language_servers")
        .expect("language_servers section");
    let server = lang_servers
        .get("sqlalchemy-lsp")
        .expect("sqlalchemy-lsp server entry");
    let languages = server["languages"].as_array().unwrap();
    assert!(
        languages.iter().any(|l| l.as_str() == Some("Python")),
        "must register for Python language"
    );
}

/// REQ-EDIT-07: extension.toml carries exact marketplace metadata fields.
#[test]
fn zed_extension_toml_marketplace_metadata() {
    let path = repo_root().join("editors/zed/extension.toml");
    let content = std::fs::read_to_string(&path).expect("read extension.toml");
    let doc: toml::Value = toml::from_str(&content).expect("parse extension.toml as TOML");

    assert_eq!(
        doc["repository"].as_str().unwrap(),
        "https://github.com/alex-oleshkevich/sqlalchemy-lsp",
        "repository must match the exact marketplace URL"
    );

    let authors = doc["authors"].as_array().unwrap();
    assert!(!authors.is_empty(), "authors must be non-empty");
    assert!(
        authors.iter().any(|a| a
            .as_str()
            .map(|s| s.contains("alex.oleshkevich"))
            .unwrap_or(false)),
        "authors must include alex.oleshkevich"
    );
}

/// REQ-EDIT-07: root LICENSE exists (required by Zed marketplace validator).
#[test]
fn license_file_exists_at_repo_root() {
    let license = repo_root().join("LICENSE");
    assert!(
        license.exists(),
        "LICENSE must exist at repo root for Zed marketplace validation"
    );
}

/// REQ-EDIT-07: package-zed-extension.sh copies LICENSE into editors/zed/ directory.
#[test]
fn package_script_copies_license_into_zed_dir() {
    let script = repo_root().join("scripts/package-zed-extension.sh");
    assert!(
        script.exists(),
        "scripts/package-zed-extension.sh must exist"
    );

    let content = std::fs::read_to_string(&script).expect("read package-zed-extension.sh");
    assert!(
        content.contains("cp LICENSE \"$ZED_SRC/\"")
            || content.contains("cp LICENSE \"${ZED_SRC}/\""),
        "package script must copy LICENSE into editors/zed/ for marketplace validation"
    );
}

/// REQ-EDIT-01: install and package scripts reference the correct binary name.
#[test]
fn scripts_reference_sqlalchemy_lsp_binary() {
    for script_name in &["install-zed-extension.sh", "package-zed-extension.sh"] {
        let path = repo_root().join("scripts").join(script_name);
        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("read scripts/{script_name}"));
        assert!(
            content.contains("sqlalchemy_lsp_zed") || content.contains("sqlalchemy-lsp"),
            "{script_name} must reference sqlalchemy-lsp, not babel-lsp"
        );
        assert!(
            !content.contains("babel_lsp_zed") && !content.contains("babel-lsp"),
            "{script_name} must not reference babel-lsp"
        );
    }
}
