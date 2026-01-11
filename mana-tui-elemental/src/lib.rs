#![doc = include_str!("../readme.md")]
#![forbid(missing_docs)]
#![cfg_attr(feature = "nightly", feature(trait_alias))]

extern crate self as mana_tui_elemental;

pub mod layout;
pub mod prelude;
pub mod ui;

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use hecs::World;
    use mana_tui_macros::{subview, ui};
    use ratatui::{buffer::Buffer, layout::Rect, widgets::Wrap};

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
                Width::fixed(36),
                Height::fixed(18),
                Direction::Horizontal,
                Padding::uniform(1),
            ))
            .children((
                ui(block().title_top("sidebar"))
                    .with((Width::fixed(10), Height::grow(), Padding::uniform(1)))
                    .child(ui(Paragraph::new(
                        "this sidebar is so amazing it can have long text that wraps around",
                    )
                    .wrap(ratatui::widgets::Wrap { trim: false }))),
                ui(block().title_top("child #1"))
                    .with((
                        Width::grow(),
                        Height::grow(),
                        Padding::uniform(1),
                        Gap(1),
                        Direction::Vertical,
                    ))
                    .children((
                        ui(block().title_top("child #2")).with((Width::grow(), Height::grow())),
                        ui(block().title_top("child #3")).with((Width::grow(), Height::grow())),
                    )),
            ));
        let root = ctx.spawn_ui(root);
        ctx.calculate_layout(root).unwrap();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 24));
        ctx.render(root, buf.area, &mut buf);
        tracing::info!("\ntest_grow_2\n{}", buffer_to_string(&buf));
    }

    #[test]
    fn test_grow_3() {
        _ = tracing_subscriber::fmt::try_init();
        _ = color_eyre::install();
        let mut ctx = ElementCtx::new();

        #[subview]
        fn sidebar() -> View {
            let value = "i am formatted";
            ui! {
                <Block .rounded .title_top="sidebar" Width::fixed(10) Height::grow()>
                    <Paragraph .wrap={Wrap::default()}>
                        "this sidebar is so amazing it can have long text that wraps around
                        and formatting arguments {value}"
                    </Paragraph>
                </Block>
            }
        }

        let root = ui! {
            <Block
                .rounded .title_top="parent"
                Width::fixed(36) Height::fixed(18) Direction::Horizontal Padding::uniform(1)
            >
                <Sidebar />
                <Block .rounded .title_top="child #1"
                    Width::grow() Height::grow() Padding::uniform(1) Gap(1) Direction::Vertical
                >
                    <Block .rounded .title_top="child #2" Width::grow() Height::grow()/>
                    <Block .rounded .title_top="child #3" Width::grow() Height::grow()/>
                </Block>
            </Block>
        };
        let root = ctx.spawn_ui(root);
        ctx.calculate_layout(root).unwrap();
        let mut buf = Buffer::empty(Rect::new(0, 0, 36, 18));
        ctx.render(root, buf.area, &mut buf);
        tracing::info!("\ntest_grow_3\n{}", buffer_to_string(&buf));

        let expected = Buffer::with_lines(vec![
            "╭parent────────────────────────────╮",
            "│╭sidebar─╮╭child #1──────────────╮│",
            "││        ││╭child #2────────────╮││",
            "││        │││                    │││",
            "││        │││                    │││",
            "││        │││                    │││",
            "││        │││                    │││",
            "││        │││                    │││",
            "││        ││╰────────────────────╯││",
            "││        ││                      ││",
            "││        ││╭child #3────────────╮││",
            "││        │││                    │││",
            "││        │││                    │││",
            "││        │││                    │││",
            "││        │││                    │││",
            "││        ││╰────────────────────╯││",
            "│╰────────╯╰──────────────────────╯│",
            "╰──────────────────────────────────╯",
        ]);
        assert_eq!(buf, expected);
    }

    #[test]
    fn test_gap() {
        _ = tracing_subscriber::fmt::try_init();
        _ = color_eyre::install();

        let mut ctx = ElementCtx::new();
        let root = ui! {
            <Block .rounded .title_top="parent" Width::fit() Height::fit() Direction::Horizontal Gap(2)>
                <Block .rounded Width::fixed(4) Height::fixed(3)>
                    "01"
                </Block>
                <Block .rounded Width::fixed(4) Height::fixed(3)>
                    "02"
                </Block>
                <Block .rounded Width::fixed(4) Height::fixed(3)>
                    "03"
                </Block>
            </Block>
        };
        let root = ctx.spawn_ui(root);

        ctx.calculate_layout(root).unwrap();
        let mut buf = Buffer::empty(Rect::new(0, 0, 24, 6));
        ctx.render(root, buf.area, &mut buf);
        tracing::info!("\ntest_gap\n{}", buffer_to_string(&buf));
    }

    #[test]
    fn test_list_justify() {
        _ = tracing_subscriber::fmt::try_init();
        _ = color_eyre::install();

        let mut ctx = ElementCtx::new();

        #[subview]
        fn numbered_box(idx: i32) -> View {
            ui! {
                <Block .rounded Width::fixed(4) Height::fixed(3)>
                    // formatting out of the box :)
                    "{idx:02}"
                </Block>
            }
        }
        #[subview]
        fn container(justify: Justify, children: impl AsChildren) -> View {
            ui! {
                <Block
                    .title_top={format!("{justify:?}")}
                    .rounded
                    MainJustify(justify)
                    Width::fixed(24)
                    Height::fixed(5)
                    Direction::Horizontal
                >
                    {children}
                </Block>
            }
        }
        #[subview]
        fn root() -> View {
            ui! {
                <Block>
                {
                    Justify::iter().map(|justify|
                        ui! {
                            <Container
                                .justify={justify}
                                .children={
                                    (0..3).map(|idx| ui!{
                                        <NumberedBox .idx={idx} />
                                    })
                                }
                            />
                        }
                    )
                }
                </Block>
            }
        }
        let root = ctx.spawn_ui(root());
        ctx.calculate_layout(root).unwrap();

        let mut buf = Buffer::empty(Rect::new(0, 0, 24, 30));
        ctx.render(root, buf.area, &mut buf);
        let expected = Buffer::with_lines(vec![
            "╭Start─────────────────╮",
            "│╭──╮╭──╮╭──╮          │",
            "││00││01││02│          │",
            "│╰──╯╰──╯╰──╯          │",
            "╰──────────────────────╯",
            "╭Center────────────────╮",
            "│     ╭──╮╭──╮╭──╮     │",
            "│     │00││01││02│     │",
            "│     ╰──╯╰──╯╰──╯     │",
            "╰──────────────────────╯",
            "╭SpaceBetween──────────╮",
            "│╭──╮     ╭──╮     ╭──╮│",
            "││00│     │01│     │02││",
            "│╰──╯     ╰──╯     ╰──╯│",
            "╰──────────────────────╯",
            "╭SpaceAround───────────╮",
            "│ ╭──╮   ╭──╮   ╭──╮   │",
            "│ │00│   │01│   │02│   │",
            "│ ╰──╯   ╰──╯   ╰──╯   │",
            "╰──────────────────────╯",
            "╭SpaceEvenly───────────╮",
            "│  ╭──╮  ╭──╮  ╭──╮    │",
            "│  │00│  │01│  │02│    │",
            "│  ╰──╯  ╰──╯  ╰──╯    │",
            "╰──────────────────────╯",
            "╭End───────────────────╮",
            "│          ╭──╮╭──╮╭──╮│",
            "│          │00││01││02││",
            "│          ╰──╯╰──╯╰──╯│",
            "╰──────────────────────╯",
        ]);

        assert_eq!(buf, expected);
        tracing::info!("\ntest_list_justify\n{}", buffer_to_string(&buf));
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
