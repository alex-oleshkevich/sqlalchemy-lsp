use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use serde::Deserialize;
use tower_lsp_server::ls_types::{Diagnostic, NumberOrString};

use crate::model::types::{DiagnosticTags, FixKind, Severity};

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

#[derive(Clone, Debug, Deserialize)]
pub struct OverrideEntry {
    pub includes: Vec<String>,
    #[serde(default)]
    pub diagnostics: DiagnosticsConfig,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub model_paths: Vec<String>,
    pub alembic_path: Option<String>,
    pub target_dialect: Option<String>,
    #[serde(default)]
    pub diagnostics: DiagnosticsConfig,
    #[serde(default)]
    pub overrides: Vec<OverrideEntry>,
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
        // overrides: append in declaration order
        self.overrides.extend(overlay.overrides);
        self
    }

    /// Resolve the active diagnostic config for a given file path, applying
    /// any matching glob overrides on top of the base `diagnostics` block.
    pub fn resolve_for_file(
        &self,
        file_path: &str,
        registry: &CodeRegistry,
    ) -> (ResolvedDiagnosticsConfig, Vec<String>) {
        let mut combined = self.diagnostics.clone();
        for entry in &self.overrides {
            if glob_matches_any(&entry.includes, file_path) {
                if entry.diagnostics.select != default_select() {
                    combined.select = entry.diagnostics.select.clone();
                }
                combined.ignore.extend(entry.diagnostics.ignore.clone());
                for (k, v) in &entry.diagnostics.severity {
                    combined.severity.insert(k.clone(), v.clone());
                }
            }
        }
        ResolvedDiagnosticsConfig::resolve(&combined, registry)
    }
}

// ── Code parsing ─────────────────────────────────────────────────────────────

/// A parsed `SQLA-<SEV><CLASS><NN>` code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedCode {
    pub severity_char: char, // 'E' | 'W' | 'I' | 'H'
    pub class: u8,           // 1..=9
    pub rule: u8,            // 01..=99
}

impl ParsedCode {
    /// Returns `None` if `s` is not a well-formed `SQLA-<SEV><CLASS><NN>` code.
    pub fn parse(s: &str) -> Option<Self> {
        let rest = s.strip_prefix("SQLA-")?;
        let mut chars = rest.chars();
        let sev = chars.next()?;
        if !matches!(sev, 'E' | 'W' | 'I' | 'H') {
            return None;
        }
        let digits: String = chars.collect();
        if digits.len() != 3 {
            return None;
        }
        let class = digits[0..1].parse::<u8>().ok()?;
        let rule = digits[1..].parse::<u8>().ok()?;
        if class == 0 {
            return None;
        }
        Some(ParsedCode {
            severity_char: sev,
            class,
            rule,
        })
    }
}

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

/// Classifies a `select`/`ignore`/`severity` entry by specificity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagTarget {
    Preset(Preset),
    ClassToken(String),
    Code(String),
    Unknown(String),
}

impl DiagTarget {
    pub fn parse(s: &str) -> Self {
        match s {
            "recommended" => DiagTarget::Preset(Preset::Recommended),
            "all" => DiagTarget::Preset(Preset::All),
            "none" => DiagTarget::Preset(Preset::None),
            s => {
                let Some(rest) = s.strip_prefix("SQLA-") else {
                    return DiagTarget::Unknown(s.to_string());
                };
                if rest.len() == 3 && rest.ends_with("xx") {
                    DiagTarget::ClassToken(s.to_string())
                } else if ParsedCode::parse(s).is_some() {
                    DiagTarget::Code(s.to_string())
                } else {
                    DiagTarget::Unknown(s.to_string())
                }
            }
        }
    }
}

// ── Preset ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Recommended,
    All,
    None,
}

// ── Code registry (REQ-CFG-14) ───────────────────────────────────────────────

/// One entry in the central code registry — the single source of truth for
/// all metadata about a diagnostic code.
#[derive(Clone, Debug)]
pub struct CodeEntry {
    pub code: String,
    pub rule_name: String,
    pub default_severity: Severity,
    pub class: u8,
    pub enabled_by_default: bool,
    pub fix_kind: FixKind,
    pub tags: DiagnosticTags,
}

