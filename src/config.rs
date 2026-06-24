use std::{collections::HashMap, path::Path};

use serde::Deserialize;
use tower_lsp_server::ls_types::{Diagnostic, NumberOrString};

// ── Raw config types (as parsed from TOML) ───────────────────────────────────

fn default_select() -> Vec<String> {
    vec!["recommended".to_string()]
}

#[derive(Clone, Debug, Deserialize)]
pub struct DiagnosticsConfig {
    #[serde(default = "default_select")]
    pub select: Vec<String>,
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default)]
    pub severity: HashMap<String, String>,
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            select: default_select(),
            ignore: vec![],
            severity: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub model_paths: Vec<String>,
    pub alembic_path: Option<String>,
    pub target_dialect: Option<String>,
    #[serde(default)]
    pub diagnostics: DiagnosticsConfig,
    pub log_level: Option<String>,
    pub log_file: Option<String>,
}

impl Config {
    /// Merge two raw configs with per-key precedence: `overlay` wins per scalar key,
    /// list keys accumulate, map keys merge (overlay wins per entry).
    pub fn merge(mut self, overlay: Config) -> Config {
        // Scalar keys: overlay wins if set
        if overlay.alembic_path.is_some() {
            self.alembic_path = overlay.alembic_path;
        }
        if overlay.target_dialect.is_some() {
            self.target_dialect = overlay.target_dialect;
        }
        if overlay.log_level.is_some() {
            self.log_level = overlay.log_level;
        }
        if overlay.log_file.is_some() {
            self.log_file = overlay.log_file;
        }
        // model_paths: accumulate
        self.model_paths.extend(overlay.model_paths);
        // diagnostics.select: overlay replaces if it differs from the default
        if overlay.diagnostics.select != default_select() {
            self.diagnostics.select = overlay.diagnostics.select;
        }
        // diagnostics.ignore: accumulate
        self.diagnostics.ignore.extend(overlay.diagnostics.ignore);
        // diagnostics.severity: merge, overlay wins per entry
        for (k, v) in overlay.diagnostics.severity {
            self.diagnostics.severity.insert(k, v);
        }
        self
    }
}

// ── Code parsing ─────────────────────────────────────────────────────────────

/// Returns `true` if `code` belongs to the group identified by `token`
/// (e.g. `"SQLA-W303"` matches `"SQLA-3xx"`).
pub fn code_matches_class_token(code: &str, token: &str) -> bool {
    let Some(code_rest) = code.strip_prefix("SQLA-") else {
        return false;
    };
    let Some(tok_rest) = token.strip_prefix("SQLA-") else {
        return false;
    };
    // token format: "<digit>xx"  (3 chars, ends with "xx")
    if tok_rest.len() != 3 || !tok_rest.ends_with("xx") {
        return false;
    }
    let Some(tok_class) = tok_rest.chars().next() else {
        return false;
    };
    // code format: "<SEV><digit><NN>"  (4 chars)
    if code_rest.len() != 4 {
        return false;
    }
    let Some(code_class) = code_rest.chars().nth(1) else {
        return false;
    };
    tok_class == code_class
}

// ── Noqa suppression (REQ-CFG-09) ────────────────────────────────────────────

/// An inline `# noqa` marker parsed from a source line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoqaMarker {
    /// `# noqa` — suppresses all SQLA findings on this line.
    Bare,
    /// `# noqa: file` — suppresses all SQLA findings in the whole file.
    File,
    /// `# noqa: SQLA-W303, SQLA-W402` — suppresses only the listed codes.
    Codes(Vec<String>),
}

impl NoqaMarker {
    /// Parse a source line for a `# noqa` marker.
    pub fn parse(line: &str) -> Option<Self> {
        let idx = line.find("# noqa")?;
        let after = line[idx + 6..].trim_start_matches(' ');
        // Bare marker: nothing follows, or only whitespace/newline
        if after.is_empty() || after.starts_with('\n') || after.starts_with('\r') {
            return Some(NoqaMarker::Bare);
        }
        let rest = after.strip_prefix(':')?.trim_start();
        if rest == "file" {
            return Some(NoqaMarker::File);
        }
        let codes: Vec<String> = rest
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if codes.is_empty() {
            None
        } else {
            Some(NoqaMarker::Codes(codes))
        }
    }

