#![feature(impl_trait_in_assoc_type)]
#![feature(option_into_flat_iter)]
#![feature(try_blocks)]
#![feature(trait_alias)]
#![feature(async_fn_traits)]
#![feature(unboxed_closures)]
#![feature(explicit_tail_calls)]
#![allow(incomplete_features)]

use std::io::Stdout;

use crossterm::event::{EnableMouseCapture, Event, KeyCode, KeyEvent};
use flume::{Receiver, Sender};
use ratatui::{Terminal, prelude::CrosstermBackend};
use smallbox::SmallBox;
use smol::stream::{Stream, StreamExt};

use crate::elements::Node;

pub trait UpdateFn<Msg, Model> = Fn(Model, Msg) -> (Model, Effect<Msg>) + Send + Sync + 'static;
pub trait InitFn<Msg, Model> = Fn() -> (Model, Effect<Msg>) + Send + Sync + 'static;
pub trait ViewFn<Msg, Model> = Fn(&Model) -> Node<Msg> + Send + Sync + 'static;

pub type Dispatch<Msg> = (Sender<Msg>, Receiver<Msg>);

type PinnedFuture = SmallBox<dyn Future<Output = ()> + Send + Sync + 'static, [usize; 4]>;

pub trait EffectFn<Msg>: Send + Sync + 'static {
    fn run_effect(&self, tx: Sender<Msg>) -> PinnedFuture;
}

