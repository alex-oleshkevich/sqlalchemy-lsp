use std::path::PathBuf;

use crate::{features::schema::render_schema, pipeline::build_workspace_index};

#[derive(Debug, clap::Args)]
pub struct SchemaArgs {
    /// Files or directories to scan (default: current directory)
    #[arg()]
    pub paths: Vec<PathBuf>,

    /// Output format: mermaid (default), graphviz, ascii
    #[arg(long, default_value = "mermaid")]
    pub format: String,

    /// Write output to FILE instead of stdout
    #[arg(long, value_name = "FILE")]
    pub output: Option<PathBuf>,
}

pub fn run_schema(args: SchemaArgs) -> i32 {
    let root = if let Some(p) = args.paths.first() {
        if p.is_absolute() { p.clone() } else { std::env::current_dir().unwrap_or_default().join(p) }
    } else {
        std::env::current_dir().unwrap_or_default()
    };

    let state = build_workspace_index(&root);
    let diagram = render_schema(&state, &args.format);

    match args.output {
        Some(path) => {
            if let Err(e) = std::fs::write(&path, &diagram) {
                eprintln!("error: could not write to {}: {e}", path.display());
                return 1;
            }
        }
        None => print!("{diagram}"),
    }

    0
}
