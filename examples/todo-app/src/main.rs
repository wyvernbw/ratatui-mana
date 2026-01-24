#![allow(clippy::collapsible_if)]
use std::io::stdout;

use anyhow::Result;
use crossterm::{
    event::{
        DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture, Event,
        KeyCode, KeyEvent, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    terminal::EnterAlternateScreen,
};
use mana_tui::{key, mana_tui_beheaded::focus::FocusExt};
use mana_tui::{
    mana_tui_beheaded::{
        self,
        focus::{FocusPolicy, Keybind, OnClick},
        setup_interactions,
    },
    prelude::*,
};
use ratatui::{
    DefaultTerminal, layout::Rect, style::palette::tailwind as tw, symbols::braille::BRAILLE,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    mana_tui::mx::init();
    crossterm::terminal::enable_raw_mode()?;
    // just crossterm ceremony
    crossterm::execute!(
        stdout(),
        EnterAlternateScreen,
        EnableFocusChange,
        EnableMouseCapture,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        )
    )?;

    let mut term = ratatui::init();
    app(&mut term).await?;
    ratatui::restore();

    crossterm::execute!(
        stdout(),
        DisableMouseCapture,
        DisableFocusChange,
        PopKeyboardEnhancementFlags
    )?;
    Ok(())
}

async fn app(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut ctx = ElementCtx::new();
    mana_tui_beheaded::init(&mut ctx);
    let r = init(&mut ctx, terminal.get_frame().area());
    ctx.despawn_ui(r);
    loop {
        let root = init(&mut ctx, terminal.get_frame().area());
        terminal
            .draw(|frame| {
                ctx.render(root, frame.area(), frame.buffer_mut());
            })
            .unwrap();
        // returning Some from the read handler gives control back to the application
        //
        // note that mana tui will give back control occasionally on certain crossterm
        // or ui events. in those cases, read returns None.
        let quit = mana_tui_beheaded::read(&mut ctx, |world, event| match event {
            // quit application
            Event::Key(key!(Char('q'), Press)) => Some(true),
            // redraw screen (debug purposes)
            Event::Key(key!(Char('r'), Press)) => Some(false),
            _ => None,
        })
        .await;

        if quit == Some(true) {
            return Ok(());
        }

        ctx.despawn_ui(root);
    }
}

#[derive(Debug, Clone)]
struct Todo {
    done: bool,
    description: String,
}

fn init(ctx: &mut ElementCtx, area: Rect) -> Element {
    let root = todo_app().ctx(ctx).into_view();
    let root = ctx.spawn_ui(root);
    ctx.calculate_layout(root, area).unwrap();
    setup_interactions(ctx, root);
    root
}

fn handle_events(ctx: &mut ElementCtx, event: Event) -> bool {
    matches!(
        event,
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            ..
        })
    )
}

#[subview]
fn todo_app(ctx: &mut ElementCtx) -> View {
    struct AddTodoButton;
    ctx.use_focus::<AddTodoButton>();

    let mut todos = ctx.query::<&Todo>();
    let todo_count = todos.iter().count();
    ui! {
        <Block Center Width::grow() Height::grow()>
            <Block .borders={Borders::all().difference(Borders::TOP)} .border_type={BorderType::QuadrantOutside} Width::fixed(48) Height::fixed(28) Padding::ZERO>
                <Titlebar/>
                <Block Padding::new(2, 2, 1, 2) Width::grow() Height::grow()>
                    <Text
                        .style={Style::new().bg(tw::SKY.c200).fg(Color::Black)}
                        AddTodoButton
                        FocusPolicy::Block
                        Keybind::new(KeyCode::Char('a'))
                        OnClick::new(move |world| {
                            // we throw our state into the ECS world :)
                            world.spawn((Todo {
                                done: false,
                                description: format!("{} i crave productivity", todo_count + 1),
                            },));
                        })
                    >
                        "(a) add todo"
                    </Text>
                    <Block .borders={Borders::TOP} .border_type={BorderType::LightDoubleDashed} Width::grow() Height::fixed(1)/>
                    <Block Gap(1) ScrollView::default() ScrollViewState::new() Width::grow()>
                    {
                        todos.iter().map(|todo| ui! {
                            <TodoItem .todo={todo}/>
                        })
                    }
                    </Block>
                </Block>
            </Block>
        </Block>
    }
}

#[subview]
fn titlebar() -> View {
    let title_icon = BRAILLE[255];
    ui! {
        <Block
            // .rounded
            // .border_style={
            //     Style::new().fg(tw::PURPLE.c300).bg(Color::Reset)
            // }
            // .borders={Borders::LEFT | Borders::RIGHT | Borders::TOP }
            .style={
                Style::new().bg(tw::PURPLE.c300).fg(tw::PURPLE.c300)
            }
            Direction::Horizontal
            MainJustify::SpaceBetween
            Width::grow() Height::fixed(1)
        >
            <Paragraph .style={Style::new().fg(Color::Black)}>"{title_icon} +++ Mana To Do +++"</Paragraph>
            <Text .style={Style::new().fg(Color::Black)}>"q {title_icon}"</Text>
        </Block>
    }
}

#[subview]
fn todo_item(todo: &Todo) -> View {
    let Todo { done, description } = todo;

    #[subview]
    fn checkbox(done: bool) -> View {
        if done {
            ui! {
                <Text>"[x]"</Text>
            }
        } else {
            ui! {
                <Text>"[ ]"</Text>
            }
        }
    }
    ui! {
        <Block Direction::Horizontal Gap(1) Width::grow()>
            <Checkbox .done={*done}/>
            "{description}"
        </Block>
    }
}
