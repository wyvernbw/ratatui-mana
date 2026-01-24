#![feature(trait_alias)]

pub(crate) mod focus;

use flume::{Receiver, Sender};
use hecs::Component;
use mana_tui_elemental::{
    layout::{Element, ElementCtx},
    ui::View,
};
use ratatui::{
    Terminal,
    prelude::{Backend, CrosstermBackend},
};
use smallbox::SmallBox;
use tailcall::tailcall;
use tokio_stream::StreamExt;

pub type Chan<Msg> = (Sender<Msg>, Receiver<Msg>);
pub trait UpdateFn<Msg, Model> = AsyncFn(Model, Msg) -> (Model, Effect<Msg>) + Component;
pub trait InitFn<Msg, Model> = AsyncFn() -> (Model, Effect<Msg>) + Component;
pub trait ViewFn<Msg, Model> = AsyncFn(&Model) -> View + Component;
pub trait SignalFn<Msg, Model> = Fn(&Model, &Msg) -> bool;

type PinnedFuture<R> = SmallBox<dyn Future<Output = R> + Send + Sync + 'static, [usize; 4]>;

pub trait EffectFn<Msg>: Send + Sync + 'static {
    fn run_effect(&self, tx: Sender<Msg>) -> PinnedFuture<()>;
}

impl<F, Fut, Msg> EffectFn<Msg> for F
where
    F: Fn(Sender<Msg>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + Sync + 'static,
{
    fn run_effect(&self, tx: Sender<Msg>) -> PinnedFuture<()> {
        let future = (self)(tx);
        SmallBox::<Fut, [usize; 4]>::new(future as _)
    }
}
pub struct Effect<Msg>(SmallBox<dyn EffectFn<Msg>, [usize; 4]>);

impl<Msg: Send + Sync + 'static> Effect<Msg> {
    #[must_use]
    pub fn none() -> Self {
        Self::new(async move |_| {})
    }
    pub fn new<
        Fut: Future<Output = ()> + Send + Sync + 'static,
        F: Fn(Sender<Msg>) -> Fut + 'static + Send + Sync,
    >(
        f: F,
    ) -> Self {
        Self(SmallBox::new(f) as _)
    }
}

enum RuntimeMsg<Msg> {
    App(Msg),
    Term(DefaultEvent),
}

#[derive(thiserror::Error, Debug)]
pub enum RuntimeErr {
    #[error("app channel closed")]
    ChannelClosed,
    #[error("error propagating event: {0}")]
    PropagateEventError(#[from] hecs::ComponentError),
    #[error("error initializing runtine")]
    InitErr,
}

#[derive(derive_more::Deref, derive_more::DerefMut)]
pub struct Ctx<B: Backend> {
    #[deref]
    #[deref_mut]
    el_ctx: ElementCtx,
    terminal: Terminal<B>,
}

#[tailcall]
async fn runtime<Msg: Clone + 'static, Model: 'static, B: 'static + ManaBackend>(
    model: Model,
    view: impl ViewFn<Msg, Model>,
    update: impl UpdateFn<Msg, Model>,
    quit_signal: impl SignalFn<Msg, Model>,
    mut msg_stream: MsgStream<Msg>,
    ctx: &mut Ctx<B>,
    prev_root: Option<Element>,
) -> Result<(), RuntimeErr> {
    let msg = MsgStream::<Msg>::next(&mut msg_stream).await;
    match msg {
        RuntimeMsg::App(msg) if quit_signal(&model, &msg) => Ok(()),
        RuntimeMsg::App(msg) => {
            let (model, effect) = update(model, msg).await;
            tokio::spawn(effect.0.run_effect(msg_stream.dispatch.0.clone()));
            let root = view(&model).await;
            if let Some(prev) = prev_root {
                ctx.despawn_ui(prev);
            }
            let root = render::<B>(ctx, root);

            runtime(
                model,
                view,
                update,
                quit_signal,
                msg_stream,
                ctx,
                Some(root),
            )
        }
        RuntimeMsg::Term(event) => {
            let result = focus::propagate_event::<Msg, Model>(&ctx.el_ctx, &model, &event)?;
            if let Some((msg, effect)) = result {
                tokio::spawn(effect.0.run_effect(msg_stream.dispatch.0.clone()));
                msg_stream
                    .dispatch
                    .0
                    .send_async(msg)
                    .await
                    .map_err(|_| RuntimeErr::ChannelClosed)?;
            }
            runtime(model, view, update, quit_signal, msg_stream, ctx, prev_root)
        }
    }
}

fn render<B: Backend>(ctx: &mut Ctx<B>, view: View) -> Element {
    let root = ctx.spawn_ui(view);
    let result = ctx.terminal.draw(|frame| {
        let result = ctx.el_ctx.calculate_layout(root, frame.area());

        if let Err(err) = result {
            tracing::error!("failed to calculate layout: {err}");
            return;
        }

        ctx.el_ctx.render(root, frame.area(), frame.buffer_mut());
        focus::generate_ui_stack(&mut ctx.el_ctx, root);
    });

    if let Err(err) = result {
        tracing::error!("failed to draw: {err}");
    }

    root
}

