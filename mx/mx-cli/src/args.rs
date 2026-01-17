use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(clap::Parser, Clone, Debug)]
#[command(version, about)]
pub struct MxArgs {
    #[clap(subcommand)]
    pub cmd: MxCommand,
}

#[derive(clap::Args, Clone, Debug, Serialize, Deserialize)]
/// Run an executable
pub struct Serve {
    #[command(flatten)]
    pub args: RunnableArgs,
    #[command(flatten)]
    pub workspace_args: Workspace,
    #[command(flatten)]
    pub features_args: Features,
}

#[derive(clap::Subcommand, Clone, Debug, Serialize, Deserialize)]
pub enum MxCommand {
    Serve(Serve),
    Ipc,
}

#[derive(clap::Args, Clone, Debug, Serialize, Deserialize)]
pub struct RunnableArgs {
    /// The height of the inline tui app view as a percentage of the screen.
    #[arg(
        short = 'y', long,
        default_value = "60%",
        value_parser = parse_percentage
    )]
    pub height: u32,
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

// Portions of this code were adapted from clap-cargo
// https://github.com/crate-ci/clap-cargo
// (c) 2021-2024 clap-cargo developers – MIT/Apache-2.0

#[derive(Default, Clone, Debug, PartialEq, Eq, clap::Args, Serialize, Deserialize)]
#[command(about = None, long_about = None)]
#[non_exhaustive]
pub struct Workspace {
    #[arg(short, long, value_name = "SPEC")]
    /// Package to process (see `cargo help pkgid`)
    pub package: Vec<String>,
    // #[arg(long)]
    // /// Process all packages in the workspace
    // pub workspace: bool,
    // #[arg(long, hide = true)]
    // /// Process all packages in the workspace
    // pub all: bool,
    // #[arg(long, value_name = "SPEC")]
    // /// Exclude packages from being processed
    // pub exclude: Vec<String>,
}

#[derive(Default, Clone, Debug, PartialEq, Eq, clap::Args, Serialize, Deserialize)]
#[command(about = None, long_about = None)]
#[non_exhaustive]
pub struct Features {
    #[arg(long)]
    /// Activate all available features
    pub all_features: bool,
    #[arg(long)]
    /// Do not activate the `default` feature
    pub no_default_features: bool,
    #[arg(short = 'F', long, value_delimiter = ' ')]
    /// Space-separated list of features to activate
    pub features: Vec<String>,
}
