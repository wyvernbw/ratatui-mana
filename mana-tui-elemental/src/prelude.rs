//! prelude module. contains public api for `ratatui-elemental`.
//!
//! # Usage
//!
//! ```
//! use mana_tui_elemental::prelude::*;
//! ```

pub use crate::layout::TuiElMarker;

pub use crate::layout::{
    Children, CrossJustify, ElWidget, Element, ElementCtx, Gap, Height, MainJustify, Size, Width,
};
pub use ratatui::{
    layout::Direction,
    text::Text,
    widgets::*,
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
};

#[cfg(feature = "nightly")]
pub use crate::ui::AsChildren;

pub use bon;

pub use crate::layout::Center;
pub use crate::ui::{IntoUiBuilderList, IntoView, IteratorMarker, UiBuilder, View, ui};

/// wip
pub trait BlockExt<'a>: Sized {
    /// wip
    fn into_block(self) -> Block<'a>;

    /// creates a [`Block`] with [`Borders::all()`] and [`BorderType::Rounded`].
    ///
    /// # Example
    ///
    /// ```plain
    /// ╭───────╮
    /// │       │
    /// ╰───────╯
    /// ```
    fn rounded(self) -> Block<'a> {
        self.into_block()
            .borders(Borders::all())
            .border_type(BorderType::Rounded)
    }
}

impl BlockExt<'static> for Block<'static> {
    fn into_block(self) -> Block<'static> {
        self
    }
}