pub trait ManaBackend: Backend {
    type Events: EventStream;

    #[allow(async_fn_in_trait)]
    async fn create_events(&mut self) -> Self::Events;
}

pub trait EventStream {
    type Out;
    type Err;

    #[allow(async_fn_in_trait)]
    async fn read(&mut self) -> Result<Self::Out, Self::Err>;
}

pub struct MsgStream<Msg> {
    event_stream: <DefaultBackend<std::io::Stdout> as ManaBackend>::Events,
    dispatch: Chan<Msg>,
}

impl<Msg> MsgStream<Msg> {
    async fn next(this: &mut Self) -> RuntimeMsg<Msg> {
        loop {
            tokio::select! {
                event = this.event_stream.read() => {
                    if let Ok(event) = event { return RuntimeMsg::Term(event) }
                }
                msg = this.dispatch.1.recv_async() => {
                    if let Ok(msg) = msg { return RuntimeMsg::App(msg) }
                }
            }
        }
    }
}

impl<W: std::io::Write> ManaBackend for CrosstermBackend<W> {
    type Events = crossterm::event::EventStream;

    async fn create_events(&mut self) -> Self::Events {
        crossterm::event::EventStream::new()
    }
}

impl EventStream for crossterm::event::EventStream {
    type Out = crossterm::event::Event;
    type Err = std::io::Error;

    async fn read(&mut self) -> Result<Self::Out, Self::Err> {
        loop {
            let res = self.next().await;
            if let Some(event) = res {
                return event;
            }
        }
    }
}

pub type DefaultBackend<W> = CrosstermBackend<W>;
pub type DefaultEvent =
    <<DefaultBackend<std::io::Stdout> as ManaBackend>::Events as EventStream>::Out;

/// # Errors
///
/// errors here should be treated as fatal. this function errros:
///
/// - if the app channel is closed somehow
/// - if an error happens while propagating an event
/// - if there is an error initializing the runtime
pub async fn run<W: std::io::Write + 'static, Msg, Model>(
    writer: W,
    init: impl InitFn<Msg, Model>,
    view: impl ViewFn<Msg, Model>,
    update: impl UpdateFn<Msg, Model>,
    quit_signal: impl SignalFn<Msg, Model>,
) -> Result<(), RuntimeErr>
where
    Msg: Send + Sync + 'static,
    Model: Send + Sync + 'static,
    Msg: Clone + 'static + std::fmt::Debug,
{
    let dispatch = flume::unbounded::<Msg>();
    let mut backend = DefaultBackend::new(writer);
    let msg_stream = MsgStream {
        event_stream: backend.create_events().await,
        dispatch: dispatch.clone(),
    };
    let terminal = ratatui::Terminal::new(backend).map_err(|_| RuntimeErr::InitErr)?;

    ratatui::init();
    let mut ctx = Ctx {
        el_ctx: mana_tui_elemental::prelude::ElementCtx::new(),
        terminal,
    };

    let (model, effect) = init().await;
    tokio::spawn(effect.0.run_effect(dispatch.0.clone()));
    let tree = view(&model).await;
    let root = render::<DefaultBackend<W>>(&mut ctx, tree);

    let result = runtime(
        model,
        view,
        update,
        quit_signal,
        msg_stream,
        &mut ctx,
        Some(root),
    )
    .await;

    ratatui::restore();

    result
}

#[cfg(test)]
mod examples {
    use std::time::Duration;

    use crossterm::event::{Event, KeyModifiers};
    use mana_tui::key;
    use mana_tui_elemental::prelude::*;
    use mana_tui_elemental::ui::View;
    use mana_tui_macros::ui;

    use crate::focus::On;
    use crate::{DefaultEvent, Effect};

    #[tokio::test(flavor = "current_thread")]
    async fn simple_app() {
        fn should_quit(_: &Model, event: &AppMsg) -> bool {
            matches!(event, AppMsg::Quit)
        }

        crate::run(std::io::stdout(), init, view, update, should_quit)
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

    async fn init() -> (Model, Effect<AppMsg>) {
        (
            Model::default(),
            Effect::new(async |tx| {
                tokio::time::sleep(Duration::from_secs(1)).await;
                _ = tx.send_async(AppMsg::Wakeup).await;
            }),
        )
    }

    async fn view(model: &Model) -> View {
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
                        .rounded .title_bottom="j"
                        Width::fixed(5) Center {On::new(
                            move |_: &Model, event: &Event| match event {
                                Event::Key(key!(Char('j'), Press)) => Some((AppMsg::Dec, Effect::none())),
                                _ => None,
                            }
                        )
                    }>
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
                        .rounded .title_bottom="k"
                        Width::fixed(5) Center {On::new(
                            move |_: &Model, event: &Event| match event {
                                Event::Key(key!(Char('k'), Press)) => Some((AppMsg::Inc, Effect::none())),
                                _ => None,
                            }
                        )
                    }>
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
}