/// The authoritative registry every feature and config resolver reads.
/// Feature beads (F01/F02/F13) will register their codes at startup.
pub struct CodeRegistry {
    entries: Vec<CodeEntry>,
}

impl CodeRegistry {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn register(&mut self, entry: CodeEntry) {
        self.entries.push(entry);
    }

    pub fn get(&self, code: &str) -> Option<&CodeEntry> {
        self.entries.iter().find(|e| e.code == code)
    }

    pub fn by_class(&self, class: u8) -> impl Iterator<Item = &CodeEntry> {
        self.entries.iter().filter(move |e| e.class == class)
    }

    pub fn all_codes(&self) -> impl Iterator<Item = &CodeEntry> {
        self.entries.iter()
    }

    pub fn codes_for_preset(&self, preset: Preset) -> Vec<&CodeEntry> {
        match preset {
            Preset::All => self.entries.iter().collect(),
            Preset::Recommended => self
                .entries
                .iter()
                .filter(|e| e.enabled_by_default)
                .collect(),
            Preset::None => vec![],
        }
    }
}

impl Default for CodeRegistry {
    fn default() -> Self {
        let mut reg = Self::new();
        // Off-by-default preview rules (REQ-CFG-07): shaky heuristics that false-positive.
        reg.register(CodeEntry {
            code: "SQLA-H416".to_string(),
            rule_name: "viewonly-write".to_string(),
            default_severity: Severity::Hint,
            class: 4,
            enabled_by_default: false,
            fix_kind: FixKind::None,
            tags: DiagnosticTags::default(),
        });
        reg.register(CodeEntry {
            code: "SQLA-H602".to_string(),
            rule_name: "association-proxy-misconfigured".to_string(),
            default_severity: Severity::Hint,
            class: 6,
            enabled_by_default: false,
            fix_kind: FixKind::None,
            tags: DiagnosticTags::default(),
        });
        // Off-by-default style rule (REQ-CFG-07): opt-in, fires on nearly every column.
        reg.register(CodeEntry {
            code: "SQLA-I207".to_string(),
            rule_name: "missing-column-comment".to_string(),
            default_severity: Severity::Info,
            class: 2,
            enabled_by_default: false,
            fix_kind: FixKind::None,
            tags: DiagnosticTags::default(),
        });
        // Tooling meta-finding — always on.
        reg.register(CodeEntry {
            code: "SQLA-W901".to_string(),
            rule_name: "unused-noqa".to_string(),
            default_severity: Severity::Warning,
            class: 9,
            enabled_by_default: true,
            fix_kind: FixKind::Safe,
            tags: DiagnosticTags {
                fixable: true,
                ..Default::default()
            },
        });
        reg
    }
}

// ── Resolution (REQ-CFG-08) ──────────────────────────────────────────────────

/// The resolved active rule set and severity overrides for one file.
pub struct ResolvedDiagnosticsConfig {
    pub active_codes: HashSet<String>,
    pub severity_overrides: HashMap<String, Severity>,
}

impl ResolvedDiagnosticsConfig {
    /// Resolve select → ignore → severity (REQ-CFG-08).
    /// Returns the resolved config and a list of config warnings (unknown codes/tokens).
    pub fn resolve(diag: &DiagnosticsConfig, registry: &CodeRegistry) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut active: HashSet<String> = HashSet::new();

        // Step 1: select
        for s in &diag.select {
            match DiagTarget::parse(s) {
                DiagTarget::Preset(p) => {
                    for e in registry.codes_for_preset(p) {
                        active.insert(e.code.clone());
                    }
                }
                DiagTarget::ClassToken(ref tok) => {
                    for e in registry.all_codes() {
                        if code_matches_class_token(&e.code, tok) {
                            active.insert(e.code.clone());
                        }
                    }
                }
                DiagTarget::Code(ref code) => {
                    if registry.get(code).is_some() {
                        active.insert(code.clone());
                    } else {
                        warnings.push(format!("unknown diagnostic code in select: {code}"));
                    }
                }
                DiagTarget::Unknown(ref s) => {
                    warnings.push(format!("unknown select target: {s}"));
                }
            }
        }