    /// Returns `true` if this marker suppresses `code`.
    pub fn suppresses(&self, code: &str) -> bool {
        match self {
            // Bare and File only suppress SQLA-* codes, leaving foreign namespaces alone.
            NoqaMarker::Bare | NoqaMarker::File => code.starts_with("SQLA-"),
            NoqaMarker::Codes(codes) => codes.iter().any(|c| c == code),
        }
    }
}

// ── Config loading ────────────────────────────────────────────────────────────

/// Load and merge config from `pyproject.toml` (lower precedence) and
/// `sqlalchemy-lsp.toml` (higher precedence) found at `workspace_root`.
/// Missing files are silently skipped; parse errors are logged and skipped.
pub fn load_config(workspace_root: &Path) -> Config {
    let mut config = Config::default();

    // Layer 1: pyproject.toml → [tool.sqlalchemy-lsp]
    let pyproject = workspace_root.join("pyproject.toml");
    if let Ok(text) = std::fs::read_to_string(&pyproject) {
        match text.parse::<toml::Value>() {
            Ok(doc) => {
                if let Some(section) = doc.get("tool").and_then(|t| t.get("sqlalchemy-lsp")) {
                    match section.clone().try_into::<Config>() {
                        Ok(layer) => config = config.merge(layer),
                        Err(e) => {
                            tracing::warn!("pyproject.toml [tool.sqlalchemy-lsp] parse error: {e}")
                        }
                    }
                }
            }
            Err(e) => tracing::warn!("pyproject.toml TOML parse error: {e}"),
        }
    }

    // Layer 2: sqlalchemy-lsp.toml (wins over pyproject.toml per key)
    let lsp_toml = workspace_root.join("sqlalchemy-lsp.toml");
    if let Ok(text) = std::fs::read_to_string(&lsp_toml) {
        match toml::from_str::<Config>(&text) {
            Ok(layer) => config = config.merge(layer),
            Err(e) => tracing::warn!("sqlalchemy-lsp.toml parse error: {e}"),
        }
    }

    config
}

// ── Diagnostic filtering ──────────────────────────────────────────────────────

/// Remove diagnostics whose code matches any entry in `diag_config.ignore`.
/// Supports exact code strings (`"SQLA-W303"`) and class tokens (`"SQLA-3xx"`).
pub fn filter_diagnostics(
    diags: Vec<Diagnostic>,
    diag_config: &DiagnosticsConfig,
) -> Vec<Diagnostic> {
    if diag_config.ignore.is_empty() {
        return diags;
    }
    diags
        .into_iter()
        .filter(|d| {
            let code_str = match d.code.as_ref() {
                Some(NumberOrString::String(s)) => s.as_str(),
                _ => return true,
            };
            !diag_config
                .ignore
                .iter()
                .any(|ig| ig == code_str || code_matches_class_token(code_str, ig))
        })
        .collect()
}

// ── # noqa suppression (LSP path) ────────────────────────────────────────────

