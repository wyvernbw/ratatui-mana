//! # ratatui-elemental
//!
//! ratatui layout library

#![forbid(missing_docs)]
pub mod layout;
pub mod prelude;
pub mod ui;

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use hecs::World;
    use mana_tui_macros::ui;
    use ratatui::{buffer::Buffer, layout::Rect};

    fn buffer_to_string(buf: &Buffer) -> String {
        buf.content()
            .chunks(buf.area.width as usize)
            .flat_map(|line| line.iter().map(|cell| cell.symbol()).chain(["\n"]))
            .collect()
    }

    #[test]
    fn test_grow_2() {
        _ = tracing_subscriber::fmt::try_init();
        _ = color_eyre::install();
        let mut ctx = ElementCtx::new();
        let block = || Block::bordered().border_type(BorderType::Rounded);
        let root = ui(block().title_top("parent"))
            .with((
                Width(Size::Fixed(36)),
                Height(Size::Fixed(18)),
                Direction::Horizontal,
                Padding::uniform(1),
            ))
            .children((
                ui(block().title_top("sidebar"))
                    .with((
                        Width(Size::Fixed(10)),
                        Height(Size::Grow),
                        Padding::uniform(1),
                    ))
                    .child(ui(Paragraph::new(
                        "this sidebar is so amazing it can have long text that wraps around",
                    )
                    .wrap(ratatui::widgets::Wrap { trim: false }))),
                ui(block().title_top("child #1"))
                    .with((
                        Width(Size::Grow),
                        Height(Size::Grow),
                        Padding::uniform(1),
                        Gap(1),
                        Direction::Vertical,
                    ))
                    .children((
                        ui(block().title_top("child #2"))
                            .with((Width(Size::Grow), Height(Size::Grow))),
                        ui(block().title_top("child #3"))
                            .with((Width(Size::Grow), Height(Size::Grow))),
                    )),
            ));
        let root = ctx.spawn_ui(root);
        ctx.calculate_layout(root).unwrap();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 24));
        ctx.render(root, buf.area, &mut buf);
        tracing::info!("\ntest_grow_2\n{}", buffer_to_string(&buf));
    }

    #[test]
    fn test_gap() {
        _ = tracing_subscriber::fmt::try_init();
        _ = color_eyre::install();
        let mut ctx = ElementCtx::new();
        let root = ui! {
            <Block .title_top="parent" Width(Size::Fit) Height(Size::Fit) Direction::Horizontal Padding::uniform(1) Gap(2)>
                <Block Width(Size::Fixed(4)) Height(Size::Fixed(3)) />
                <Block Width(Size::Fixed(4)) Height(Size::Fixed(3)) />
                <Block Width(Size::Fixed(4)) Height(Size::Fixed(3)) />
            </Block>
        };
        let root = ctx.spawn_ui(root);
        ctx.calculate_layout(root).unwrap();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 24));
        ctx.render(root, buf.area, &mut buf);
        tracing::info!("\ntest_gap\n{}", buffer_to_string(&buf));
    }

    #[test]
    #[should_panic]
    fn test_hecs() {
        _ = color_eyre::install();
        _ = tracing_subscriber::fmt::try_init();
        let mut world = World::new();
        let a = world.spawn((0i32, "hi"));
        let b = world.spawn((1i32, "hello"));
        let mut query1 = world.query_one::<&mut i32>(a).unwrap();
        let mut query2 = world.query_one::<&mut i32>(b).unwrap();
        let a = query1.get();
        let b = query2.get();
        assert_ne!(a, b);
    }
}
