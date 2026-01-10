use mana_tui_elemental::prelude::*;
use mana_tui_macros::ui;

fn test() {
    let _ = ui! {
        <Block .title_top="sidebar" Width(Size::Fixed(10)) Padding::uniform(1)>
            <Block .title_top="2" />
        </Block>
    };

    let _ = ui! {
        <Block .title_top="sidebar" Width(Size::Fixed(10)) Padding::uniform(1)>
        </Block>
    };
}
