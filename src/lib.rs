#![feature(async_fn_traits)]
#![feature(unboxed_closures)]
use std::{
    any::Any,
    cell::{Cell, RefCell},
    marker::PhantomData,
    pin::Pin,
};

use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use flume::{Receiver, RecvError, Sender, r#async::RecvFut};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    widgets::{Block, Paragraph, Widget},
};
use slab::Slab;
use smol::stream::NextFuture;

type Handler = Box<dyn Fn(Event) -> Pin<Box<dyn Future<Output = ()>>>>;

pub struct Ctx {
    hook_idx: Cell<usize>,
    handler_idx: Cell<usize>,
    event_sender: Sender<Event>,
    event_receiver: Receiver<Event>,
    signal_sender: Sender<()>,
    signal_receiver: Receiver<()>,
    state: RefCell<Slab<Box<RefCell<dyn Any + 'static>>>>,
    handlers: RefCell<Slab<Handler>>,
}

impl Default for Ctx {
    fn default() -> Self {
        let (event_sender, event_receiver) = flume::unbounded();
        let (signal_sender, signal_receiver) = flume::unbounded();
        Self {
            hook_idx: Cell::new(0),
            handler_idx: Cell::new(0),
            event_sender,
            event_receiver,
            signal_sender,
            signal_receiver,
            state: RefCell::new(Slab::new()),
            handlers: RefCell::new(Slab::new()),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Signal<T> {
    ctx: &'static Ctx,
    id: usize,
    _ty: PhantomData<T>,
}

impl Ctx {
    pub fn create_signal<T>(&'static self, value: T) -> Signal<T>
    where
        T: 'static,
    {
        let idx = self.hook_idx.get();
        if self.state.borrow().get(idx).is_some() {
            Signal {
                id: idx,
                ctx: self,
                _ty: PhantomData,
            }
        } else {
            let id = self
                .state
                .borrow_mut()
                .insert(Box::new(RefCell::new(value)));
            self.hook_idx.set(id);
            Signal {
                id,
                ctx: self,
                _ty: PhantomData,
            }
        }
    }

    pub fn create_handler<F: Future<Output = ()> + 'static>(
        &'static self,
        f: impl Fn(Event) -> F + 'static,
    ) {
        // check already existing handler
        let mut handlers = self.handlers.borrow_mut();
        let id = self.handler_idx.get();
        if handlers.contains(id) {
            return;
        }

        // append new handler
        let f = move |event| {
            let future = f(event);
            Box::pin(future) as Pin<Box<dyn Future<Output = ()> + 'static>>
        };
        let id = handlers.insert(Box::new(f));

        // set handler index
        self.handler_idx.set(id)
    }

    async fn handle_event(&'static self, event: Event) {
        let futures = {
            let mut handlers = self.handlers.borrow_mut();
            let mut futures = Vec::with_capacity(handlers.len());

            for (_, handler) in handlers.iter_mut() {
                let event = event.clone();
                futures.push(handler(event));
            }
            futures
        };

        for future in futures {
            future.await
        }
    }

    fn clear_hooks(&'static self) {
        self.hook_idx.set(0);
    }
}

impl<T: 'static + Clone> Signal<T> {
    fn get(&self) -> T {
        let state = self.ctx.state.borrow();
        let value = state[self.id].borrow();
        let value = value.downcast_ref::<T>().cloned();
        value.unwrap()
    }

    fn set(&self, value: T) {
        {
            let state = self.ctx.state.borrow_mut();
            let mut signal = state[self.id].borrow_mut();
            let signal = signal.downcast_mut::<T>().unwrap();
            *signal = value;
        }
        self.ctx.signal_sender.send(()).unwrap();
    }
}

impl<T: 'static> Signal<T> {
    fn update(&self, f: impl Fn(&mut T)) {
        let state = self.ctx.state.borrow_mut();
        let mut value = state[self.id].borrow_mut();
        let value = value.downcast_mut::<T>().unwrap();
        f(value);
        self.ctx.signal_sender.send(()).unwrap();
    }
}

pub trait ComponentMarker: std::fmt::Debug {
    fn children(&self) -> Option<&[Component]> {
        None
    }
    fn sizing(&self) -> Option<Constraint> {
        None
    }
    fn layout(&self, sizes: &[Constraint]) -> Option<Layout> {
        None
    }
    fn render(&self, area: Rect, buf: &mut Buffer) {}
}

pub type Component = Box<dyn ComponentMarker>;

pub struct ComponentProps;

pub fn props() -> ComponentProps {
    ComponentProps
}

impl<W: Widget + std::fmt::Debug + Clone> ComponentMarker for W {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        Widget::render(self.clone(), area, buf);
    }
}

