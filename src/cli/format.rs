use std::io::Write;

use owo_colors::OwoColorize;
use serde::Serialize;
use tower_lsp_server::ls_types::DiagnosticSeverity;

use crate::model::types::FixKind;

// ── Reporter enum ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, clap::ValueEnum)]
pub enum Reporter {
    Concise,
    Full,
    Json,
    #[value(name = "json-lines")]
    JsonLines,
    Grouped,
    Github,
    Gitlab,
    Junit,
    Pylint,
}

impl Reporter {
    pub fn is_machine(&self) -> bool {
        matches!(
            self,
            Reporter::Json
                | Reporter::JsonLines
                | Reporter::Github
                | Reporter::Gitlab
                | Reporter::Junit
        )
    }
}

// ── Finding ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Finding {
    pub rel_path: String,
    pub code: String,
    pub message: String,
    pub severity: DiagnosticSeverity,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub fix_kind: FixKind,
    pub source_line: Option<String>,
}

impl Finding {
    pub fn severity_str(&self) -> &'static str {
        match self.severity {
            DiagnosticSeverity::ERROR => "error",
            DiagnosticSeverity::WARNING => "warning",
            DiagnosticSeverity::INFORMATION => "info",
            DiagnosticSeverity::HINT => "hint",
            _ => "warning",
        }
    }
}

// ── Fix summary ───────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct FixResult {
    pub fixed: usize,
    pub remaining: usize,
    pub unsafe_fixable: usize,
}

// ── Main render entry ─────────────────────────────────────────────────────────

pub fn render<W: Write>(
    findings: &[Finding],
    reporter: &Reporter,
    files_checked: usize,
    fix_result: Option<&FixResult>,
    use_color: bool,
    out: &mut W,
) {
    match reporter {
        Reporter::Concise => render_concise(findings, files_checked, fix_result, use_color, out),
        Reporter::Full => render_full(findings, files_checked, fix_result, use_color, out),
        Reporter::Json => render_json(findings, out),
        Reporter::JsonLines => render_json_lines(findings, out),
        Reporter::Grouped => render_grouped(findings, files_checked, fix_result, use_color, out),
        Reporter::Github => render_github(findings, out),
        Reporter::Gitlab => render_gitlab(findings, out),
        Reporter::Junit => render_junit(findings, out),
        Reporter::Pylint => render_pylint(findings, files_checked, fix_result, use_color, out),
    }
}

// ── Summary helpers ───────────────────────────────────────────────────────────

fn count_by_severity(findings: &[Finding]) -> (usize, usize, usize, usize) {
    let mut errors = 0usize;
    let mut warnings = 0usize;
    let mut infos = 0usize;
    let mut hints = 0usize;
    for f in findings {
        match f.severity {
            DiagnosticSeverity::ERROR => errors += 1,
            DiagnosticSeverity::WARNING => warnings += 1,
            DiagnosticSeverity::INFORMATION => infos += 1,
            _ => hints += 1,
        }
    }
    (errors, warnings, infos, hints)
}

fn write_summary<W: Write>(
    findings: &[Finding],
    files_checked: usize,
    fix_result: Option<&FixResult>,
    use_color: bool,
    out: &mut W,
) {
    if findings.is_empty() && fix_result.is_none() {
        let _ = writeln!(out, "All checks passed! (checked {files_checked} files)");
        return;
    }

    let safe_fixable = findings
        .iter()
        .filter(|f| matches!(f.fix_kind, FixKind::Safe))
        .count();
    let unsafe_fixable = findings
        .iter()
        .filter(|f| matches!(f.fix_kind, FixKind::Unsafe))
        .count();

    if safe_fixable > 0 {
        let _ = writeln!(out, "[*] {safe_fixable} fixable with the `--fix` option.");
    }
    if unsafe_fixable > 0 {
        let _ = writeln!(out, "[*] {unsafe_fixable} fixable with `--fix --unsafe`.");
    }

    if !findings.is_empty() {
        let (errors, warnings, infos, hints) = count_by_severity(findings);
        let mut parts: Vec<String> = Vec::new();
        if errors > 0 {
            let label = if use_color {
                "errors".red().bold().to_string()
            } else {
                "errors".to_string()
            };
            parts.push(format!("{errors} {label}"));
        }
        if warnings > 0 {
            let label = if use_color {
                "warnings".yellow().bold().to_string()
            } else {
                "warnings".to_string()
            };
            parts.push(format!("{warnings} {label}"));
        }
        if infos > 0 {
            let label = if use_color {
                "info".blue().to_string()
            } else {
                "info".to_string()
            };
            parts.push(format!("{infos} {label}"));
        }
        if hints > 0 {
            let label = if use_color {
                "hints".dimmed().to_string()
            } else {
                "hints".to_string()
            };
            parts.push(format!("{hints} {label}"));
        }
        let n = findings.len();
        let files: std::collections::HashSet<&str> =
            findings.iter().map(|f| f.rel_path.as_str()).collect();
        let problem_word = if n == 1 { "problem" } else { "problems" };
        let _ = writeln!(
            out,
            "Found {n} {problem_word} ({}) in {} files (checked {files_checked} files).",
            parts.join(", "),
            files.len()
        );
    }

    if let Some(fr) = fix_result {
        let rem = fr.remaining;
        if fr.unsafe_fixable > 0 {
            let _ = writeln!(
                out,
                "Fixed {} problem{}; {rem} remaining ({} fixable with --unsafe).",
                fr.fixed,
                if fr.fixed == 1 { "" } else { "s" },
                fr.unsafe_fixable
            );
        } else {
            let _ = writeln!(
                out,
                "Fixed {} problem{}; {rem} remaining.",
                fr.fixed,
                if fr.fixed == 1 { "" } else { "s" }
            );
        }
    }
}

