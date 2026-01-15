use std::{collections::HashMap, sync::Arc};

use flume::Sender;
use ratatui::{
    prelude::{Buffer, Rect},
    style::{
        Color, Style,
        palette::{self, material},
    },
    text::{self, Line},
    widgets::Widget,
};
use tracing::Level;
use tracing_subscriber::Layer;

use crate::RenderMsg;

pub struct RatatuiLayer {
    sender: Sender<RenderMsg>,
}

impl RatatuiLayer {
    pub fn new(sender: Sender<RenderMsg>) -> Self {
        Self { sender }
    }
}

impl<S> Layer<S> for RatatuiLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = event.metadata();

        // Extract message
        let mut visitor = MxVisitor::default();
        event.record(&mut visitor);
        let trace = Trace {
            level: *metadata.level(),
            message: visitor.message,
        };

        // Send to channel (ignore errors if receiver dropped)
        let _ = self.sender.send(RenderMsg::Log(trace));
    }
}

type Str = Arc<str>;

#[derive(Default)]
struct MxVisitor {
    message: Str,
    fields: Vec<(Str, Str)>,
}

impl tracing::field::Visit for MxVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value).into();
        } else {
            let name = format!("{:?}", field.name()).into();
            let value = format!("{value:?}").into();
            self.fields.push((name, value));
        }
    }
}

#[derive(Debug, Clone)]
pub struct Trace {
    level: Level,
    message: Arc<str>,
}

impl Trace {
    fn color(&self) -> Color {
        match self.level {
            Level::INFO => Color::Green,
            Level::DEBUG => Color::Blue,
            Level::TRACE => Color::Magenta,
            Level::WARN => Color::Yellow,
            Level::ERROR => Color::Red,
        }
    }
}

impl Widget for Trace {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let level =
            text::Span::raw(format!("[{}] ", self.level)).style(Style::new().fg(self.color()));
        let message =
            text::Span::raw(format!("{}", self.message)).style(Style::new().fg(Color::White).dim());
        let line = Line::from_iter([level, message]);
        let area = area.centered_horizontally(ratatui::layout::Constraint::Ratio(1, 2));
        line.render(area, buf);
    }
}
