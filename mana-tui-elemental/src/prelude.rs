//! prelude module. contains public api for `ratatui-elemental`.
//!
//! # Usage
//!
//! ```
//! use mana_tui_elemental::prelude::*;
//! ```

use std::sync::Arc;

pub use crate::layout::TuiElMarker;
use glam::U16Vec2;
use ratatui::{buffer::Buffer, layout::Rect};

use crate::layout::Props;
pub use crate::layout::{
    Children, ElWidget, Element, ElementCtx, Gap, Height, Justify, MainJustify, Size, Width,
};
pub use ratatui::{
    layout::Direction,
    text::Text,
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
};

pub use crate::ui::*;

/// create element builder.
///
/// an element is a unit in the layout system. elements have children
/// and form a tree that whose layout can be rendered by the context.
///
/// # params
/// - `widget`: the widget to be rendered
///
/// # methods
/// - `create`: construct the element
/// - `width`: sizing along the x axis
/// - `height`: sizing along the y axis,
/// - `direction`: layout direction for children
/// - `padding`: padding around around children
/// - `gap`: gap between children on the main axis
#[bon::builder]
#[builder(finish_fn = create)]
pub fn element<E: ElWidget + 'static>(
    #[builder(start_fn)] widget: E,
    #[builder(finish_fn)] ctx: &mut ElementCtx,
    #[builder(default, overwritable)] width: Size,
    #[builder(default, overwritable)] height: Size,
    #[builder(default, overwritable)] direction: Direction,
    #[builder(default, overwritable)] main_justify: Justify,
    #[builder(overwritable)] padding: Option<Padding>,
    #[builder(default, overwritable)] padding_left: u16,
    #[builder(default, overwritable)] padding_right: u16,
    #[builder(default, overwritable)] padding_top: u16,
    #[builder(default, overwritable)] padding_bottom: u16,
    #[builder(default, overwritable)] gap: u16,
    mut children: Option<Vec<Element>>,
) -> Element {
    let padding = padding.unwrap_or(Padding {
        left: padding_left,
        right: padding_right,
        top: padding_top,
        bottom: padding_bottom,
    });
    fn system<E: ElWidget>(ctx: &ElementCtx, entity: Element, area: Rect, buf: &mut Buffer) {
        if let Ok(widget) = ctx.world.get::<&E>(entity) {
            widget.render_element(area, buf);
        }
    }
    if let Some(ref mut children) = children {
        children.dedup();
    }
    ctx.spawn((
        widget,
        Props {
            size: U16Vec2::default(),
            position: U16Vec2::default(),
            render: system::<E>,
        },
        Width(width),
        Height(height),
        direction,
        MainJustify(main_justify),
        Gap(gap),
        padding,
        Children::Some(Arc::new(children.unwrap_or_default())),
    ))
}

/// wrapper around the element builder that allows modifying the inner paragraph.
/// call [`Self::commit_text`] to consume the text builder and return the element
/// builder.
#[derive(derive_more::Deref, derive_more::DerefMut)]
pub struct ElementTextBuilder<B> {
    builder: B,
    #[deref]
    #[deref_mut]
    paragraph: Paragraph<'static>,
}

impl<'f1, W: ElWidget, S: element_builder::State> ElementBuilder<'f1, W, S> {
    /// wraps the [`ElementBuilder`] in a [`ElementTextBuilder`] that stores a paragraph
    /// and allows the modification of it through its builder methods.
    ///
    /// see [`ElementTextBuilder`] and [`Paragraph`].
    pub fn text(
        self,
        text: impl Into<Text<'static>>,
    ) -> ElementTextBuilder<ElementBuilder<'f1, W, S>> {
        ElementTextBuilder {
            builder: self,
            paragraph: Paragraph::new(text),
        }
    }
}

impl<'f1, W: ElWidget, S: element_builder::State> ElementTextBuilder<ElementBuilder<'f1, W, S>>
where
    S::Children: element_builder::IsUnset,
{
    /// commits the text, creating an element out of the paragraph widget and returns
    /// the original [`ElementBuilder`].
    pub fn commit_text(
        self,
        ctx: &mut ElementCtx,
    ) -> ElementBuilder<'f1, W, impl element_builder::State + use<W, S>> {
        let child = element(self.paragraph)
            .width(Size::Grow)
            .height(Size::Grow)
            .create(ctx);
        self.builder.children(vec![child])
    }
}

/// function for creating [`Block`] structs with sensible defaults around borders.
///
/// this function will create an [`ElementBuilder`] that has its padding set to 1
/// on all sides where the block has a border. this ensures that children elements
/// do not draw over the block's borders.
///
/// see [`element`] for more options.
pub fn block() -> Block<'static> {
    Block::new()
}

/// wip
pub trait BlockExt<'a> {
    /// wip
    fn rounded(self) -> Self;
    /// wip
    fn commit(self) -> ElementBuilder<'a, Block<'a>, element_builder::Empty>;
}

impl BlockExt<'static> for Block<'static> {
    fn rounded(self) -> Self {
        self.borders(Borders::all())
            .border_type(BorderType::Rounded)
    }

    fn commit(self) -> ElementBuilder<'static, Block<'static>, element_builder::Empty> {
        // FIXME: when ratatui exposes `Block::borders`
        let test_area = Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        };
        let inner_area = self.inner(test_area);
        let left = inner_area.left() - test_area.left();
        let top = inner_area.top() - test_area.top();
        let right = (test_area.height - inner_area.height).saturating_sub(1);
        let bottom = (test_area.height - inner_area.height).saturating_sub(1);

        element(self).padding(Padding {
            left,
            right,
            top,
            bottom,
        })
    }
}
