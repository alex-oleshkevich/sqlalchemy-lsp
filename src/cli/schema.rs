use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct SchemaArgs {
    /// Files or directories to render (default: current directory)
    #[arg()]
    pub paths: Vec<PathBuf>,
}

pub fn run_schema(_args: SchemaArgs) -> i32 {
    0
}
