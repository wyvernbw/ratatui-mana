use std::{
    fmt::Display,
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
};

use flume::Sender;
use ratatui::{
    prelude::{Buffer, Rect},
    style::{Color, Style, Styled},
    text::{self, Line, ToSpan},
    widgets::{Paragraph, Widget},
};
use serde::{Deserialize, Serialize, de::Visitor};
use tracing::Level;
use tracing_subscriber::{Layer, registry::LookupSpan};

use crate::RenderMsg;
use anyhow::Result;

trait MxLayer {
    fn send(&self, trace: Trace);
}

pub struct MxLayerImpl<L>(L);

impl<S, L> Layer<S> for MxLayerImpl<L>
where
    L: MxLayer + 'static,
    S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let metadata = event.metadata();

        // Extract message
        let mut visitor = MxVisitor::default();
        event.record(&mut visitor);

        let trace = Trace {
            level: MxLevel(*metadata.level()),
            message: visitor.message,
            fields: visitor.fields,
            span_data: None,
            widget: None,
        };

        self.0.send(trace);
    }
}

pub struct RatatuiLayer {
    sender: Sender<RenderMsg>,
}

impl RatatuiLayer {
    pub fn new(sender: Sender<RenderMsg>) -> MxLayerImpl<Self> {
        MxLayerImpl(Self { sender })
    }
}

impl MxLayer for RatatuiLayer {
    fn send(&self, trace: Trace) {
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
            return;
        };
        let name = match field.name() {
            "return" => "ret",
            "error" => "err",
            name => name,
        };
        let name = name.to_string().into();
        let value = format!("{value:?}").into();
        self.fields.push((name, value));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    level: MxLevel,
    message: Str,
    fields: Vec<(Str, Str)>,
    span_data: Option<Vec<SpanData>>,
    #[serde(skip)]
    widget: Option<Paragraph<'static>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanData {
    span_name: Str,
    fields: Vec<(Str, Str)>,
}

#[derive(Debug, Clone)]
struct MxLevel(Level);

impl Display for MxLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for MxLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(match self.0 {
            Level::INFO => 0,
            Level::DEBUG => 1,
            Level::TRACE => 2,
            Level::WARN => 3,
            Level::ERROR => 4,
        })
    }
}

struct LevelVisitor;

impl<'v> Visitor<'v> for LevelVisitor {
    type Value = MxLevel;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "an integer between 0 and 4")
    }

    fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match v {
            0 => Ok(MxLevel(Level::INFO)),
            1 => Ok(MxLevel(Level::DEBUG)),
            2 => Ok(MxLevel(Level::TRACE)),
            3 => Ok(MxLevel(Level::WARN)),
            4 => Ok(MxLevel(Level::ERROR)),
            n => Err(E::custom(format!("invalid number {n} for level"))),
        }
    }
}

impl<'de> Deserialize<'de> for MxLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_u8(LevelVisitor)
    }
}

impl Trace {
    fn color(&self) -> Color {
        match self.level.0 {
            Level::INFO => Color::Green,
            Level::DEBUG => Color::Blue,
            Level::TRACE => Color::Magenta,
            Level::WARN => Color::Yellow,
            Level::ERROR => Color::Red,
        }
    }
}

impl Trace {
    pub fn create_line_and_get_height(&mut self) -> u16 {
        let fields = self
            .fields
            .iter()
            .map(|(name, value)| format!("{}={value} ", name.set_style(Style::new().italic())))
            .collect::<String>();
        let fields = text::Span::raw(fields).style(Style::new().fg(Color::White).dim());
        let level =
            text::Span::raw(format!("[{}] ", self.level)).style(Style::new().fg(self.color()));
        let mut message = self
            .message
            .lines()
            .map(|line| Line::<'static>::raw(line.to_string()))
            .collect::<Vec<_>>();
        let height = message.len() as u16;
        let first: Line<'static> = message[0].clone();
        let first = text::Span::raw(first.spans[0].to_string());
        let new_line = Line::from_iter([level, fields, first]);
        message[0] = new_line;
        let message = Paragraph::new(message).style(Style::new().dim());
        self.widget = Some(message);
        height
    }
}

impl Widget for Trace {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        if let Some(line) = self.widget {
            let area = area.centered_horizontally(ratatui::layout::Constraint::Ratio(1, 2));
            line.render(area, buf);
        }
    }
}

pub struct DevClientLayer {
    enabled: bool,
    tcp: Option<Mutex<TcpStream>>,
}

impl MxLayer for DevClientLayer {
    fn send(&self, trace: Trace) {
        if !self.enabled {
            return;
        }
        let Some(tcp) = &self.tcp else {
            return;
        };
        let mut tcp = tcp.lock().unwrap();
        let mut serializer = dlhn::Serializer::new(&mut *tcp);
        _ = trace.serialize(&mut serializer);
    }
}

impl DevClientLayer {
    pub fn new() -> MxLayerImpl<Self> {
        let Ok(port) = std::env::var("MX_DEV_SERVER_PORT") else {
            return MxLayerImpl(Self {
                enabled: false,
                tcp: None,
            });
        };
        let Ok(tcp) = TcpStream::connect(format!("127.0.0.1:{port}")) else {
            return MxLayerImpl(Self {
                enabled: false,
                tcp: None,
            });
        };
        MxLayerImpl(Self {
            tcp: Some(Mutex::new(tcp)),
            enabled: true,
        })
    }
}

pub fn layer() -> MxLayerImpl<DevClientLayer> {
    DevClientLayer::new()
}

pub struct DevServerLogCollector;

impl DevServerLogCollector {
    // returns the port of the collector
    pub fn start(tx: Sender<RenderMsg>) -> Result<u16> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let socket = listener.local_addr()?;
        tracing::trace!("listening on {}", listener.local_addr()?);

        let handle = std::thread::spawn(move || -> Result<()> {
            loop {
                let (mut stream, _) = listener.accept()?;
                tracing::trace!("accepted connection");
                loop {
                    let mut deserializer = dlhn::Deserializer::new(&mut stream);
                    let trace = Trace::deserialize(&mut deserializer);
                    match trace {
                        Ok(trace) => {
                            tx.send(RenderMsg::Log(trace))?;
                        }
                        Err(dlhn::de::Error::Read) => break,
                        Err(err) => {
                            tracing::warn!("dev server error: {err}");
                        }
                    }
                }
            }
        });

        // bon voyage
        drop(handle);

        Ok(socket.port())
    }
}
