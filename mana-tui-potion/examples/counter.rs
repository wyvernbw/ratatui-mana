use std::io::stdout;
use std::time::Duration;

use crossterm::event::{EnableMouseCapture, Event, KeyEvent, KeyModifiers};
use mana_tui::key;
use mana_tui_elemental::prelude::*;
use mana_tui_elemental::ui::View;
use mana_tui_macros::ui;
use mana_tui_potion::backends::{DefaultEvent, KeyEventExt};
use mana_tui_potion::focus::handlers::{On, OnClickOrKey, OnKey};
use mana_tui_potion::focus::{FocusStyle, FocusTarget};
use mana_tui_potion::{Effect, Message, run};
use ratatui::style::Style;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    fn should_quit(_: &Model, event: &AppMsg) -> bool {
        matches!(event, AppMsg::Quit)
    }
    run()
        .writer(std::io::stdout())
        .init(init)
        .view(view)
        .update(update)
        .quit_signal(should_quit)
        .run()
        .await
        .unwrap();
}

#[derive(Debug, Default, Clone)]
struct Model {
    value: i32,
    awake: bool,
}

#[derive(Debug, Clone)]
enum AppMsg {
    Inc,
    Dec,
    Quit,
    Wakeup,
}

impl Message for AppMsg {
    type Model = Model;
}

async fn init() -> (Model, Effect<AppMsg>) {
    _ = crossterm::execute!(stdout(), EnableMouseCapture);
    (
        Model::default(),
        Effect::new(async |tx| {
            tokio::time::sleep(Duration::from_secs(1)).await;
            _ = tx.send_async(AppMsg::Wakeup).await;
        }),
    )
}

async fn view(model: &Model) -> View {
    struct DecButton;
    struct IncButton;

    let count = model.value;

    ui! {
        <Block
            .rounded
            .title_top="Magical App"
            Center
            Height::grow() Width::grow()
            On::new(handle_quit)
        >
            <Block Direction::Horizontal CrossJustify::Center Gap(2)>
                <Block
                    .rounded .title_bottom="j" .title_alignment={ratatui::layout::HorizontalAlignment::Center}
                    FocusTarget::new::<DecButton>()
                    FocusStyle(Style::new().green())
                    Width::fixed(5) Center
                    OnClickOrKey::new(KeyEvent::char('j'), AppMsg::Dec)
                >
                    "-"
                </Block>
                <Block Width::fixed(20) Height::fixed(1) Center>
                {
                    if model.awake {
                        format!("I have awoken {count}")
                    } else {
                        "I sleep...".to_string()
                    }
                }
                </Block>
                <Block
                    .rounded .title_bottom="k" .title_alignment={ratatui::layout::HorizontalAlignment::Center}
                    FocusTarget::new::<IncButton>()
                    FocusStyle(Style::new().green())
                    Width::fixed(5) Center OnClickOrKey::new(KeyEvent::char('k'), AppMsg::Inc)
                >
                    "+"
                </Block>
            </Block>
        </Block>
    }
}

fn handle_quit(_: &Model, event: &DefaultEvent) -> Option<(AppMsg, Effect<AppMsg>)> {
    match event {
        Event::Key(key!(Char('q'), Press) | key!(Char('c'), Press, KeyModifiers::CONTROL)) => {
            Some((AppMsg::Quit, Effect::none()))
        }
        _ => None,
    }
}

async fn update(model: Model, msg: AppMsg) -> (Model, Effect<AppMsg>) {
    match msg {
        AppMsg::Inc => (
            Model {
                value: model.value + 1,
                ..model
            },
            Effect::none(),
        ),
        AppMsg::Dec => (
            Model {
                value: model.value - 1,
                ..model
            },
            Effect::none(),
        ),
        AppMsg::Wakeup => (
            Model {
                awake: true,
                ..model
            },
            Effect::none(),
        ),
        AppMsg::Quit => (model, Effect::none()),
    }
}
