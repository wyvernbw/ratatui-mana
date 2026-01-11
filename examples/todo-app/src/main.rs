use mana_tui::prelude::*;
use ratatui::{DefaultTerminal, layout::Rect};

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    ratatui::run(app)?;
    Ok(())
}

fn app(terminal: &mut DefaultTerminal) -> std::io::Result<()> {
    let mut ctx = ElementCtx::new();
    let root = init(&mut ctx, terminal.get_frame().area());
    loop {
        terminal
            .draw(|frame| {
                ctx.render(root, frame.area(), frame.buffer_mut());
            })
            .unwrap();
        if crossterm::event::read()?.is_key_press() {
            break Ok(());
        }
    }
}

fn init(ctx: &mut ElementCtx, area: Rect) -> Element {
    let root = todo_app();
    let root = ctx.spawn_ui(root);
    ctx.calculate_layout(root, area).unwrap();
    root
}

#[subview]
fn todo_app() -> View {
    ui! {
        <Block Center Width::grow() Height::grow()>
            <Block .rounded Width::fixed(32) Height::fixed(24)/>
        </Block>
    }
}