// ── color helpers ─────────────────────────────────────────────────────────────

fn sev_colored_code(code: &str, severity: DiagnosticSeverity, use_color: bool) -> String {
    if !use_color {
        return code.to_string();
    }
    match severity {
        DiagnosticSeverity::ERROR => code.red().bold().to_string(),
        DiagnosticSeverity::WARNING => code.yellow().bold().to_string(),
        DiagnosticSeverity::HINT => code.dimmed().to_string(),
        _ => code.yellow().to_string(),
    }
}

fn sev_colored_carets(carets: &str, severity: DiagnosticSeverity) -> String {
    match severity {
        DiagnosticSeverity::ERROR => carets.red().to_string(),
        DiagnosticSeverity::WARNING => carets.yellow().to_string(),
        DiagnosticSeverity::HINT => carets.dimmed().to_string(),
        _ => carets.yellow().to_string(),
    }
}

// ── concise ───────────────────────────────────────────────────────────────────

fn render_concise<W: Write>(
    findings: &[Finding],
    files_checked: usize,
    fix_result: Option<&FixResult>,
    use_color: bool,
    out: &mut W,
) {
    for f in findings {
        let code_str = sev_colored_code(f.code.as_str(), f.severity, use_color);
        let _ = writeln!(
            out,
            "{}:{}:{}: {} {}",
            f.rel_path, f.line, f.col, code_str, f.message
        );
    }
    write_summary(findings, files_checked, fix_result, use_color, out);
}

// ── pylint ────────────────────────────────────────────────────────────────────

fn render_pylint<W: Write>(
    findings: &[Finding],
    files_checked: usize,
    fix_result: Option<&FixResult>,
    use_color: bool,
    out: &mut W,
) {
    for f in findings {
        let _ = writeln!(out, "{}:{}: [{}] {}", f.rel_path, f.line, f.code, f.message);
    }
    write_summary(findings, files_checked, fix_result, use_color, out);
}

// ── full ──────────────────────────────────────────────────────────────────────

