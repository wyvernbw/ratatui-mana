use ratatui::prelude::Backend;

use crate::{Chan, RuntimeMsg};

pub trait ManaBackend: Backend {
    type Events: EventStream;
    type KeyEvent;
    type Event;

    #[allow(async_fn_in_trait)]
    async fn create_events(&mut self) -> Self::Events;

    fn default_cycle_event() -> Self::Event;

    fn event_as_key(ev: Self::Event) -> Option<Self::KeyEvent>;
}

pub trait EventStream {
    type Out;
    type Err;

    #[allow(async_fn_in_trait)]
    async fn read(&mut self) -> Result<Self::Out, Self::Err>;
}

pub(crate) struct MsgStream<Msg> {
    pub(crate) event_stream: <DefaultBackend<std::io::Stdout> as ManaBackend>::Events,
    pub(crate) dispatch: Chan<Msg>,
}

impl<Msg> MsgStream<Msg> {
    pub(crate) async fn next(this: &mut Self) -> RuntimeMsg<Msg> {
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

#[cfg(feature = "crossterm")]
pub(crate) mod crossterm_backend {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use ratatui::prelude::CrosstermBackend;
    use tokio_stream::StreamExt;

    use crate::backends::{EventStream, ManaBackend};

    impl<W: std::io::Write> ManaBackend for CrosstermBackend<W> {
        type Events = crossterm::event::EventStream;
        type KeyEvent = crossterm::event::KeyEvent;
        type Event = crossterm::event::Event;

        async fn create_events(&mut self) -> Self::Events {
            crossterm::event::EventStream::new()
        }

        fn default_cycle_event() -> <Self::Events as EventStream>::Out {
            Event::Key(crossterm::event::KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::empty(),
                kind: KeyEventKind::Press,
                state: KeyEventState::empty(),
            })
        }

        fn event_as_key(ev: Self::Event) -> Option<Self::KeyEvent> {
            ev.as_key_event()
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
    pub type DefaultKeyEvent = <DefaultBackend<std::io::Stdout> as ManaBackend>::KeyEvent;

    pub trait KeyEventExt {
        #[must_use]
        fn char(c: char) -> KeyEvent {
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::empty(),
            }
        }
    }

    impl KeyEventExt for KeyEvent {}
}

#[cfg(feature = "crossterm")]
pub use crossterm_backend::*;
