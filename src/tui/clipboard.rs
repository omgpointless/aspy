//! Clipboard helper for copying text to the system clipboard
//!
//! Uses `arboard` crate for cross-platform support (Windows, macOS, Linux).
//! On WSL, uses `clip.exe` directly to avoid X11/Wayland clipboard manager issues.
//! The clipboard is created fresh each time to avoid holding resources.

use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// Cached WSL detection result
static IS_WSL: OnceLock<bool> = OnceLock::new();

/// Check if running in WSL (Windows Subsystem for Linux)
fn is_wsl() -> bool {
    *IS_WSL.get_or_init(|| {
        // Check for WSL-specific indicators
        std::fs::read_to_string("/proc/version")
            .map(|v| v.to_lowercase().contains("microsoft") || v.to_lowercase().contains("wsl"))
            .unwrap_or(false)
    })
}

/// Copy text to clipboard using Windows clip.exe (for WSL)
fn copy_via_clip_exe(text: &str) -> Result<()> {
    let mut child = Command::new("clip.exe")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to spawn clip.exe")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .context("Failed to write to clip.exe stdin")?;
    }

    let status = child.wait().context("Failed to wait for clip.exe")?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("clip.exe exited with status: {}", status)
    }
}

/// Copy text to clipboard using arboard (native platforms)
fn copy_via_arboard(text: &str) -> Result<()> {
    use arboard::Clipboard;
    let mut clipboard = Clipboard::new().context("Failed to access clipboard")?;
    clipboard
        .set_text(text)
        .context("Failed to set clipboard text")?;
    Ok(())
}

/// Copy text to the system clipboard
///
/// Returns Ok(()) on success, or an error if clipboard access fails.
///
/// On WSL, uses `clip.exe` directly to avoid X11/Wayland clipboard manager
/// issues (the "Could not hand the clipboard contents over" error).
/// On native platforms, uses `arboard` for cross-platform support.
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    if is_wsl() {
        copy_via_clip_exe(text)
    } else {
        copy_via_arboard(text)
    }
}
