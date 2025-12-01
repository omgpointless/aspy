// CLI module - command-line argument parsing and handlers
//
// Provides subcommands for configuration management:
// - config --show: Display effective configuration
// - config --reset: Regenerate config file with defaults
// - config --edit: Open config file in $EDITOR
// - config --update: Merge new defaults into existing config (with diff preview)
// - config --init: Interactive setup wizard

use crate::config::{Config, VERSION};
use crate::theme::list_bundled_themes;
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

        /// Update config with new defaults (preserves user values, shows diff)
        #[arg(long)]
        update: bool,

        /// Show config file path
        #[arg(long)]
        path: bool,

        /// Interactive setup wizard
        #[arg(long)]
        init: bool,
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
            init,
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
            } else if init {
                handle_config_init();
            } else {
                // No flag provided, show help
                println!("Usage: anthropic-spy config [OPTIONS]");
                println!();
                println!("Options:");
                println!("  --init    Interactive setup wizard (recommended for first-time setup)");
                println!("  --show    Display effective configuration");
                println!("  --edit    Open config file in $EDITOR");
                println!("  --update  Update config structure (preserves values, shows diff)");
                println!("  --reset   Reset config file to defaults");
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

    // Read current file content
    let current_content = std::fs::read_to_string(&path).unwrap_or_default();

    // Read existing config and generate updated TOML preserving user values
    let existing = Config::from_env();
    let updated_content = existing.to_toml();

    // Check if there are any changes
    if current_content.trim() == updated_content.trim() {
        println!("Config is already up to date. No changes needed.");
        return;
    }

    // Show diff
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("                              CONFIG DIFF PREVIEW");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    show_diff(&current_content, &updated_content);
    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    // Ask for confirmation
    eprint!("Apply these changes? [y/N] ");
    std::io::stderr().flush().unwrap();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    if !input.trim().eq_ignore_ascii_case("y") {
        println!("Aborted. No changes made.");
        return;
    }

    // Backup existing
    let backup_path = path.with_extension("toml.bak");
    if let Err(e) = std::fs::copy(&path, &backup_path) {
        eprintln!("Warning: Could not create backup: {}", e);
    } else {
        println!("Backup created: {}", backup_path.display());
    }

    // Write updated config
    if let Err(e) = std::fs::write(&path, updated_content) {
        eprintln!("Error writing config: {}", e);
        std::process::exit(1);
    }

    println!("✓ Config updated: {}", path.display());
}

/// Show a simple line-by-line diff between old and new content
fn show_diff(old: &str, new: &str) {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    // Simple diff: show removed lines, then added lines for changed sections
    let max_lines = old_lines.len().max(new_lines.len());

    for i in 0..max_lines {
        let old_line = old_lines.get(i).copied().unwrap_or("");
        let new_line = new_lines.get(i).copied().unwrap_or("");

        if old_line == new_line {
            println!("  {}", new_line);
        } else if old_line.is_empty() {
            println!("\x1b[32m+ {}\x1b[0m", new_line); // Green for additions
        } else if new_line.is_empty() {
            println!("\x1b[31m- {}\x1b[0m", old_line); // Red for removals
        } else {
            println!("\x1b[31m- {}\x1b[0m", old_line); // Red for old
            println!("\x1b[32m+ {}\x1b[0m", new_line); // Green for new
        }
    }
}

