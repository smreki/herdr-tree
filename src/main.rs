use anyhow::Result;
use clap::Parser;
use herdr_tree::app::{Cli, run};

fn main() -> Result<()> {
    run(Cli::parse())
}
