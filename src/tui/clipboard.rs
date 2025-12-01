//! Clipboard helper for copying text to the system clipboard
//!
//! Uses `arboard` crate for cross-platform support (Windows, macOS, Linux).
//! The clipboard is created fresh each time to avoid holding resources.

use anyhow::{Context, Result};
use arboard::Clipboard;

/// Copy text to the system clipboard
///
/// Returns Ok(()) on success, or an error if clipboard access fails.
/// Common failure cases: no display server (headless Linux), permission denied.
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new().context("Failed to access clipboard")?;
    clipboard
        .set_text(text)
        .context("Failed to set clipboard text")?;
    Ok(())
}
