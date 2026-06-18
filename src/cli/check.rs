use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct CheckArgs {
    /// Files or directories to check (default: current directory)
    #[arg()]
    pub paths: Vec<PathBuf>,
}

pub fn run_check(_args: CheckArgs) -> i32 {
    0
}