fn render_full<W: Write>(
    findings: &[Finding],
    files_checked: usize,
    fix_result: Option<&FixResult>,
    use_color: bool,
    out: &mut W,
) {
    for f in findings {
        let line_str = f.line.to_string();
        let pad = " ".repeat(line_str.len());

        let code_str = sev_colored_code(f.code.as_str(), f.severity, use_color);
        let msg_str = if use_color {
            f.message.as_str().bold().to_string()
        } else {
            f.message.clone()
        };
        let arrow = if use_color { "-->".blue().to_string() } else { "-->".to_string() };
        let pipe = if use_color { "|".blue().to_string() } else { "|".to_string() };
        let line_num_colored = if use_color {
            line_str.as_str().blue().to_string()
        } else {
            line_str.clone()
        };

        let _ = writeln!(out, "{code_str}: {msg_str}");
        let _ = writeln!(out, "{pad} {arrow} {}:{}:{}", f.rel_path, f.line, f.col);
        let _ = writeln!(out, "{pad} {pipe}");
        if let Some(src) = &f.source_line {
            let _ = writeln!(out, "{line_num_colored} {pipe} {src}");
            let span_width = (f.end_col.saturating_sub(f.col) as usize).max(1);
            let indent = f.col.saturating_sub(1) as usize;
            let raw_carets = "^".repeat(span_width);
            let carets_str = if use_color {
                sev_colored_carets(&raw_carets, f.severity)
            } else {
                raw_carets
            };
            let _ = writeln!(out, "{pad} {pipe} {}{}", " ".repeat(indent), carets_str);
        }
        let _ = writeln!(out, "{pad} {pipe}");
        if let Some(help) = help_for_code(&f.code) {
            let help_label = if use_color {
                "help".yellow().bold().to_string()
            } else {
                "help".to_string()
            };
            let help_text = if use_color {
                help.white().bold().to_string()
            } else {
                help.to_string()
            };
            let _ = writeln!(out, "{pad} = {help_label}: {help_text}");
        }
        let note_label = if use_color {
            "note".yellow().bold().to_string()
        } else {
            "note".to_string()
        };
        let _ = writeln!(
            out,
            "{pad} = {note_label}: disable with `# noqa: {}` or in [tool.sqlalchemy-lsp]",
            f.code
        );
        let _ = writeln!(out);
    }
    write_summary(findings, files_checked, fix_result, use_color, out);
}

fn help_for_code(code: &str) -> Option<&'static str> {
    match code {
        "SQLA-W101" => Some("add `__tablename__ = \"table_name\"` to the class body"),
        "SQLA-W303" => Some("align the column type with the FK target, or change the target"),
        "SQLA-W402" => {
            Some("update `back_populates` to match the attribute name on the other side")
        }
        "SQLA-W403" => Some("add a matching `back_populates` on the target model"),
        "SQLA-W409" => {
            Some("valid cascade values: all, save-update, merge, expunge, delete, delete-orphan")
        }
        _ => None,
    }
}

// ── grouped ───────────────────────────────────────────────────────────────────

fn render_grouped<W: Write>(
    findings: &[Finding],
    files_checked: usize,
    fix_result: Option<&FixResult>,
    use_color: bool,
    out: &mut W,
) {
    let mut by_file: std::collections::BTreeMap<&str, Vec<&Finding>> =
        std::collections::BTreeMap::new();
    for f in findings {
        by_file.entry(f.rel_path.as_str()).or_default().push(f);
    }
    for (file, file_findings) in &by_file {
        let file_header = if use_color {
            file.bold().to_string()
        } else {
            file.to_string()
        };
        let _ = writeln!(out, "{file_header}");
        for f in file_findings {
            let code_str = sev_colored_code(f.code.as_str(), f.severity, use_color);
            let _ = writeln!(out, "  {}:{}: {} {}", f.line, f.col, code_str, f.message);
        }
    }
    write_summary(findings, files_checked, fix_result, use_color, out);
}

// ── json ──────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct JsonFinding<'a> {
    code: &'a str,
    message: &'a str,
    location: JsonLocation,
    end_location: JsonLocation,
    filename: &'a str,
    severity: &'a str,
    fix: Option<JsonFix<'a>>,
}

#[derive(Serialize)]
struct JsonLocation {
    row: u32,
    column: u32,
}

#[derive(Serialize)]
struct JsonFix<'a> {
    applicability: &'a str,
}

fn to_json_finding(f: &Finding) -> JsonFinding<'_> {
    let fix = match f.fix_kind {
        FixKind::Safe => Some(JsonFix {
            applicability: "safe",
        }),
        FixKind::Unsafe => Some(JsonFix {
            applicability: "unsafe",
        }),
        FixKind::None => None,
    };
    JsonFinding {
        code: &f.code,
        message: &f.message,
        location: JsonLocation {
            row: f.line,
            column: f.col,
        },
        end_location: JsonLocation {
            row: f.end_line,
            column: f.end_col,
        },
        filename: &f.rel_path,
        severity: f.severity_str(),
        fix,
    }
}

fn render_json<W: Write>(findings: &[Finding], out: &mut W) {
    let jf: Vec<_> = findings.iter().map(to_json_finding).collect();
    let _ = writeln!(
        out,
        "{}",
        serde_json::to_string_pretty(&jf).unwrap_or_default()
    );
}

fn render_json_lines<W: Write>(findings: &[Finding], out: &mut W) {
    for f in findings {
        let jf = to_json_finding(f);
        let _ = writeln!(out, "{}", serde_json::to_string(&jf).unwrap_or_default());
    }
}

// ── github ────────────────────────────────────────────────────────────────────

