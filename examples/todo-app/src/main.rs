use std::io::stdout;

use anyhow::Result;
use crossterm::event::{EnableMouseCapture, Event, KeyCode, KeyEvent};
use mana_tui::prelude::*;
use ratatui::{
    DefaultTerminal, layout::Rect, style::palette::tailwind as tw, symbols::braille::BRAILLE,
};

fn main() -> anyhow::Result<()> {
    mana_tui::mx::init();
    crossterm::execute!(stdout(), EnableMouseCapture)?;
    ratatui::run(app)?;
    Ok(())
}

fn app(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut ctx = ElementCtx::new();
    let mut state = AppState::default();
    loop {
        let root = init(&mut ctx, &mut state, terminal.get_frame().area());
        terminal
            .draw(|frame| {
                ctx.render(root, frame.area(), frame.buffer_mut());
            })
            .unwrap();
        if handle_events(&mut ctx, &mut state, crossterm::event::read()?) {
            break Ok(());
        }
    }
}

#[derive(Debug, Default)]
struct AppState {
    todos: Vec<Todo>,
}

#[derive(Debug, Clone)]
struct Todo {
    done: bool,
    description: String,
}

fn init(ctx: &mut ElementCtx, state: &mut AppState, area: Rect) -> Element {
    let root = todo_app().state(state);
    let root = ctx.spawn_ui(root);
    ctx.calculate_layout(root, area).unwrap();
    root
}

fn handle_events(ctx: &mut ElementCtx, state: &mut AppState, event: Event) -> bool {
    match event {
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            ..
        }) => true,
        Event::Key(KeyEvent {
            code: KeyCode::Char('a'),
            ..
        }) => {
            tracing::info!("new todo");
            state.todos.push(Todo {
                done: false,
                description: format!("{} i crave productivity", state.todos.len() + 1),
            });
            false
        }
        _ => false,
    }
}

#[subview]
fn todo_app(state: &mut AppState) -> View {
    ui! {
        <Block Center Width::grow() Height::grow()>
            <Block .rounded Width::fixed(48) Height::fixed(28) Padding::ZERO>
                <Titlebar/>
                <Block Padding::new(2, 2, 1, 2) Width::grow() Height::grow()>
                    <Text .style={Style::new().bg(tw::SKY.c200).fg(Color::Black)}>"(a) add todo"</Text>
                    <Block .borders={Borders::TOP} .border_type={BorderType::LightDoubleDashed} Width::grow() Height::fixed(1)/>
                    <Block Gap(1) ScrollView::default() ScrollViewState::new() Width::grow()>
                    {
                        state.todos.iter().map(|todo| ui! {
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
            .rounded
            .border_style={
                Style::new().fg(tw::PURPLE.c300).bg(Color::Reset)
            }
            .borders={Borders::LEFT | Borders::RIGHT | Borders::TOP }
            .style={
                Style::new().bg(tw::PURPLE.c300).fg(tw::PURPLE.c300)
            }
            Direction::Horizontal
            MainJustify::SpaceBetween
            Width::grow() Height::fixed(2)
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