pub trait IntoComponent {
    fn into_component(self) -> Component;
}
impl<T> IntoComponent for T
where
    T: ComponentMarker + 'static,
{
    fn into_component(self) -> Component {
        Box::new(self) as _
    }
}

impl IntoComponent for Component {
    fn into_component(self) -> Component {
        self
    }
}

#[derive(Debug)]
pub struct ChildrenWrapper<C: ComponentMarker> {
    base: C,
    children: Vec<Component>,
}

pub trait IntoComponentList {
    fn into_component_list(self) -> Vec<Component>;
}

macro_rules! impl_into_component_list_tuples {
    ($($T:ident),+) => {
        impl<$($T: IntoComponent),+> IntoComponentList for ($($T,)+) {
            fn into_component_list(self) -> Vec<Component> {
                #[allow(non_snake_case)]
                let ($($T,)+) = self;
                vec![$($T.into_component()),+]
            }
        }
    };
}

impl_into_component_list_tuples!(T1);
impl_into_component_list_tuples!(T1, T2);
impl_into_component_list_tuples!(T1, T2, T3);
impl_into_component_list_tuples!(T1, T2, T3, T4);
impl_into_component_list_tuples!(T1, T2, T3, T4, T5);
impl_into_component_list_tuples!(T1, T2, T3, T4, T5, T6);
impl_into_component_list_tuples!(T1, T2, T3, T4, T5, T6, T7);
impl_into_component_list_tuples!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_into_component_list_tuples!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_into_component_list_tuples!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_into_component_list_tuples!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_into_component_list_tuples!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

impl IntoComponentList for Vec<Component> {
    fn into_component_list(self) -> Vec<Component> {
        self
    }
}

impl<const N: usize, T> IntoComponentList for [T; N]
where
    T: IntoComponent,
{
    fn into_component_list(self) -> Vec<Component> {
        self.into_iter().map(|el| el.into_component()).collect()
    }
}

pub trait WithChildren: Sized + ComponentMarker {
    fn with(self, children: impl IntoComponentList) -> ChildrenWrapper<Self> {
        ChildrenWrapper {
            base: self,
            children: children.into_component_list(),
        }
    }
}

impl<C: ComponentMarker> ComponentMarker for ChildrenWrapper<C> {
    fn children(&self) -> Option<&[Component]> {
        Some(&self.children)
    }
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let layout = self
            .children
            .iter()
            .map(|node| node.sizing().unwrap_or(Constraint::Fill(1)))
            .collect::<Vec<_>>();
        let layout = self.base.layout(&layout);
        let layout = layout
            .unwrap_or_else(|| Layout::vertical([Constraint::Fill(1)].repeat(self.children.len())));
        let areas = layout.split(area);
        for (area, node) in areas.iter().zip(self.children.iter()) {
            node.render(*area, buf);
        }
    }
}

#[derive(Debug)]
pub struct HStack;

impl ComponentMarker for HStack {
    fn layout(&self, sizes: &[Constraint]) -> Option<Layout> {
        Some(Layout::horizontal(sizes))
    }
}

#[derive(Debug)]
pub struct VStack;

impl ComponentMarker for VStack {
    fn layout(&self, sizes: &[Constraint]) -> Option<Layout> {
        Some(Layout::vertical(sizes))
    }
}

#[derive(Debug)]
pub struct BlockWrapper<'a, C> {
    base: C,
    block: Block<'a>,
}

impl<'a, C> ComponentMarker for BlockWrapper<'a, C>
where
    C: ComponentMarker,
{
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let inner = self.block.inner(area);
        Widget::render(self.block.clone(), area, buf);
        self.base.render(inner, buf);
    }
}

