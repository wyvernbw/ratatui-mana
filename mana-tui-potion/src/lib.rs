#![feature(trait_alias)]
#![allow(clippy::collapsible_if)]

pub mod backends;
pub mod focus;

use flume::{Receiver, Sender};
use hecs::Component;
use mana_tui_elemental::{
    layout::{Element, ElementCtx},
    ui::View,
};
use ratatui::{Terminal, prelude::Backend};
use smallbox::SmallBox;
use tailcall::tailcall;

use crate::backends::{DefaultBackend, DefaultEvent, ManaBackend, MsgStream};

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
    #[error("error propagating event")]
    PropagateEventError,
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
async fn runtime<Msg: Message, B: 'static + ManaBackend>(
    model: Msg::Model,
    view: impl ViewFn<Msg, Msg::Model>,
    update: impl UpdateFn<Msg, Msg::Model>,
    quit_signal: impl SignalFn<Msg, Msg::Model>,
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
            let result = focus::propagate_event::<Msg>(&ctx.el_ctx, &model, &event)
                .map_err(|_| RuntimeErr::PropagateEventError)?;
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
        focus::generate_ui_stack(&mut ctx.el_ctx, root);
        focus::init_focus_system(&mut ctx.el_ctx);
        _ = focus::set_focus_style(&mut ctx.el_ctx);

        if let Err(err) = result {
            tracing::error!("failed to calculate layout: {err}");
            return;
        }

        ctx.el_ctx.render(root, frame.area(), frame.buffer_mut());
    });

    if let Err(err) = result {
        tracing::error!("failed to draw: {err}");
    }

    root
}

/// # Errors
///
/// errors here should be treated as fatal. this function errros:
///
/// - if the app channel is closed somehow
/// - if an error happens while propagating an event
/// - if there is an error initializing the runtime
#[bon::builder]
#[builder(finish_fn = run)]
pub async fn run<W, Msg>(
    writer: W,
    init: impl InitFn<Msg, Msg::Model>,
    view: impl ViewFn<Msg, Msg::Model>,
    update: impl UpdateFn<Msg, Msg::Model>,
    quit_signal: impl SignalFn<Msg, Msg::Model>,
) -> Result<(), RuntimeErr>
where
    Msg: Component,
    Msg: Clone,
    W: std::io::Write + 'static,
    Msg: Message,
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

pub trait Message: Clone + Component {
    type Model;
}
