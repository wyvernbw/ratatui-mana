//! [`mana-tui-elemental`]: mana_tui_elemental
//!
#![doc = include_str!("../readme.md")]

pub use mana_tui_elemental;

pub mod prelude {
    pub use mana_tui_elemental::prelude::*;

    #[cfg(feature = "macros")]
    pub use mana_tui_macros::*;
    #[cfg(feature = "macros")]
    pub extern crate bon;

    pub use ratatui::style::palette;
    pub use ratatui::style::*;
}

pub use mx_core as mx;
