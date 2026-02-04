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
    /// Toggle window visibility (show if hidden, hide if visible)
    Toggle,
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
    /// Manage the background daemon
    Daemon {
        #[command(subcommand)]
        action: Option<DaemonAction>,
    },
    /// Chat with AI using document context (RAG)
    ChatDocs {
        /// The question or query to ask
        query: String,
        /// Use small/fast chat model instead of large model
        #[arg(long)]
        small: bool,
    },
    /// Chat with AI directly (no document context)
    Chat {
        /// The question or query to ask
        query: String,
        /// Use small/fast chat model instead of large model
        #[arg(long)]
        small: bool,
    },
    /// Manage AI model configuration
    Models {
        #[command(subcommand)]
        action: Option<ModelsAction>,
    },
}

#[derive(Subcommand, Clone)]
pub enum DaemonAction {
    /// Start the daemon (default if no action specified)
    Start {
        /// Run in background (daemonize)
        #[arg(short, long)]
        background: bool,
    },
    /// Stop a running daemon
    Stop,
    /// Check daemon status
    Status,
}

#[derive(Subcommand, Clone)]
pub enum ModelsAction {
    /// List current model configuration
    List,
    /// Set a model interactively
    Set {
        /// Model type to configure: embedding, chat, chat_large
        #[arg(value_name = "MODEL_TYPE")]
        model_type: Option<String>,
    },
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
    fn cli_parses_toggle() {
        let cli = Cli::parse_from(["burrow", "toggle"]);
        assert!(matches!(cli.command, Some(Commands::Toggle)));
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

    #[test]
    fn cli_parses_daemon_no_action() {
        let cli = Cli::parse_from(["burrow", "daemon"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Daemon { action: None })
        ));
    }

    #[test]
    fn cli_parses_daemon_start() {
        let cli = Cli::parse_from(["burrow", "daemon", "start"]);
        if let Some(Commands::Daemon {
            action: Some(DaemonAction::Start { background }),
        }) = cli.command
        {
            assert!(!background);
        } else {
            panic!("Expected Daemon Start command");
        }
    }

    #[test]
    fn cli_parses_daemon_start_background() {
        let cli = Cli::parse_from(["burrow", "daemon", "start", "--background"]);
        if let Some(Commands::Daemon {
            action: Some(DaemonAction::Start { background }),
        }) = cli.command
        {
            assert!(background);
        } else {
            panic!("Expected Daemon Start command with background");
        }
    }

    #[test]
    fn cli_parses_daemon_stop() {
        let cli = Cli::parse_from(["burrow", "daemon", "stop"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Daemon {
                action: Some(DaemonAction::Stop)
            })
        ));
    }

    #[test]
    fn cli_parses_daemon_status() {
        let cli = Cli::parse_from(["burrow", "daemon", "status"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Daemon {
                action: Some(DaemonAction::Status)
            })
        ));
    }

    #[test]
    fn cli_parses_chat_docs() {
        let cli = Cli::parse_from(["burrow", "chat-docs", "What is Rust?"]);
        if let Some(Commands::ChatDocs { query, small }) = cli.command {
            assert_eq!(query, "What is Rust?");
            assert!(!small);
        } else {
            panic!("Expected ChatDocs command");
        }
    }

    #[test]
    fn cli_parses_chat_docs_small() {
        let cli = Cli::parse_from(["burrow", "chat-docs", "--small", "Hello"]);
        if let Some(Commands::ChatDocs { query, small }) = cli.command {
            assert_eq!(query, "Hello");
            assert!(small);
        } else {
            panic!("Expected ChatDocs command");
        }
    }

    #[test]
    fn cli_parses_chat() {
        let cli = Cli::parse_from(["burrow", "chat", "Hello world"]);
        if let Some(Commands::Chat { query, small }) = cli.command {
            assert_eq!(query, "Hello world");
            assert!(!small);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn cli_parses_chat_small() {
        let cli = Cli::parse_from(["burrow", "chat", "--small", "Hi"]);
        if let Some(Commands::Chat { query, small }) = cli.command {
            assert_eq!(query, "Hi");
            assert!(small);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn cli_parses_models_no_action() {
        let cli = Cli::parse_from(["burrow", "models"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Models { action: None })
        ));
    }

    #[test]
    fn cli_parses_models_list() {
        let cli = Cli::parse_from(["burrow", "models", "list"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Models {
                action: Some(ModelsAction::List)
            })
        ));
    }

    #[test]
    fn cli_parses_models_set() {
        let cli = Cli::parse_from(["burrow", "models", "set"]);
        if let Some(Commands::Models {
            action: Some(ModelsAction::Set { model_type }),
        }) = cli.command
        {
            assert!(model_type.is_none());
        } else {
            panic!("Expected Models Set command");
        }
    }

    #[test]
    fn cli_parses_models_set_type() {
        let cli = Cli::parse_from(["burrow", "models", "set", "chat_large"]);
        if let Some(Commands::Models {
            action: Some(ModelsAction::Set { model_type }),
        }) = cli.command
        {
            assert_eq!(model_type, Some("chat_large".into()));
        } else {
            panic!("Expected Models Set command with model type");
        }
    }
}
