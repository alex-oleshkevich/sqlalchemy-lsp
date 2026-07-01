use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// REQ-REL-05: version-check job compares tag to Cargo.toml via cargo metadata.
#[test]
fn release_yml_has_version_check_job() {
    let content = read(".github/workflows/release.yml");
    assert!(
        content.contains("version-check"),
        "version-check job must exist"
    );
    assert!(
        content.contains("cargo metadata"),
        "version-check must use cargo metadata"
    );
    assert!(
        content.contains("packages[0].version") || content.contains(".version"),
        "version-check must extract crate version"
    );
}

/// REQ-REL-06: all five cross-compile targets are present.
#[test]
fn release_yml_has_all_five_targets() {
    let content = read(".github/workflows/release.yml");
    for target in &[
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ] {
        assert!(
            content.contains(target),
            "release.yml must include target {target}"
        );
    }
}

/// REQ-REL-06: aarch64-linux is built with cross.
#[test]
fn release_yml_uses_cross_for_aarch64_linux() {
    let content = read(".github/workflows/release.yml");
    assert!(
        content.contains("cross: true"),
        "aarch64-linux must be flagged for cross"
    );
    assert!(
        content.contains("cross build"),
        "cross build command must be present"
    );
}

/// REQ-REL-06: non-Windows binaries are stripped before upload.
#[test]
fn release_yml_strips_binaries() {
    let content = read(".github/workflows/release.yml");
    assert!(
        content.contains("strip "),
        "release.yml must strip non-Windows binaries"
    );
}

/// REQ-REL-07: release job packages the Zed extension.
#[test]
fn release_yml_packages_zed_extension() {
    let content = read(".github/workflows/release.yml");
    assert!(
        content.contains("package-zed-extension.sh"),
        "release job must call package-zed-extension.sh"
    );
}

/// REQ-REL-08: publish-aur job with AUR_SSH_KEY guard and aur.archlinux.org push.
#[test]
fn release_yml_has_publish_aur_job() {
    let content = read(".github/workflows/release.yml");
    assert!(
        content.contains("publish-aur"),
        "publish-aur job must exist"
    );
    assert!(
        content.contains("aur.archlinux.org"),
        "publish-aur must push to aur.archlinux.org"
    );
    assert!(
        content.contains("AUR_SSH_KEY"),
        "publish-aur must be gated on AUR_SSH_KEY"
    );
}

/// REQ-REL-10: no ${{ github.* }} context interpolated directly inside run: scripts.
/// All github contexts must go through env: blocks, not inline in shell commands.
#[test]
fn release_yml_no_github_context_in_run_scripts() {
    let content = read(".github/workflows/release.yml");

    // Walk lines and track whether we're inside a run: block.
    // A run: block starts at a line containing "run:" and ends at the next
    // dedented key. We flag any line inside a run: block that contains "${{ github.".
    let mut in_run_block = false;
    let mut run_indent = 0usize;
    let mut bad_lines: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();

        // Detect start of a run: block (multi-line or inline).
        if trimmed.starts_with("run:") {
            in_run_block = true;
            run_indent = indent;
            // Inline run: — check the same line.
            if trimmed.contains("${{ github.") {
                bad_lines.push(line.to_string());
            }
            continue;
        }

        if in_run_block {
            // A less-indented or same-indented key ends the run block.
            if !trimmed.is_empty()
                && indent <= run_indent
                && trimmed.contains(':')
                && !trimmed.starts_with('-')
            {
                in_run_block = false;
            } else if trimmed.contains("${{ github.") {
                bad_lines.push(line.to_string());
            }
        }
    }

    assert!(
        bad_lines.is_empty(),
        "github context used directly in run: scripts (injection risk). \
         Route through env: instead.\nOffending lines:\n{}",
        bad_lines.join("\n")
    );
}

/// REQ-REL-07: package-zed-extension.sh copies LICENSE and produces a zip.
#[test]
fn package_script_produces_zip_with_license() {
    let content = read("scripts/package-zed-extension.sh");
    assert!(
        content.contains(".zip"),
        "package script must produce a .zip artifact"
    );
    assert!(
        content.contains("cp LICENSE") || content.contains("copy LICENSE"),
        "package script must copy LICENSE into the extension directory"
    );
}

/// REQ-REL-08: pkg/aur/PKGBUILD and .SRCINFO templates exist with correct metadata.
#[test]
fn aur_pkgbuild_template_exists() {
    let content = read("pkg/aur/PKGBUILD");
    assert!(
        content.contains("pkgname=sqlalchemy-lsp"),
        "PKGBUILD must set pkgname"
    );
    assert!(content.contains("x86_64"), "PKGBUILD must support x86_64");
    assert!(content.contains("aarch64"), "PKGBUILD must support aarch64");
    assert!(
        content.contains("alex-oleshkevich/sqlalchemy-lsp"),
        "PKGBUILD must point to the correct GitHub repo"
    );
}

/// REQ-REL-08: a valid .SRCINFO template is committed. The publish-aur job regenerates
/// it with `makepkg --printsrcinfo` inside an archlinux container (see release.yml), so
/// the committed template only needs the expected fields and no malformed markers.
#[test]
fn aur_srcinfo_template_is_valid() {
    let content = read("pkg/aur/.SRCINFO");
    assert!(
        content.contains("pkgbase = sqlalchemy-lsp"),
        ".SRCINFO must define pkgbase"
    );
    assert!(
        content.contains("pkgname = sqlalchemy-lsp"),
        ".SRCINFO must define pkgname"
    );
    // makepkg --printsrcinfo never emits a "%PACKAGE%" marker; its presence means the
    // template is malformed and the AUR will reject it.
    assert!(
        !content.contains("%PACKAGE%"),
        ".SRCINFO must not contain a %PACKAGE% marker (invalid; AUR rejects it)"
    );
    // The publish-aur job rewrites these exact lines with sed; they must exist verbatim
    // in the template or the sed no-ops and the AUR push ships stale metadata.
    for line in &[
        "pkgver = ",
        "source_x86_64 = ",
        "source_aarch64 = ",
        "sha256sums_x86_64 = ",
        "sha256sums_aarch64 = ",
    ] {
        assert!(
            content.contains(line),
            ".SRCINFO must contain a `{line}` line for the release sed to update"
        );
    }
}