impl<F, Fut, Msg> EffectFn<Msg> for F
where
    F: Fn(Sender<Msg>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + Sync + 'static,
{
    fn run_effect(&self, tx: Sender<Msg>) -> PinnedFuture {
        let future = (self)(tx);
        SmallBox::<Fut, [usize; 4]>::new(future as _)
    }
}
pub struct Effect<Msg>(SmallBox<dyn EffectFn<Msg>, [usize; 4]>);

impl<Msg: Send + Sync + 'static> Effect<Msg> {
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

#[derive(Debug)]
enum RuntimeMessage<Msg> {
    App(Msg),
    Term(std::io::Result<crossterm::event::Event>),
}

#[tailcall::tailcall]
#[allow(clippy::too_many_arguments)]
async fn runtime<Msg, Model>(
    model: Model,
    view: impl ViewFn<Msg, Model>,
    update: impl UpdateFn<Msg, Model>,
    mut ctx: Ctx,
    dispatch: Dispatch<Msg>,
    quit_signal: impl Fn(&Msg) -> bool + Send + Sync + 'static,
    tree: Option<Node<Msg>>,
    event_stream: &mut (impl Stream<Item = RuntimeMessage<Msg>> + std::marker::Unpin),
) -> std::io::Result<()>
where
    Model: Send + Sync + 'static,
    Msg: Clone + 'static + std::fmt::Debug,
{
    let Some(event) = event_stream.next().await else {
        return Ok(());
    };

    match event {
        RuntimeMessage::App(msg) if quit_signal(&msg) => Ok(()),
        RuntimeMessage::App(msg) => {
            let (model, effect) = update(model, msg);
            smol::spawn(effect.0.run_effect(dispatch.0.clone())).detach();

            let tree = view(&model);

            render(&mut ctx, &tree)?;

            runtime(
                model,
                view,
                update,
                ctx,
                dispatch,
                quit_signal,
                Some(tree),
                event_stream,
            );
        }
        RuntimeMessage::Term(Err(err)) => panic!("{err}"),
        RuntimeMessage::Term(Ok(event)) => {
            let tree = tree.unwrap_or_else(|| view(&model));
            let area = ctx.get_frame().area();
            tree.handle_event(area, event, &dispatch)
                .expect("failed to send message from event handler");

            runtime(
                model,
                view,
                update,
                ctx,
                dispatch,
                quit_signal,
                Some(tree),
                event_stream,
            );
        }
    }
}

pub mod elements {
    use std::marker::PhantomData;

    use crossterm::event::Event;
    use crossterm::event::MouseEvent;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Constraint;
    use ratatui::layout::Direction;
    use ratatui::layout::Flex;
    use ratatui::layout::Layout;
    use ratatui::layout::Rect;
    use ratatui::text::Text;
    use ratatui::widgets::Block;
    use ratatui::widgets::Paragraph;
    use ratatui::widgets::Widget;
    use smallbox::SmallBox;

    use crate::Dispatch;

    #[derive(derive_more::Debug)]
    enum NodeKind<Msg> {
        Leaf {
            widget: SmallBox<dyn WidgetRender + Send + 'static, [u64; 1]>,
        },
        Container {
            direction: Direction,
            #[debug(skip)]
            children: Vec<Node<Msg>>,
        },
    }

    #[derive(Debug, Clone)]
    pub struct WidgetWrapper<W>(W);

    pub trait WidgetRender: std::fmt::Debug {
        fn widget_render(&self, area: Rect, buf: &mut Buffer);
    }

    impl<W: Widget + Clone + std::fmt::Debug> WidgetRender for WidgetWrapper<W> {
        fn widget_render(&self, area: Rect, buf: &mut Buffer) {
            self.0.clone().render(area, buf);
        }
    }

    type Callback<Msg> = SmallBox<dyn Fn(Event, Rect) -> Option<Msg> + 'static, [u64; 4]>;

    #[derive(derive_more::Debug)]
    pub struct Node<Msg> {
        inner: NodeKind<Msg>,
        constraint: Constraint,
        block: Block<'static>,
        callback: Option<Callback<Msg>>,
        flex: Option<Flex>,

        #[debug(skip)]
        _msg: PhantomData<Msg>,
    }

    impl<Msg: Clone + 'static> Node<Msg> {
        pub fn render(&self, area: Rect, buf: &mut Buffer) {
            self.block.clone().render(area, buf);
            let area = self.block.inner(area);

            match &self.inner {
                NodeKind::Leaf { widget, .. } => {
                    widget.widget_render(area, buf);
                }
                NodeKind::Container {
                    direction,
                    children,
                } => {
                    let layout =
                        Layout::new(*direction, children.iter().map(|node| node.constraint));
                    let layout = self
                        .flex
                        .map(|flex| layout.clone().flex(flex))
                        .unwrap_or(layout);
                    let areas = layout.split(area);
                    for (node, area) in children.iter().zip(areas.iter()) {
                        node.render(*area, buf);
                    }
                }
            }
        }

        pub fn handle_event(
            &self,
            area: Rect,
            event: Event,
            dispatch: &Dispatch<Msg>,
        ) -> Result<(), flume::SendError<Msg>> {
            let area = self.block.inner(area);

            if let Some(callback) = &self.callback
                && let Some(msg) = callback(event.clone(), area)
            {
                dispatch.0.send(msg)?;
            }

            match &self.inner {
                NodeKind::Leaf { .. } => {}
                NodeKind::Container {
                    direction,
                    children,
                } => {
                    let layout =
                        Layout::new(*direction, children.iter().map(|node| node.constraint));
                    let layout = self
                        .flex
                        .map(|flex| layout.clone().flex(flex))
                        .unwrap_or(layout);
                    let areas = layout.split(area);
                    for (node, area) in children.iter().zip(areas.iter()) {
                        node.handle_event(*area, event.clone(), dispatch)?;
                    }
                }
            };
            Ok(())
        }

        pub fn block(self, block: Block<'static>) -> Self {
            Self { block, ..self }
        }

        pub fn size(self, size: Constraint) -> Self {
            Self {
                constraint: size,
                ..self
            }
        }
        pub fn on(self, callback: impl Fn(Event, Rect) -> Option<Msg> + 'static) -> Self {
            let callback = Some(SmallBox::new(callback) as _);
            Self { callback, ..self }
        }

        pub fn on_click_keybind_down(
            self,
            keybind: impl Fn(Event, Rect) -> bool + 'static,
            result: Msg,
        ) -> Self {
            let callback = move |event: Event, rect| {
                let keybind_pressed = keybind(event.clone(), rect);

                // holy matches!
                let mouse_clicked = matches!(
                    event,
                    Event::Mouse(MouseEvent {
                        kind:
                            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                        column,
                        row,
                        ..
                    })
                    if rect.contains(ratatui::layout::Position { x: column, y: row })
                );

                if mouse_clicked || keybind_pressed {
                    Some(result.clone())
                } else {
                    None
                }
            };
            let callback = Some(SmallBox::new(callback) as _);
            Self { callback, ..self }
        }

        pub fn on_click_down(self, result: Msg) -> Self {
            self.on_click_keybind_down(|_, _| false, result)
        }

        pub fn flex(self, flex: Flex) -> Self {
            Self {
                flex: Some(flex),
                ..self
            }
        }
    }

    pub fn vstack<Msg>(children: impl IntoIterator<Item = Node<Msg>>) -> Node<Msg> {
        let children = children.into_iter().collect();

        Node {
            inner: NodeKind::Container {
                direction: Direction::Vertical,
                children,
            },
            constraint: Constraint::Fill(1),
            block: Block::new(),
            _msg: PhantomData,
            callback: None,
            flex: None,
        }
    }

    pub fn hstack<Msg>(children: impl IntoIterator<Item = Node<Msg>>) -> Node<Msg> {
        let children = children.into_iter().collect();

        Node {
            inner: NodeKind::Container {
                direction: Direction::Horizontal,
                children,
            },
            constraint: Constraint::Fill(1),
            block: Block::new(),
            callback: None,
            _msg: PhantomData,
            flex: None,
        }
    }

    pub fn paragraph<Msg>(text: impl Into<Text<'static>>) -> Node<Msg> {
        let p = Paragraph::new(text);
        Node {
            inner: NodeKind::Leaf {
                widget: SmallBox::new(WidgetWrapper(p)) as _,
            },
            constraint: Constraint::Fill(1),
            block: Block::new(),
            _msg: PhantomData,
            callback: None,
            flex: None,
        }
    }

    pub fn button<Msg>(text: impl Into<Text<'static>>) -> Node<Msg> {
        Node {
            block: Block::bordered().border_type(ratatui::widgets::BorderType::Rounded),
            ..paragraph(text)
        }
    }
}

