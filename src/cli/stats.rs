use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct StatsArgs {
    /// Files or directories to analyse (default: current directory)
    #[arg()]
    pub paths: Vec<PathBuf>,
}

pub fn run_stats(_args: StatsArgs) -> i32 {
    0
}