        // Step 2: ignore
        for s in &diag.ignore {
            match DiagTarget::parse(s) {
                DiagTarget::Preset(p) => {
                    for e in registry.codes_for_preset(p) {
                        active.remove(&e.code);
                    }
                }
                DiagTarget::ClassToken(ref tok) => {
                    active.retain(|code| !code_matches_class_token(code, tok));
                }
                DiagTarget::Code(ref code) => {
                    active.remove(code);
                }
                DiagTarget::Unknown(ref s) => {
                    warnings.push(format!("unknown ignore target: {s}"));
                }
            }
        }

        // Step 3: severity — class tokens first, then specific codes (specificity order)
        let mut severity_overrides: HashMap<String, Severity> = HashMap::new();
        let class_entries: Vec<_> = diag
            .severity
            .iter()
            .filter(|(k, _)| matches!(DiagTarget::parse(k), DiagTarget::ClassToken(_)))
            .collect();
        let code_entries: Vec<_> = diag
            .severity
            .iter()
            .filter(|(k, _)| matches!(DiagTarget::parse(k), DiagTarget::Code(_)))
            .collect();
        for (tok, sev_str) in class_entries {
            if let Some(sev) = parse_severity_str(sev_str) {
                for e in registry.all_codes() {
                    if code_matches_class_token(&e.code, tok) {
                        severity_overrides.insert(e.code.clone(), sev);
                    }
                }
            }
        }
        for (code, sev_str) in code_entries {
            if let Some(sev) = parse_severity_str(sev_str) {
                severity_overrides.insert(code.clone(), sev);
            }
        }

        (
            Self {
                active_codes: active,
                severity_overrides,
            },
            warnings,
        )
    }

    pub fn is_active(&self, code: &str) -> bool {
        self.active_codes.contains(code)
    }

    pub fn effective_severity(&self, code: &str, registry: &CodeRegistry) -> Option<Severity> {
        if let Some(&sev) = self.severity_overrides.get(code) {
            return Some(sev);
        }
        registry.get(code).map(|e| e.default_severity)
    }
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

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_severity_str(s: &str) -> Option<Severity> {
    match s.to_lowercase().as_str() {
        "error" => Some(Severity::Error),
        "warning" | "warn" => Some(Severity::Warning),
        "info" | "information" => Some(Severity::Info),
        "hint" => Some(Severity::Hint),
        _ => None,
    }
}