fn render_github<W: Write>(findings: &[Finding], out: &mut W) {
    for f in findings {
        let level = match f.severity {
            DiagnosticSeverity::ERROR => "error",
            _ => "warning",
        };
        let _ = writeln!(
            out,
            "::{level} title=sqlalchemy-lsp ({}),file={},line={},col={}::{}",
            f.code, f.rel_path, f.line, f.col, f.message
        );
    }
}

// ── gitlab ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct GitlabFinding<'a> {
    check_name: &'a str,
    description: &'a str,
    severity: &'static str,
    fingerprint: String,
    location: GitlabLocation<'a>,
}

#[derive(Serialize)]
struct GitlabLocation<'a> {
    path: &'a str,
    lines: GitlabLines,
}

#[derive(Serialize)]
struct GitlabLines {
    begin: u32,
}

fn render_gitlab<W: Write>(findings: &[Finding], out: &mut W) {
    let jf: Vec<_> = findings
        .iter()
        .map(|f| {
            let gl_severity = match f.severity {
                DiagnosticSeverity::ERROR => "critical",
                DiagnosticSeverity::INFORMATION => "info",
                DiagnosticSeverity::HINT => "info",
                _ => "minor",
            };
            let fingerprint = format!("{:x}", md5_fingerprint(&f.rel_path, &f.code, f.line));
            GitlabFinding {
                check_name: &f.code,
                description: &f.message,
                severity: gl_severity,
                fingerprint,
                location: GitlabLocation {
                    path: &f.rel_path,
                    lines: GitlabLines { begin: f.line },
                },
            }
        })
        .collect();
    let _ = writeln!(
        out,
        "{}",
        serde_json::to_string_pretty(&jf).unwrap_or_default()
    );
}

