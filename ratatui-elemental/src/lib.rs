//! # ratatui-elemental
//!
//! ratatui layout library

#![forbid(missing_docs)]
#![feature(negative_impls)]
#![feature(alloc_layout_extra)]
#![feature(ptr_as_ref_unchecked)]
#![feature(downcast_unchecked)]

pub(crate) mod layout;
mod typemap;

/// prelude module. contains public api for `ratatui-elemental`.
///
/// # Usage
///
/// ```
/// use ratatui_elemental::prelude::*;
/// ```
pub mod prelude {
    use ratatui::{
        layout::{Direction, Rect},
        text::Text,
        widgets::{Block, BorderType, Borders, Padding, Paragraph},
    };

    use crate::layout::{ElWidget, ElementCtx, ElementKey, Justify, LayoutParams, Size};

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
    pub fn element(
        #[builder(start_fn)] widget: impl ElWidget + 'static,
        #[builder(finish_fn)] ctx: &mut ElementCtx,
        #[builder(overwritable)] layout_params: Option<LayoutParams>,
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
        children: Option<Vec<ElementKey>>,
    ) -> ElementKey {
        let layout_params = layout_params.unwrap_or(LayoutParams {
            width,
            height,
            direction,
            main_justify,
            padding: padding.unwrap_or(Padding {
                left: padding_left,
                right: padding_right,
                top: padding_top,
                bottom: padding_bottom,
            }),
            gap,
        });
        ElementCtx::element(widget)
            .maybe_children(children)
            .layout_params(layout_params)
            .create(ctx)
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

    /// marker trait implemented for any `ElementBuilder`.
    ///
    /// it is used to extend builders with other methods. you can also use it to target
    /// implementations of extension traits for your own needs.
    ///
    /// ```
    /// use ratatui_elemental::prelude::*;
    ///
    /// trait MyElementExt {
    ///     fn foo(&self);
    /// }
    ///
    /// impl<T: ElementalBuilder> MyElementExt for T {
    ///     # fn foo(&self) {}
    ///     /* ... */
    /// }
    /// ```
    pub trait ElementalBuilder {}

    impl<'f1, W: ElWidget, S: element_builder::State> ElementalBuilder for ElementBuilder<'f1, W, S> {}
}

#[cfg(test)]
mod tests {
    use ratatui::{
        buffer::Buffer,
        layout::{Direction, Rect},
        style::{Color, Stylize},
        widgets::{Block, BorderType, Padding, Paragraph},
    };

    use crate::{
        layout::{ElementCtx, Justify, LayoutParams, Size},
        prelude::{BlockExt, block},
    };