fn glob_matches_any(patterns: &[String], path: &str) -> bool {
    use globset::{Glob, GlobSetBuilder};
    let mut builder = GlobSetBuilder::new();
    for pat in patterns {
        if let Ok(g) = Glob::new(pat) {
            builder.add(g);
        }
    }
    builder
        .build()
        .map(|set| set.is_match(path))
        .unwrap_or(false)
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
                        Err(e) => tracing::warn!("pyproject.toml [tool.sqlalchemy-lsp] parse error: {e}"),
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
    if lines
        .iter()
        .any(|l| NoqaMarker::parse(l).map(|m| matches!(m, NoqaMarker::File)).unwrap_or(false))
    {
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

    fn registry() -> CodeRegistry {
        CodeRegistry::default()
    }

    // ── REQ-CFG-05: code parsing ──────────────────────────────────────────────

    #[test]
    fn parsed_code_valid() {
        let c = ParsedCode::parse("SQLA-W303").unwrap();
        assert_eq!(c.severity_char, 'W');
        assert_eq!(c.class, 3);
        assert_eq!(c.rule, 3);
    }

    #[test]
    fn parsed_code_all_severity_chars() {
        assert!(ParsedCode::parse("SQLA-E101").is_some());
        assert!(ParsedCode::parse("SQLA-W303").is_some());
        assert!(ParsedCode::parse("SQLA-I207").is_some());
        assert!(ParsedCode::parse("SQLA-H416").is_some());
    }

    #[test]
    fn parsed_code_rejects_malformed() {
        assert!(ParsedCode::parse("SQLA-").is_none());
        assert!(ParsedCode::parse("SQLA-W30").is_none(), "too short");
        assert!(ParsedCode::parse("SQLA-W3003").is_none(), "too long");
        assert!(ParsedCode::parse("SQLA-X303").is_none(), "bad sev char");
        assert!(ParsedCode::parse("SQLA-W003").is_none(), "class 0");
        assert!(ParsedCode::parse("E303").is_none(), "missing prefix");
    }

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

    // ── REQ-CFG-07: default-on policy ────────────────────────────────────────

    #[test]
    fn off_by_default_set_is_exactly_three() {
        let reg = registry();
        let off: Vec<_> = reg.all_codes().filter(|e| !e.enabled_by_default).collect();
        let off_codes: Vec<&str> = off.iter().map(|e| e.code.as_str()).collect();
        assert!(
            off_codes.contains(&"SQLA-H416"),
            "H416 must be off by default"
        );
        assert!(
            off_codes.contains(&"SQLA-H602"),
            "H602 must be off by default"
        );
        assert!(
            off_codes.contains(&"SQLA-I207"),
            "I207 must be off by default"
        );
        assert_eq!(off_codes.len(), 3, "exactly three off-by-default rules");
    }

    // ── REQ-CFG-08: resolution order ─────────────────────────────────────────

    #[test]
    fn recommended_preset_excludes_off_by_default() {
        let reg = registry();
        let diag = DiagnosticsConfig::default(); // select = ["recommended"]
        let (resolved, warnings) = ResolvedDiagnosticsConfig::resolve(&diag, &reg);
        assert!(warnings.is_empty());
        assert!(resolved.is_active("SQLA-W901"), "W901 must be active");
        assert!(!resolved.is_active("SQLA-H416"), "H416 off by default");
        assert!(!resolved.is_active("SQLA-H602"), "H602 off by default");
        assert!(!resolved.is_active("SQLA-I207"), "I207 off by default");
    }

    #[test]
    fn all_preset_includes_off_by_default() {
        let reg = registry();
        let diag = DiagnosticsConfig {
            select: vec!["all".to_string()],
            ..Default::default()
        };
        let (resolved, _) = ResolvedDiagnosticsConfig::resolve(&diag, &reg);
        assert!(resolved.is_active("SQLA-H416"));
        assert!(resolved.is_active("SQLA-H602"));
        assert!(resolved.is_active("SQLA-I207"));
    }

    #[test]
    fn none_preset_starts_empty() {
        let reg = registry();
        let diag = DiagnosticsConfig {
            select: vec!["none".to_string()],
            ..Default::default()
        };
        let (resolved, _) = ResolvedDiagnosticsConfig::resolve(&diag, &reg);
        assert!(resolved.active_codes.is_empty());
    }

    #[test]
    fn ignore_removes_after_select() {
        let reg = registry();
        let diag = DiagnosticsConfig {
            select: vec!["all".to_string()],
            ignore: vec!["SQLA-W901".to_string()],
            ..Default::default()
        };
        let (resolved, _) = ResolvedDiagnosticsConfig::resolve(&diag, &reg);
        assert!(!resolved.is_active("SQLA-W901"));
        assert!(resolved.is_active("SQLA-H416"));
    }

    #[test]
    fn unknown_code_in_select_emits_warning() {
        let reg = registry();
        let diag = DiagnosticsConfig {
            select: vec!["SQLA-E999".to_string()],
            ..Default::default()
        };
        let (_, warnings) = ResolvedDiagnosticsConfig::resolve(&diag, &reg);
        assert!(!warnings.is_empty());
    }

    // ── REQ-CFG-12: class tokens + specificity ────────────────────────────────

    #[test]
    fn class_token_in_select_enables_group() {
        let reg = registry();
        let diag = DiagnosticsConfig {
            select: vec!["none".to_string(), "SQLA-4xx".to_string()],
            ..Default::default()
        };
        let (resolved, _) = ResolvedDiagnosticsConfig::resolve(&diag, &reg);
        assert!(resolved.is_active("SQLA-H416"), "H416 is class 4");
        assert!(!resolved.is_active("SQLA-W901"), "W901 is class 9, not 4");
    }

    #[test]
    fn specific_code_overrides_class_in_severity() {
        let reg = registry();
        let diag = DiagnosticsConfig {
            select: vec!["all".to_string()],
            severity: {
                let mut m = HashMap::new();
                m.insert("SQLA-9xx".to_string(), "error".to_string());
                m.insert("SQLA-W901".to_string(), "hint".to_string());
                m
            },
            ..Default::default()
        };
        let (resolved, _) = ResolvedDiagnosticsConfig::resolve(&diag, &reg);
        // Class token makes W901 error, then specific code overrides to hint
        assert_eq!(
            resolved.effective_severity("SQLA-W901", &reg),
            Some(Severity::Hint),
            "specific code beats class token"
        );
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
