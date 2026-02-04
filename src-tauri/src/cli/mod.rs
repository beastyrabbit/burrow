pub mod output;
pub mod progress;
mod runner;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

pub use runner::run_command;

#[derive(Parser)]
#[command(
    name = "burrow",
    version,
    about = "Fast application launcher for Linux",
    long_about = "Burrow is a fast application launcher with content search capabilities.\n\n\
                  Without any command, Burrow launches the graphical user interface.\n\
                  Use subcommands for direct CLI operations."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Full reindex of all configured directories
    Reindex {
        /// Suppress progress output
        #[arg(short, long)]
        quiet: bool,
    },
    /// Incremental update (only new/modified files)
    Update {
        /// Suppress progress output
        #[arg(short, long)]
        quiet: bool,
    },
    /// Index a single file
    Index {
        /// Path to the file to index
        file: PathBuf,
        /// Force re-index even if file is unchanged
        #[arg(short, long)]
        force: bool,
    },
    /// Check system health (Ollama, Vector DB, API key)
    Health {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show statistics (indexed files, launch count)
    Stats {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Open or show config file
    Config {
        /// Print config path instead of opening
        #[arg(long)]
        path: bool,
    },
    /// Show current indexer progress
    Progress,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_no_args() {
        let cli = Cli::parse_from::<[_; 1], &str>(["burrow"]);
        assert!(cli.command.is_none());
    }

    #[test]
    fn cli_parses_reindex() {
        let cli = Cli::parse_from(["burrow", "reindex"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Reindex { quiet: false })
        ));
    }

    #[test]
    fn cli_parses_reindex_quiet() {
        let cli = Cli::parse_from(["burrow", "reindex", "-q"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Reindex { quiet: true })
        ));
    }

    #[test]
    fn cli_parses_update() {
        let cli = Cli::parse_from(["burrow", "update"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Update { quiet: false })
        ));
    }

    #[test]
    fn cli_parses_index_file() {
        let cli = Cli::parse_from(["burrow", "index", "/tmp/test.md"]);
        if let Some(Commands::Index { file, force }) = cli.command {
            assert_eq!(file, PathBuf::from("/tmp/test.md"));
            assert!(!force);
        } else {
            panic!("Expected Index command");
        }
    }

    #[test]
    fn cli_parses_index_force() {
        let cli = Cli::parse_from(["burrow", "index", "--force", "/tmp/test.md"]);
        if let Some(Commands::Index { file, force }) = cli.command {
            assert_eq!(file, PathBuf::from("/tmp/test.md"));
            assert!(force);
        } else {
            panic!("Expected Index command");
        }
    }

    #[test]
    fn cli_parses_health() {
        let cli = Cli::parse_from(["burrow", "health"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Health { json: false })
        ));
    }

    #[test]
    fn cli_parses_health_json() {
        let cli = Cli::parse_from(["burrow", "health", "--json"]);
        assert!(matches!(cli.command, Some(Commands::Health { json: true })));
    }

    #[test]
    fn cli_parses_stats() {
        let cli = Cli::parse_from(["burrow", "stats"]);
        assert!(matches!(cli.command, Some(Commands::Stats { json: false })));
    }

    #[test]
    fn cli_parses_stats_json() {
        let cli = Cli::parse_from(["burrow", "stats", "--json"]);
        assert!(matches!(cli.command, Some(Commands::Stats { json: true })));
    }

    #[test]
    fn cli_parses_config() {
        let cli = Cli::parse_from(["burrow", "config"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Config { path: false })
        ));
    }

    #[test]
    fn cli_parses_config_path() {
        let cli = Cli::parse_from(["burrow", "config", "--path"]);
        assert!(matches!(cli.command, Some(Commands::Config { path: true })));
    }

    #[test]
    fn cli_parses_progress() {
        let cli = Cli::parse_from(["burrow", "progress"]);
        assert!(matches!(cli.command, Some(Commands::Progress)));
    }
}
