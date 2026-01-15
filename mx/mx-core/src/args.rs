use std::path::PathBuf;

use color_eyre::{Result, eyre::bail};

#[derive(clap::Parser, Clone, Debug)]
pub struct MxArgs {
    /// The height of the inline tui app view as a percentage of the screen.
    #[clap(
        short = 'y', long,
        default_value = "60%",
        value_parser = parse_percentage
    )]
    pub height: u32,
    #[clap(subcommand)]
    pub cmd: MxCommand,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum MxCommand {
    /// Run an executable
    Run { path: PathBuf },
}

impl MxArgs {
    pub fn parse() -> Self {
        clap::Parser::parse()
    }
}

fn parse_percentage(val: &str) -> Result<u32> {
    let val = val.trim_end_matches("%");
    let num: u32 = val.parse()?;
    if !(0..=100).contains(&num) {
        bail!("Percentage must be between 0 and 100");
    }
    Ok(num)
}