pub trait CrosstermEventExt {
    fn is_keycode(&self, keycode: KeyCode) -> bool;
}

impl CrosstermEventExt for Event {
    fn is_keycode(&self, keycode: KeyCode) -> bool {
        matches!(self, Event::Key(KeyEvent { code, ..}) if code == &keycode)
    }
}

pub async fn run<Msg, Model>(
    init: impl InitFn<Msg, Model>,
    view: impl ViewFn<Msg, Model>,
    update: impl UpdateFn<Msg, Model>,
    quit_signal: impl Fn(&Msg) -> bool + Send + Sync + 'static,
) -> std::io::Result<()>
where
    Msg: Send + Sync + 'static,
    Model: Send + Sync + 'static,
    Msg: Clone + 'static + std::fmt::Debug,
{
    let dispatch = flume::unbounded::<Msg>();
    let mut ctx = create_ctx();

    ratatui::init();
    crossterm::execute!(std::io::stdout(), EnableMouseCapture)?;

    let (model, effect) = init();
    smol::spawn(effect.0.run_effect(dispatch.0.clone())).detach();
    let tree = view(&model);
    render(&mut ctx, &tree)?;

    let mut event_stream = smol::stream::race(
        dispatch
            .1
            .clone()
            .into_stream()
            .fuse()
            .map(RuntimeMessage::App),
        crossterm::event::EventStream::new()
            .fuse()
            .map(RuntimeMessage::Term),
    );
    let result = runtime(
        model,
        view,
        update,
        ctx,
        dispatch,
        quit_signal,
        None,
        &mut event_stream,
    )
    .await;

    ratatui::restore();

    result
}