fn md5_fingerprint(path: &str, code: &str, line: u32) -> u64 {
    // Simple stable hash for the fingerprint — not cryptographic
    let mut h: u64 = 0xcbf29ce484222325;
    for byte in format!("{path}:{code}:{line}").bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

// ── junit ─────────────────────────────────────────────────────────────────────

fn render_junit<W: Write>(findings: &[Finding], out: &mut W) {
    let _ = writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    let _ = writeln!(
        out,
        r#"<testsuite name="sqlalchemy-lsp" tests="{}" failures="{}">"#,
        findings.len(),
        findings.len()
    );
    for f in findings {
        let classname = f.rel_path.replace(['/', '\\'], ".");
        let _ = writeln!(
            out,
            r#"  <testcase classname="{classname}" name="{}">"#,
            f.code
        );
        let _ = writeln!(
            out,
            r#"    <failure message="{}" type="{}">{}: {}:{}: {}</failure>"#,
            escape_xml(&f.message),
            f.code,
            f.code,
            f.rel_path,
            f.line,
            escape_xml(&f.message)
        );
        let _ = writeln!(out, "  </testcase>");
    }
    let _ = writeln!(out, "</testsuite>");
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp_server::ls_types::DiagnosticSeverity;

    fn sample() -> Finding {
        Finding {
            rel_path: "models/post.py".to_string(),
            code: "SQLA-W303".to_string(),
            message: "FK type mismatch: `author_id` is Mapped[str] but `users.id` is Integer"
                .to_string(),
            severity: DiagnosticSeverity::WARNING,
            line: 14,
            col: 5,
            end_line: 14,
            end_col: 14,
            fix_kind: FixKind::None,
            source_line: Some(
                "    author_id: Mapped[str] = mapped_column(ForeignKey(\"users.id\"))".to_string(),
            ),
        }
    }

    fn render_to_string(findings: &[Finding], reporter: &Reporter) -> String {
        let mut buf: Vec<u8> = Vec::new();
        render(findings, reporter, 42, None, false, &mut buf);
        String::from_utf8(buf).unwrap()
    }

    // ── REQ-CLI-08: concise line format ──────────────────────────────────────

    #[test]
    fn req_cli_08_concise_line_format() {
        let out = render_to_string(&[sample()], &Reporter::Concise);
        assert!(
            out.contains("models/post.py:14:5: SQLA-W303"),
            "concise line: {out}"
        );
    }

    // ── REQ-CLI-09: summary wording ──────────────────────────────────────────

    #[test]
    fn req_cli_09_summary_clean() {
        let mut buf: Vec<u8> = Vec::new();
        render(&[], &Reporter::Concise, 42, None, false, &mut buf);
        let out = String::from_utf8(buf).unwrap();
        assert_eq!(out.trim(), "All checks passed! (checked 42 files)");
    }

    #[test]
    fn req_cli_09_summary_with_findings() {
        let out = render_to_string(&[sample()], &Reporter::Concise);
        assert!(out.contains("Found 1 problem"), "summary: {out}");
        assert!(out.contains("1 warnings"), "severity: {out}");
        assert!(out.contains("checked 42 files"), "files: {out}");
    }

    #[test]
    fn req_cli_09_summary_fixable_hints() {
        let safe = Finding {
            fix_kind: FixKind::Safe,
            ..sample()
        };
        let unsafe_ = Finding {
            fix_kind: FixKind::Unsafe,
            ..sample()
        };
        let findings = vec![safe, unsafe_];
        let out = render_to_string(&findings, &Reporter::Concise);
        assert!(
            out.contains("[*] 1 fixable with the `--fix` option."),
            "safe hint: {out}"
        );
        assert!(
            out.contains("[*] 1 fixable with `--fix --unsafe`."),
            "unsafe hint: {out}"
        );
    }

    #[test]
    fn req_cli_09_machine_formats_no_summary() {
        let out = render_to_string(&[sample()], &Reporter::Json);
        assert!(!out.contains("Found"), "json no summary: {out}");
        let out = render_to_string(&[sample()], &Reporter::Github);
        assert!(!out.contains("Found"), "github no summary: {out}");
    }

    // ── REQ-CLI-07: nine reporters emit correct shapes ────────────────────────

    #[test]
    fn req_cli_07_full_reporter() {
        let out = render_to_string(&[sample()], &Reporter::Full);
        assert!(
            out.contains("SQLA-W303: FK type mismatch"),
            "full header: {out}"
        );
        assert!(
            out.contains("--> models/post.py:14:5"),
            "full location: {out}"
        );
        assert!(out.contains("# noqa: SQLA-W303"), "full note: {out}");
    }

    #[test]
    fn req_cli_07_json_reporter() {
        let out = render_to_string(&[sample()], &Reporter::Json);
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["code"], "SQLA-W303");
        assert_eq!(arr[0]["location"]["row"], 14);
        assert_eq!(arr[0]["location"]["column"], 5);
        assert_eq!(arr[0]["severity"], "warning");
    }

    #[test]
    fn req_cli_07_json_empty_array() {
        let out = render_to_string(&[], &Reporter::Json);
        let parsed: serde_json::Value = serde_json::from_str(out.trim()).expect("valid json");
        assert!(parsed.as_array().unwrap().is_empty());
    }

    #[test]
    fn req_cli_07_json_lines_reporter() {
        let out = render_to_string(&[sample()], &Reporter::JsonLines);
        let lines: Vec<&str> = out.trim().lines().collect();
        assert_eq!(lines.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).expect("valid ndjson");
        assert_eq!(parsed["code"], "SQLA-W303");
    }

    #[test]
    fn req_cli_07_grouped_reporter() {
        let out = render_to_string(&[sample()], &Reporter::Grouped);
        assert!(out.contains("models/post.py"), "file header: {out}");
        assert!(out.contains("  14:5: SQLA-W303"), "indented finding: {out}");
    }

    #[test]
    fn req_cli_07_github_reporter() {
        let out = render_to_string(&[sample()], &Reporter::Github);
        assert!(
            out.starts_with("::warning title=sqlalchemy-lsp (SQLA-W303)"),
            "github: {out}"
        );
        assert!(
            out.contains("file=models/post.py,line=14,col=5"),
            "github loc: {out}"
        );
    }

    #[test]
    fn req_cli_07_gitlab_reporter() {
        let out = render_to_string(&[sample()], &Reporter::Gitlab);
        let parsed: serde_json::Value = serde_json::from_str(out.trim()).expect("valid json");
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr[0]["check_name"], "SQLA-W303");
        assert_eq!(arr[0]["severity"], "minor");
    }

    #[test]
    fn req_cli_07_junit_reporter() {
        let out = render_to_string(&[sample()], &Reporter::Junit);
        assert!(out.contains(r#"<?xml version="1.0""#), "xml header: {out}");
        assert!(out.contains("<testsuite"), "testsuite: {out}");
        assert!(out.contains("<failure"), "failure: {out}");
    }

    #[test]
    fn req_cli_07_pylint_reporter() {
        let out = render_to_string(&[sample()], &Reporter::Pylint);
        assert!(
            out.contains("models/post.py:14: [SQLA-W303]"),
            "pylint: {out}"
        );
    }
}