/// Interactive setup wizard for first-time configuration
fn handle_config_init() {
    let path = match Config::config_path() {
        Some(p) => p,
        None => {
            eprintln!("Error: Could not determine config path");
            std::process::exit(1);
        }
    };

    // Check if config exists
    if path.exists() {
        eprint!("Config already exists at {}. Overwrite? [y/N] ", path.display());
        std::io::stderr().flush().unwrap();

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted. Use --edit to modify existing config.");
            return;
        }
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("                    ANTHROPIC-SPY CONFIGURATION WIZARD");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    // Start with defaults
    let mut config = Config::default();

    // ─────────────────────────────────────────────────────────────────────────
    // Theme Selection
    // ─────────────────────────────────────────────────────────────────────────
    println!("┌─ THEME SELECTION ─────────────────────────────────────────────────────────┐");
    println!("│ Choose a color theme for the TUI. You can change this anytime with 't'.  │");
    println!("└────────────────────────────────────────────────────────────────────────────┘");
    println!();

    let themes = list_bundled_themes();
    let popular_themes = [
        "Spy Dark",
        "Spy Light",
        "Dracula",
        "Catppuccin Mocha",
        "Tokyo Night",
        "Nord",
        "Gruvbox Dark",
        "Monokai Pro",
    ];

    println!("Popular themes:");
    for (i, theme) in popular_themes.iter().enumerate() {
        let marker = if *theme == "Spy Dark" { " (default)" } else { "" };
        println!("  {}. {}{}", i + 1, theme, marker);
    }
    println!("  9. Show all {} themes", themes.len());
    println!();

    eprint!("Select theme [1-9, or Enter for default]: ");
    std::io::stderr().flush().unwrap();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let input = input.trim();

    if !input.is_empty() {
        if input == "9" {
            // Show all themes
            println!();
            println!("All available themes:");
            for (i, theme) in themes.iter().enumerate() {
                println!("  {:2}. {}", i + 1, theme);
            }
            println!();
            eprint!("Select theme number [1-{}]: ", themes.len());
            std::io::stderr().flush().unwrap();

            let mut input2 = String::new();
            std::io::stdin().read_line(&mut input2).unwrap();
            if let Ok(num) = input2.trim().parse::<usize>() {
                if num >= 1 && num <= themes.len() {
                    config.theme = themes[num - 1].to_string();
                }
            }
        } else if let Ok(num) = input.parse::<usize>() {
            if num >= 1 && num <= popular_themes.len() {
                config.theme = popular_themes[num - 1].to_string();
            }
        }
    }

    println!();
    println!("✓ Theme: {}", config.theme);
    println!();

    // ─────────────────────────────────────────────────────────────────────────
    // Bind Address
    // ─────────────────────────────────────────────────────────────────────────
    println!("┌─ PROXY SETTINGS ─────────────────────────────────────────────────────────┐");
    println!("│ Configure the proxy server that Claude Code will connect to.             │");
    println!("└────────────────────────────────────────────────────────────────────────────┘");
    println!();

    println!("Bind address (where the proxy listens)");
    eprint!("  [Enter for {}]: ", config.bind_addr);
    std::io::stderr().flush().unwrap();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let input = input.trim();

    if !input.is_empty() {
        if let Ok(addr) = input.parse() {
            config.bind_addr = addr;
        } else {
            println!("  Invalid address, keeping default");
        }
    }

    println!("✓ Bind address: {}", config.bind_addr);
    println!();

    // ─────────────────────────────────────────────────────────────────────────
    // Features
    // ─────────────────────────────────────────────────────────────────────────
    println!("┌─ FEATURES ────────────────────────────────────────────────────────────────┐");
    println!("│ Toggle optional features. All are enabled by default.                    │");
    println!("└────────────────────────────────────────────────────────────────────────────┘");
    println!();

    config.features.storage = prompt_bool("Enable session logging (JSONL files)?", true);
    config.features.thinking_panel = prompt_bool("Enable thinking panel (Claude's reasoning)?", true);
    config.features.stats = prompt_bool("Enable stats tracking (tokens, costs)?", true);

    println!();

    // ─────────────────────────────────────────────────────────────────────────
    // Write config
    // ─────────────────────────────────────────────────────────────────────────

    // Create parent directory
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("Error creating directory: {}", e);
            std::process::exit(1);
        }
    }

    // Write config
    if let Err(e) = std::fs::write(&path, config.to_toml()) {
        eprintln!("Error writing config: {}", e);
        std::process::exit(1);
    }

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("✓ Configuration saved to: {}", path.display());
    println!();
    println!("Next steps:");
    println!("  1. Set environment variable in your shell:");
    println!("     export ANTHROPIC_BASE_URL=http://{}", config.bind_addr);
    println!();
    println!("  2. Run anthropic-spy:");
    println!("     anthropic-spy");
    println!();
    println!("  3. Use Claude Code as normal - all traffic will be proxied through the TUI");
    println!();
}

/// Prompt for a yes/no boolean with default
fn prompt_bool(question: &str, default: bool) -> bool {
    let default_str = if default { "Y/n" } else { "y/N" };
    eprint!("  {} [{}]: ", question, default_str);
    std::io::stderr().flush().unwrap();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let input = input.trim().to_lowercase();

    if input.is_empty() {
        default
    } else {
        input == "y" || input == "yes"
    }
}
