use mana_tui_elemental::prelude::*;
use mana_tui_macros::{subview, ui};

#[subview]
fn subview_test(name: &'static str) -> View {
    ui! {
        { format!("Hello {name}!") }
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
            // you can drill this down as deep as you want ( ͡° ͜ʖ ͡°)
            {children}
        </Block>
    }
}

#[test]
fn test() {
    let _ = ui! {
        <Block .title_top="sidebar" Width(Size::Fixed(10)) Padding::uniform(1)>
            <Block .title_top="2" />
            <Paragraph .alignment={ratatui::layout::HorizontalAlignment::Center}/>
            <SubviewTest .name="there" />
            <Container
                .justify={Justify::Start}
                .children={
                    (0..3).map(|_| ui!{
                        <SubviewTest .name="me" />
                    })
                }
            />
        </Block>
    };

    let _ = ui! {
        <Block .title_top="sidebar" Width(Size::Fixed(10)) Padding::uniform(1)>
        </Block>
    };

    let _ = ui! {
        <Block .title_top="parent 2"
            .rounded
            Width::fixed(24) Height::fixed(5)
            Direction::Horizontal Padding::uniform(1)
            Gap(2)
        >
            {
                (0..3).map(|idx| ui! { <Block .rounded Width::fixed(4) Height::fixed(3) /> })
            }
        </Block>
    };

    let _ = ui! {
        <Block>
        {
            Justify::iter().map(|justify|
                ui! {
                    <Block
                        .title_top={format!("{justify:?}")}
                        .rounded
                        MainJustify(justify)
                        Width::fixed(24)
                        Height::fixed(5)
                        Direction::Horizontal Padding::uniform(1)
                    >
                        {
                            (0..3).map(|idx| ui! {
                                <Block .rounded Width::fixed(4) Height::fixed(3)>
                                        {
                                            format!("wooow text")
                                        }
                                </Block>
                            })
                        }
                    </Block>
                }
            )
        }
        </Block>
    };
    let _ = ui! {
        <Block>
        {
            Justify::iter().map(|justify|
                ui! {
                    <Block
                        .title_top={format!("{justify:?}")}
                        .rounded
                        MainJustify(justify)
                        Width::fixed(24)
                        Height::fixed(5)
                        Direction::Horizontal
                    >
                        {
                            (0..3).map(|idx| ui! {
                                <Block .rounded Width::fixed(4) Height::fixed(3)>
                                    { format!("{idx:02}") }
                                </Block>
                            })
                        }
                    </Block>
                }
            )
        }
        </Block>
    };
}
