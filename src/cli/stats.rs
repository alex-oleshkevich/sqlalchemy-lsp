use std::path::PathBuf;

use crate::pipeline::build_workspace_index;

#[derive(Debug, clap::Args)]
pub struct StatsArgs {
    /// Files or directories to analyse (default: current directory)
    #[arg()]
    pub paths: Vec<PathBuf>,

    /// Output format: text (default) or json
    #[arg(long = "output-format", default_value = "text")]
    pub format: String,
}

pub fn run_stats(args: StatsArgs) -> i32 {
    let root = if let Some(p) = args.paths.first() {
        if p.is_absolute() { p.clone() } else { std::env::current_dir().unwrap_or_default().join(p) }
    } else {
        std::env::current_dir().unwrap_or_default()
    };

    let state = build_workspace_index(&root);
    let files_checked = state.file_sources.len();

    // Gather stats from the index
    let model_count = state.model_index.len();
    let col_count: usize = state.file_models.iter()
        .flat_map(|e| e.value().iter().map(|m| m.columns.len()).collect::<Vec<_>>())
        .sum();
    let rel_count: usize = state.file_models.iter()
        .flat_map(|e| e.value().iter().map(|m| m.relationships.len()).collect::<Vec<_>>())
        .sum();
    let fk_count: usize = state.file_models.iter()
        .flat_map(|e| {
            e.value().iter()
                .map(|m| m.columns.values().filter(|c| c.foreign_key.is_some()).count())
                .collect::<Vec<_>>()
        })
        .sum();

    // Migration heads: revisions not referenced as a down_revision by any other migration
    use crate::alembic::DownRevision;
    let all_revisions: std::collections::HashSet<String> = state.migration_files.iter()
        .filter_map(|e| e.value().revision.clone())
        .collect();
    let pointed_to: std::collections::HashSet<String> = state.migration_files.iter()
        .flat_map(|e| match &e.value().down_revision {
            DownRevision::None => vec![],
            DownRevision::Single(s) => vec![s.clone()],
            DownRevision::Multiple(v) => v.clone(),
        })
        .collect();
    let head_count = all_revisions.difference(&pointed_to).count();

    // Finding counts by code
    let mut code_counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for entry in state.diagnostics.iter() {
        for diag in entry.value().iter() {
            if let Some(tower_lsp_server::ls_types::NumberOrString::String(code)) = &diag.code {
                *code_counts.entry(code.clone()).or_default() += 1;
            }
        }
    }
    let total_findings: usize = code_counts.values().sum();

    // Model names for summary
    let mut model_names: Vec<String> = state.model_index.iter()
        .map(|e| e.key().clone())
        .collect();
    model_names.sort();

    if args.format == "json" {
        let obj = serde_json::json!({
            "workspace": root.display().to_string(),
            "files_checked": files_checked,
            "models": model_count,
            "columns": col_count,
            "relationships": rel_count,
            "foreign_keys": fk_count,
            "migration_heads": head_count,
            "findings_by_code": code_counts,
            "total_findings": total_findings,
        });
        println!("{}", serde_json::to_string_pretty(&obj).unwrap_or_default());
    } else {
        println!("Workspace: {}   (checked {files_checked} files)", root.display());
        println!();
        let names_str = if model_names.is_empty() {
            "none".to_string()
        } else {
            format!("({})", model_names.join(", "))
        };
        println!("  Models           {model_count:<5} {names_str}");
        println!("  Columns          {col_count}");
        println!("  Relationships    {rel_count}");
        println!("  Foreign keys     {fk_count}");
        println!("  Migration heads  {head_count}");
        println!();
        println!("Findings by code");
        if code_counts.is_empty() {
            println!("  none");
        } else {
            for (code, count) in &code_counts {
                println!("  {code:<12} {count}");
            }
            println!("  {}", "\u{2500}".repeat(13));
            println!("  Total        {total_findings}");
        }
    }

    0
}
