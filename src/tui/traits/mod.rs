//! Component trait system for TUI architecture
//!
//! This module defines the contracts that UI components implement.
//! Instead of App knowing how to render/scroll/copy for every panel,
//! components declare their own capabilities through traits.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         App                                 │
//! │  (orchestrator: routes events, manages component registry)  │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!              ┌───────────────┼───────────────┐
//!              ▼               ▼               ▼
//!        ┌──────────┐   ┌──────────┐   ┌──────────┐
//!        │  Events  │   │   Logs   │   │ Thinking │
//!        │  Panel   │   │  Panel   │   │  Panel   │
//!        └──────────┘   └──────────┘   └──────────┘
//!              │               │               │
//!              └───────────────┴───────────────┘
//!                              │
//!                     Implements traits:
//!                   Component, Scrollable,
//!                   Copyable, Interactive
//! ```
//!
//! # Traits Overview
//!
//! - [`Component`] - Base trait: render + identity
//! - [`Scrollable`] - Components with scrollable content
//! - [`Copyable`] - Components that provide clipboard content
//! - [`Interactive`] - Components that handle keyboard input
//!
//! # Migration Path
//!
//! 1. Define traits (this module) ✓
//! 2. Extract LogsPanel as first real component
//! 3. Extract EventsPanel
//! 4. Extract ThinkingPanel
//! 5. Thin App to pure orchestrator

mod component;
mod copyable;
mod interactive;
mod scrollable;

pub use component::{Component, ComponentId, RenderContext};
pub use copyable::Copyable;
pub use interactive::{Handled, Interactive};
pub use scrollable::{Scrollable, Selectable};