/// Remove diagnostics suppressed by `# noqa` comments in `source`.
///
/// - `# noqa: SQLA-W303` on the diagnostic's start line suppresses that code.
/// - `# noqa` (bare) on the start line suppresses all `SQLA-*` codes on that line.
/// - `# noqa: file` on any line in the file suppresses all `SQLA-*` findings.
pub fn apply_noqa_to_diagnostics(diags: Vec<Diagnostic>, source: &str) -> Vec<Diagnostic> {
    if !source.contains("# noqa") {
        return diags;
    }
    let lines: Vec<&str> = source.lines().collect();

    // File-level suppression: any `# noqa: file` on any line clears the whole file.
    if lines.iter().any(|l| {
        NoqaMarker::parse(l)
            .map(|m| matches!(m, NoqaMarker::File))
            .unwrap_or(false)
    }) {
        return vec![];
    }

    diags
        .into_iter()
        .filter(|d| {
            let line_idx = d.range.start.line as usize;
            let Some(line) = lines.get(line_idx) else {
                return true;
            };
            let Some(marker) = NoqaMarker::parse(line) else {
                return true;
            };
            let code = match d.code.as_ref() {
                Some(NumberOrString::String(s)) => s.as_str(),
                _ => return true,
            };
            !marker.suppresses(code)
        })
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── code_matches_class_token ──────────────────────────────────────────────

    #[test]
    fn class_token_matches_same_class() {
        assert!(code_matches_class_token("SQLA-W303", "SQLA-3xx"));
        assert!(code_matches_class_token("SQLA-H416", "SQLA-4xx"));
        assert!(code_matches_class_token("SQLA-I207", "SQLA-2xx"));
    }

    #[test]
    fn class_token_rejects_different_class() {
        assert!(!code_matches_class_token("SQLA-W303", "SQLA-4xx"));
        assert!(!code_matches_class_token("SQLA-H416", "SQLA-3xx"));
    }

    // ── REQ-CFG-09: noqa parsing ──────────────────────────────────────────────

    #[test]
    fn noqa_bare() {
        assert_eq!(NoqaMarker::parse("x = 1  # noqa"), Some(NoqaMarker::Bare));
    }

    #[test]
    fn noqa_file() {
        assert_eq!(NoqaMarker::parse("# noqa: file"), Some(NoqaMarker::File));
    }

    #[test]
    fn noqa_single_code() {
        assert_eq!(
            NoqaMarker::parse("x = 1  # noqa: SQLA-W303"),
            Some(NoqaMarker::Codes(vec!["SQLA-W303".to_string()]))
        );
    }

    #[test]
    fn noqa_multiple_codes() {
        let m = NoqaMarker::parse("x = 1  # noqa: SQLA-W303, SQLA-W402").unwrap();
        assert_eq!(
            m,
            NoqaMarker::Codes(vec!["SQLA-W303".to_string(), "SQLA-W402".to_string()])
        );
    }

    #[test]
    fn noqa_bare_suppresses_sqla_only() {
        assert!(NoqaMarker::Bare.suppresses("SQLA-W303"));
        assert!(
            !NoqaMarker::Bare.suppresses("E501"),
            "foreign namespace not suppressed"
        );
    }

    #[test]
    fn noqa_codes_suppresses_only_listed() {
        let m = NoqaMarker::Codes(vec!["SQLA-W303".to_string()]);
        assert!(m.suppresses("SQLA-W303"));
        assert!(!m.suppresses("SQLA-W402"));
        assert!(!m.suppresses("E501"));
    }

    #[test]
    fn noqa_absent_returns_none() {
        assert!(NoqaMarker::parse("x = 1  # a normal comment").is_none());
        assert!(NoqaMarker::parse("x = 1").is_none());
    }

    // ── REQ-CFG-01: per-key merge ─────────────────────────────────────────────

    #[test]
    fn merge_scalar_overlay_wins() {
        let base = Config {
            target_dialect: Some("sqlite".to_string()),
            ..Default::default()
        };
        let overlay = Config {
            target_dialect: Some("postgresql".to_string()),
            ..Default::default()
        };
        let merged = base.merge(overlay);
        assert_eq!(merged.target_dialect.as_deref(), Some("postgresql"));
    }

    #[test]
    fn merge_ignore_accumulates() {
        let base = Config {
            diagnostics: DiagnosticsConfig {
                ignore: vec!["SQLA-H205".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let overlay = Config {
            diagnostics: DiagnosticsConfig {
                ignore: vec!["SQLA-W303".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let merged = base.merge(overlay);
        assert_eq!(merged.diagnostics.ignore.len(), 2);
    }
}
