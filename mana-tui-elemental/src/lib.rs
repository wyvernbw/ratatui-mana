//! # ratatui-elemental
//!
//! ratatui layout library

#![forbid(missing_docs)]
pub mod layout;
pub mod prelude;
pub mod ui;

#[cfg(test)]
mod tests {
    use hecs::World;
    use ratatui::{
        buffer::Buffer,
        layout::{Direction, Rect},
        style::{Color, Stylize},
        widgets::{Block, BorderType, Padding, Paragraph},
    };

    use crate::prelude::*;
    use crate::{
        layout::{ElementCtx, Justify, Size},
        prelude::{BlockExt, block, element},
    };

    fn buffer_to_string(buf: &Buffer) -> String {
        buf.content()
            .chunks(buf.area.width as usize)
            .flat_map(|line| line.iter().map(|cell| cell.symbol()).chain(["\n"]))
            .collect()
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

        ctx.calculate_layout(root).unwrap();
        ctx.render(root, buf.area, &mut buf);
        tracing::info!(
            "\ntest_fixed_size_with_children\n{}",
            buffer_to_string(&buf)
        );
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
    fn test_list_justify() {
        _ = tracing_subscriber::fmt::try_init();
        for justify in Justify::iter() {
            let mut buf = Buffer::empty(Rect::new(0, 0, 50, 6));
            let mut ctx = ElementCtx::default();
            let root = block()
                .rounded()
                .title_top(format!("{justify:?}"))
                .fg(Color::Red)
                .commit()
                .width(Size::Fixed(24))
                .direction(Direction::Horizontal)
                .height(Size::Fixed(5))
                .children(
                    (0..3)
                        .map(|idx| {
                            block()
                                .rounded()
                                .commit()
                                .text(format!("#{idx}"))
                                .commit_text(&mut ctx)
                                .width(Size::Fixed(4))
                                .height(Size::Fixed(3))
                                .create(&mut ctx)
                        })
                        .collect::<Vec<_>>(),
                )
                .main_justify(justify)
                .create(&mut ctx);
            ctx.calculate_layout(root).unwrap();
            ctx.render(root, buf.area, &mut buf);
            tracing::info!("\ntest_list_justify\n{}", buffer_to_string(&buf));
        }
    }

    #[test]
    fn test_gap() {
        _ = tracing_subscriber::fmt::try_init();
        _ = color_eyre::install();
        let mut ctx = ElementCtx::new();
        let block = || Block::bordered().border_type(BorderType::Rounded);
        let root = ui(block().title_top("parent"))
            .with((
                Width(Size::Fit),
                Height(Size::Fit),
                Direction::Horizontal,
                Padding::uniform(1),
                Gap(2),
            ))
            .children((
                ui(block()).with((Width(Size::Fixed(4)), Height(Size::Fixed(3)))),
                ui(block()).with((Width(Size::Fixed(4)), Height(Size::Fixed(3)))),
                ui(block()).with((Width(Size::Fixed(4)), Height(Size::Fixed(3)))),
            ));
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