type Ctx = Terminal<CrosstermBackend<Stdout>>;

fn create_ctx() -> Ctx {
    ratatui::init()
}

fn render<Msg: Clone + 'static>(ctx: &mut Ctx, tree: &Node<Msg>) -> std::io::Result<()> {
    ctx.draw(|frame| {
        tree.render(frame.area(), frame.buffer_mut());
    })
    .map(|_| ())
}

fn propagate_event<Msg>(ctx: &Ctx, tree: &Node<Msg>, event: crossterm::event::Event) {}

#[cfg(test)]
pub mod test_app {
    use std::time::Duration;

    use crossterm::event::{Event, KeyCode, KeyEvent};
    use ratatui::layout::{Constraint, Flex};

    use ratatui::widgets::{Block, BorderType, Padding};

    use crate::{CrosstermEventExt, run};
    use crate::{Effect, elements::*};

    #[derive(Debug, Default)]
    struct AppState {
        counter: isize,
        done_sleeping: bool,
    }

    #[derive(Debug, Clone)]
    enum Message {
        Quit,
        UserClickedIncrement,
        UserClickedDecrement,
        AppAwoken,
    }

    #[test]
    fn test() -> std::io::Result<()> {
        smol::block_on(run(init, view, update, |msg| matches!(msg, Message::Quit)))
    }

    fn init() -> (AppState, Effect<Message>) {
        (
            AppState::default(),
            Effect::new(async move |tx| {
                // i promise this is a network request
                smol::Timer::after(Duration::from_secs(3)).await;

                _ = tx.send_async(Message::AppAwoken).await;
            }),
        )
    }

    fn view(app: &AppState) -> Node<Message> {
        vstack([
            hstack([
                button("-1")
                    .size(Constraint::Max(4))
                    // add click and keybind handler
                    .on_click_keybind_down(
                        |event, _| event.is_keycode(KeyCode::Backspace),
                        Message::UserClickedDecrement,
                    ),
                paragraph(format!("Mmm cheese 🧀 {:08}", app.counter))
                    .size(Constraint::Max(24))
                    .block(Block::new().padding(Padding::uniform(1))),
                button("+1")
                    .size(Constraint::Max(4))
                    .on_click_down(Message::UserClickedIncrement),
            ])
            .flex(Flex::Center)
            .size(Constraint::Max(3)), // comment for rustfmt,
            // custom "component" vvv
            // components do not encapsulate state.
            the_sleeper(app),
        ])
        .flex(Flex::Center)
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title_top("Amazing application"),
        )
        // handler to send quit message
        // any node can access terminal events
        .on(|event, _| {
            if event.is_keycode(KeyCode::Char('q')) {
                Some(Message::Quit)
            } else {
                None
            }
        })
    }

    fn the_sleeper(model: &AppState) -> Node<Message> {
        hstack([
            if model.done_sleeping {
                paragraph("i am wide awake")
            } else {
                paragraph("i sleep...")
            }
            .size(Constraint::Max(16)), // another comment for rustfmt
        ])
        .size(Constraint::Length(1))
        .flex(Flex::Center)
    }

    fn update(model: AppState, msg: Message) -> (AppState, Effect<Message>) {
        let model = match msg {
            Message::UserClickedIncrement => AppState {
                counter: model.counter + 1,
                ..model
            },
            Message::UserClickedDecrement => AppState {
                counter: model.counter - 1,
                ..model
            },
            Message::AppAwoken => AppState {
                done_sleeping: true,
                ..model
            },
            Message::Quit => model,
        };
        (model, Effect::none())
    }
}