    fn buffer_to_string(buf: &Buffer) -> String {
        buf.content()
            .chunks(buf.area.width as usize)
            .flat_map(|line| line.iter().map(|cell| cell.symbol()).chain(["\n"]))
            .collect()
    }
    #[test]
    fn test_fixed_size() {
        let _ = tracing_subscriber::fmt::try_init();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 10));
        let mut ctx = ElementCtx::default();
        let root = ElementCtx::element(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .fg(Color::Red),
        )
        .layout_params(LayoutParams {
            width: Size::Fixed(24),
            height: Size::Fixed(8),
            ..Default::default()
        })
        .create(&mut ctx);
        ctx.calculate_layout(root);
        ctx.render(root, buf.area, &mut buf);
        tracing::info!("\ntest_fixed_size\n{}", buffer_to_string(&buf));
    }

    #[test]
    fn test_fixed_size_with_children() {
        _ = tracing_subscriber::fmt::try_init();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 10));
        let mut ctx = ElementCtx::default();
        let root = block()
            .rounded()
            .title_top("parent")
            .fg(Color::Red)
            .commit()
            .children(vec![
                block()
                    .rounded()
                    .title_top("child #0".to_string())
                    .commit()
                    .width(Size::Fixed(10))
                    .height(Size::Fixed(3))
                    .create(&mut ctx),
                block()
                    .rounded()
                    .title_top("child #1".to_string())
                    .commit()
                    .width(Size::Fixed(14))
                    .height(Size::Fixed(3))
                    .create(&mut ctx),
            ])
            .width(Size::Fixed(24))
            .height(Size::Fixed(8))
            .create(&mut ctx);

        ctx.calculate_layout(root);
        ctx.render(root, buf.area, &mut buf);
        tracing::info!(
            "\ntest_fixed_size_with_children\n{}",
            buffer_to_string(&buf)
        );
    }

    #[test]
    fn test_fixed_size_with_children_clamp_children() {
        _ = tracing_subscriber::fmt::try_init();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 10));
        let mut ctx = ElementCtx::default();
        let child = |ctx: &mut ElementCtx, idx| {
            ElementCtx::element(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title_top(format!("child #{idx}")),
            )
            .layout_params(LayoutParams {
                width: Size::Fixed(10 + idx as u16 * 20),
                height: Size::Fixed(3),
                ..Default::default()
            })
            .create(ctx)
        };
        let children = &[child(&mut ctx, 0), child(&mut ctx, 1)];
        let root = ElementCtx::element(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title_top("parent")
                .fg(Color::Red),
        )
        .children(children.to_vec())
        .layout_params(LayoutParams {
            width: Size::Fixed(24),
            height: Size::Fixed(8),
            direction: Direction::Vertical,
            padding: Padding::uniform(1),
            ..Default::default()
        })
        .create(&mut ctx);
        ctx.calculate_layout(root);
        ctx.render(root, buf.area, &mut buf);
        tracing::info!(
            "\ntest_fixed_size_with_children_clamp_children\n{}",
            buffer_to_string(&buf)
        );
    }

    #[test]
    fn test_fit() {
        _ = tracing_subscriber::fmt::try_init();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 10));
        let mut ctx = ElementCtx::default();
        let child = |ctx: &mut ElementCtx, idx| {
            ElementCtx::element(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title_top(format!("child #{idx}")),
            )
            .layout_params(LayoutParams {
                width: Size::Fixed(10 + idx as u16 * 2),
                height: Size::Fixed(3),
                ..Default::default()
            })
            .create(ctx)
        };
        let children = &[child(&mut ctx, 0), child(&mut ctx, 1)];
        let root = ElementCtx::element(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title_top("parent")
                .fg(Color::Red),
        )
        .children(children.to_vec())
        .layout_params(LayoutParams {
            width: Size::Fit,
            height: Size::Fit,
            direction: Direction::Vertical,
            padding: Padding::uniform(1),
            ..Default::default()
        })
        .create(&mut ctx);
        ctx.calculate_layout(root);
        ctx.render(root, buf.area, &mut buf);
        tracing::info!("\ntest_fit\n{}", buffer_to_string(&buf));
    }
    #[test]
    fn test_horizontal() {
        _ = tracing_subscriber::fmt::try_init();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 10));
        let mut ctx = ElementCtx::default();
        let child = |mut ctx: &mut ElementCtx, idx| {
            let ctx1 = &mut ctx;
            ElementCtx::element(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title_top(format!("child #{idx}")),
            )
            .layout_params(LayoutParams {
                width: Size::Fixed(10 + idx as u16 * 2),
                height: Size::Fixed(3),
                ..Default::default()
            })
            .create(ctx1)
        };
        let children = &[child(&mut ctx, 0), child(&mut ctx, 1)];
        let root = ElementCtx::element(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title_top("parent")
                .fg(Color::Red),
        )
        .children(children.to_vec())
        .layout_params(LayoutParams {
            width: Size::Fit,
            height: Size::Fit,
            direction: Direction::Horizontal,
            padding: Padding::uniform(1),
            ..Default::default()
        })
        .create(&mut ctx);
        ctx.calculate_layout(root);
        ctx.render(root, buf.area, &mut buf);
        tracing::info!("\ntest_horizontal\n{}", buffer_to_string(&buf));
    }
    #[test]
    fn test_gap() {
        _ = tracing_subscriber::fmt::try_init();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 10));
        let mut ctx = ElementCtx::default();
        let child = |ctx: &mut ElementCtx, idx| {
            ElementCtx::element(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title_top(format!("child #{idx}")),
            )
            .layout_params(LayoutParams {
                width: Size::Fixed(10 + idx as u16 * 2),
                height: Size::Fixed(3),
                ..Default::default()
            })
            .create(ctx)
        };
        let children = &[child(&mut ctx, 0), child(&mut ctx, 1)];
        let root = ElementCtx::element(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title_top("parent")
                .fg(Color::Red),
        )
        .children(children.to_vec())
        .layout_params(LayoutParams {
            width: Size::Fit,
            height: Size::Fit,
            direction: Direction::Horizontal,
            padding: Padding::uniform(1),
            gap: 2,
            ..Default::default()
        })
        .create(&mut ctx);
        ctx.calculate_layout(root);
        ctx.render(root, buf.area, &mut buf);
        tracing::info!("\ntest_gap\n{}", buffer_to_string(&buf));
    }
    #[test]
    fn test_grow() {
        _ = tracing_subscriber::fmt::try_init();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 24));
        let mut ctx = ElementCtx::default();
        let child0 = ElementCtx::element(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title_top("sidebar".to_string()),
        )
        .children(vec![
            ElementCtx::element(
                Paragraph::new(
                    "this sidebar is so amazing it can have long text that wraps around",
                )
                .wrap(ratatui::widgets::Wrap { trim: false }),
            )
            .layout_params(LayoutParams {
                width: Size::Grow,
                height: Size::Grow,
                ..Default::default()
            })
            .create(&mut ctx),
        ])
        .layout_params(LayoutParams {
            width: Size::Fixed(10),
            padding: Padding::uniform(1),
            height: Size::Grow,
            ..Default::default()
        })
        .create(&mut ctx);
        let child2 = ElementCtx::element(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title_top("child #2".to_string()),
        )
        .layout_params(LayoutParams {
            width: Size::Grow,
            height: Size::Grow,
            ..Default::default()
        })
        .create(&mut ctx);
        let child3 = ElementCtx::element(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title_top("child #3".to_string()),
        )
        .layout_params(LayoutParams {
            width: Size::Grow,
            height: Size::Grow,
            ..Default::default()
        })
        .create(&mut ctx);
        let child1 = ElementCtx::element(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title_top("child #1".to_string()),
        )
        .layout_params(LayoutParams {
            width: Size::Grow,
            padding: Padding::uniform(1),
            gap: 1,
            height: Size::Grow,
            direction: Direction::Vertical,
            ..Default::default()
        })
        .children(vec![child2, child3])
        .create(&mut ctx);
        let root = ElementCtx::element(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title_top("parent")
                .fg(Color::Red),
        )
        .children(vec![child0, child1])
        .layout_params(LayoutParams {
            width: Size::Fixed(36),
            height: Size::Fixed(18),
            direction: Direction::Horizontal,
            padding: Padding::uniform(1),
            ..Default::default()
        })
        .create(&mut ctx);
        ctx.calculate_layout(root);
        ctx.render(root, buf.area, &mut buf);
        tracing::info!("\ntest_grow\n{}", buffer_to_string(&buf));
    }
    #[test]
    fn test_multiple_children() {
        _ = tracing_subscriber::fmt::try_init();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 20));
        let mut ctx = ElementCtx::default();
        let child = |ctx: &mut ElementCtx, idx, height| {
            ElementCtx::element(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title_top(format!("child #{idx}")),
            )
            .layout_params(LayoutParams {
                padding: Padding::uniform(1),
                width: Size::Fixed(16),
                height: Size::Fixed(height),
                ..Default::default()
            })
            .create(ctx)
        };
        let subchildren = &[child(&mut ctx, 0, 6), child(&mut ctx, 1, 6)];
        let children = &[
            child(&mut ctx, 2, 14).children(&mut ctx, subchildren),
            child(&mut ctx, 3, 14),
        ];
        let root = ElementCtx::element(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title_top("parent")
                .fg(Color::Red),
        )
        .children(children.to_vec())
        .layout_params(LayoutParams {
            width: Size::Fit,
            height: Size::Fit,
            direction: Direction::Horizontal,
            padding: Padding::uniform(1),
            ..Default::default()
        })
        .create(&mut ctx);
        ctx.calculate_layout(root);
        ctx.render(root, buf.area, &mut buf);
        tracing::info!("\ntest_horizontal\n{}", buffer_to_string(&buf));
    }

    #[test]
    fn test_list_justify() {
        _ = tracing_subscriber::fmt::try_init();
        for justify in Justify::iter() {
            let mut buf = Buffer::empty(Rect::new(0, 0, 50, 20));
            let mut ctx = ElementCtx::default();
            let root = block()
                .rounded()
                .title_top(format!("{justify:?}"))
                .fg(Color::Red)
                .commit()
                .width(Size::Fixed(24))
                .height(Size::Fixed(20))
                .children(
                    (0..3)
                        .map(|idx| {
                            block()
                                .rounded()
                                .commit()
                                .text(format!("child #{idx}"))
                                .commit_text(&mut ctx)
                                .width(Size::Grow)
                                .height(Size::Fixed(3))
                                .create(&mut ctx)
                        })
                        .collect::<Vec<_>>(),
                )
                .main_justify(justify)
                .create(&mut ctx);
            ctx.calculate_layout(root);
            ctx.render(root, buf.area, &mut buf);
            tracing::info!("\ntest_list_justify\n{}", buffer_to_string(&buf));
        }
    }
}
