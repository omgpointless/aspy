// CLI module - command-line argument parsing and handlers
//
// Provides subcommands for configuration management:
// - config --show: Display effective configuration
// - config --reset: Regenerate config file with defaults
// - config --edit: Open config file in $EDITOR
// - config --update: Merge new defaults into existing config

use crate::config::{Config, VERSION};
use clap::{Parser, Subcommand};
use std::io::Write;
use std::process::Command;

/// Anthropic Spy - Observability proxy for Claude Code
#[derive(Parser)]
#[command(name = "anthropic-spy")]
#[command(version = VERSION)]
#[command(about = "Observability proxy for Claude Code", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Manage configuration
    Config {
        /// Show effective configuration
        #[arg(long)]
        show: bool,

        /// Reset config file to defaults
        #[arg(long)]
        reset: bool,

        /// Open config file in $EDITOR
        #[arg(long)]
        edit: bool,

        /// Update config with new defaults (preserves user values)
        #[arg(long)]
        update: bool,

        /// Show config file path
        #[arg(long)]
        path: bool,
    },
}

/// Handle CLI commands. Returns true if a command was handled (exit after).
pub fn handle_cli() -> bool {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Config {
            show,
            reset,
            edit,
            update,
            path,
        }) => {
            if path {
                handle_config_path();
            } else if show {
                handle_config_show();
            } else if reset {
                handle_config_reset();
            } else if edit {
                handle_config_edit();
            } else if update {
                handle_config_update();
            } else {
                // No flag provided, show help
                println!("Usage: anthropic-spy config [--show|--reset|--edit|--update|--path]");
                println!();
                println!("Options:");
                println!("  --show    Display effective configuration");
                println!("  --reset   Reset config file to defaults");
                println!("  --edit    Open config file in $EDITOR");
                println!("  --update  Update config with new defaults (preserves user values)");
                println!("  --path    Show config file path");
            }
            true
        }
        None => false, // No subcommand, run normal proxy
    }
}

fn handle_config_path() {
    match Config::config_path() {
        Some(path) => println!("{}", path.display()),
        None => {
            eprintln!("Error: Could not determine config path");
            std::process::exit(1);
        }
    }
}

fn handle_config_show() {
    let config = Config::from_env();

    println!("# Effective configuration (env > file > defaults)");
    println!();
    println!("theme = {:?}", config.theme);
    println!("use_theme_background = {}", config.use_theme_background);
    println!("context_limit = {}", config.context_limit);
    println!("bind_addr = {:?}", config.bind_addr.to_string());
    println!("log_dir = {:?}", config.log_dir.display().to_string());
    println!();
    println!("[features]");
    println!("storage = {}", config.features.storage);
    println!("thinking_panel = {}", config.features.thinking_panel);
    println!("stats = {}", config.features.stats);
    println!();
    println!("[augmentation]");
    println!("context_warning = {}", config.augmentation.context_warning);
    println!(
        "context_warning_thresholds = {:?}",
        config.augmentation.context_warning_thresholds
    );

    // Show source info
    println!();
    if let Some(path) = Config::config_path() {
        if path.exists() {
            println!("# Source: {}", path.display());
        } else {
            println!("# Source: defaults (no config file)");
        }
    }
}

fn handle_config_reset() {
    let Some(path) = Config::config_path() else {
        eprintln!("Error: Could not determine config path");
        std::process::exit(1);
    };

    // Confirm if file exists
    if path.exists() {
        eprint!(
            "Config file exists at {}. Overwrite? [y/N] ",
            path.display()
        );
        std::io::stderr().flush().unwrap();

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return;
        }
    }

    // Create parent directory
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("Error creating directory: {}", e);
            std::process::exit(1);
        }
    }

    // Write the default config (using Config's single source of truth)
    if let Err(e) = std::fs::write(&path, Config::default().to_toml()) {
        eprintln!("Error writing config: {}", e);
        std::process::exit(1);
    }

    println!("Config reset to defaults: {}", path.display());
}

fn handle_config_edit() {
    let Some(path) = Config::config_path() else {
        eprintln!("Error: Could not determine config path");
        std::process::exit(1);
    };

    // Ensure config exists
    if !path.exists() {
        Config::ensure_config_exists();
        println!("Created new config file: {}", path.display());
    }

    // Get editor from environment
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            // Platform-specific fallback
            if cfg!(windows) {
                "notepad".to_string()
            } else {
                "nano".to_string()
            }
        });

    println!("Opening {} with {}", path.display(), editor);

    let status = Command::new(&editor).arg(&path).status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("Editor exited with status: {}", s);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to launch editor '{}': {}", editor, e);
            eprintln!("Set $EDITOR environment variable to your preferred editor");
            std::process::exit(1);
        }
    }
}

fn handle_config_update() {
    let Some(path) = Config::config_path() else {
        eprintln!("Error: Could not determine config path");
        std::process::exit(1);
    };

    if !path.exists() {
        // No existing config, just create default
        Config::ensure_config_exists();
        println!("Created new config file: {}", path.display());
        return;
    }

    // Read existing config and generate updated TOML preserving user values
    let existing = Config::from_env();
    let updated = existing.to_toml();

    // Backup existing
    let backup_path = path.with_extension("toml.bak");
    if let Err(e) = std::fs::copy(&path, &backup_path) {
        eprintln!("Warning: Could not create backup: {}", e);
    } else {
        println!("Backup created: {}", backup_path.display());
    }

    // Write updated config
    if let Err(e) = std::fs::write(&path, updated) {
        eprintln!("Error writing config: {}", e);
        std::process::exit(1);
    }

    println!("Config updated with latest structure: {}", path.display());
    println!("Your values have been preserved.");
}