impl<C> WithChildren for C where C: IntoComponent + std::fmt::Debug + ComponentMarker {}

trait WithBlock: Sized + ComponentMarker {
    fn block(self, block: Block) -> BlockWrapper<Self> {
        BlockWrapper { base: self, block }
    }
}

impl<C> WithBlock for C where C: IntoComponent + std::fmt::Debug + ComponentMarker {}

enum CtxEvent {
    CrosstermEvent(Option<Result<Event, std::io::Error>>),
    SignalEvent(Result<(), RecvError>),
}

trait CtxEventFuture {
    async fn into_ctx_event(self) -> CtxEvent;
}

impl<'a> CtxEventFuture for NextFuture<'a, EventStream> {
    async fn into_ctx_event(self) -> CtxEvent {
        let result = self.await;
        CtxEvent::CrosstermEvent(result)
    }
}

impl<'a> CtxEventFuture for RecvFut<'a, ()> {
    async fn into_ctx_event(self) -> CtxEvent {
        let result = self.await;
        CtxEvent::SignalEvent(result)
    }
}

fn my_block() -> Block<'static> {
    Block::bordered().border_type(ratatui::widgets::BorderType::Rounded)
}

#[housecat_macros::component]
pub fn test_component(ctx: &'static Ctx) -> impl IntoComponent {
    HStack.with((
        Paragraph::new("Hello world!").block(my_block()),
        Paragraph::new("I am an application").block(my_block()),
        custom_component(ctx).block(my_block()),
    ))
}

#[housecat_macros::component]
fn custom_component(ctx: &'static Ctx) -> impl IntoComponent {
    VStack.with((
        Paragraph::new("custom component").block(my_block()),
        counter(ctx),
        Paragraph::new("Custom component magic child!").block(my_block()),
    ))
}

#[housecat_macros::component]
fn counter(ctx: &'static Ctx) -> impl IntoComponent {
    let value = ctx.create_signal(0);

    ctx.create_handler(move |event| async move {
        let Event::Key(key_event) = event else { return };
        if !matches!(key_event.kind, KeyEventKind::Press) {
            return;
        }

        match key_event.code {
            KeyCode::Char('a') => value.update(|x| *x += 1),
            KeyCode::Char('b') => value.update(|x| *x -= 1),
            _ => {}
        }
    });

    HStack
        .with((
            "+1(a)".block(my_block()),
            format!("value: {}", value.get()).block(my_block()),
            "-1(b)".block(my_block()),
        ))
        .block(my_block())
}

#[cfg(test)]
mod tests {

    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState};
    use ratatui::widgets::Block;
    use smol::{future::FutureExt, stream::StreamExt};

    use crate::ComponentMarker;
    use crate::Ctx;
    use crate::CtxEventFuture;
    use crate::test_component;

    #[test]
    fn test() {
        let mut terminal = ratatui::init();
        let ctx: &'static Ctx = Box::leak(Box::default());
        smol::block_on(async {
            let mut redraw = || {
                ctx.clear_hooks();
                let root = test_component(ctx);
                terminal
                    .draw(|frame| root.render(frame.area(), frame.buffer_mut()))
                    .unwrap();
            };
            redraw();
            let mut stream = crossterm::event::EventStream::new();
            loop {
                let event = stream
                    .next()
                    .into_ctx_event()
                    .race(ctx.signal_receiver.recv_async().into_ctx_event());
                match event.await {
                    crate::CtxEvent::CrosstermEvent(Some(Ok(event))) => {
                        if let Event::Key(KeyEvent {
                            code: KeyCode::Char('q'),
                            kind: KeyEventKind::Press,
                            ..
                        }) = event
                        {
                            break;
                        }
                        // ctx.event_sender.send_async(event).await.unwrap();
                        ctx.handle_event(event).await;
                    }
                    crate::CtxEvent::CrosstermEvent(None) => {}
                    crate::CtxEvent::SignalEvent(Ok(())) => {
                        redraw();
                    }
                    crate::CtxEvent::CrosstermEvent(Some(Err(err))) => {
                        panic!("{}", err)
                    }
                    crate::CtxEvent::SignalEvent(Err(err)) => panic!("{}", err),
                }
            }
        });
        ratatui::restore();
    }
}
