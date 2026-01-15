#![feature(mpmc_channel)]

use std::borrow::Cow;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::sync::Mutex;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Instant;

use color_eyre::{Result, eyre::eyre};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use flume::Receiver;
use flume::Sender;
use mx_core::RenderMsg;
use mx_core::args;
use mx_core::logging::DevServerLogCollector;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use ratatui::crossterm;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Flex;
use ratatui::layout::Layout;
use ratatui::layout::Margin;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Padding;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::{DefaultTerminal, Frame, TerminalOptions, Viewport};
use tachyonfx::Duration;
use tachyonfx::Effect;
use tachyonfx::EffectRenderer;
use tachyonfx::fx;
use tachyonfx::fx::RepeatMode;
use tachyonfx::pattern::SweepPattern;
use terminput::Encoding;
use terminput::KittyFlags;
use terminput_crossterm::to_terminput;
use tracing::Level;
use tracing::instrument;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tui_term::vt100;
use tui_term::widget::PseudoTerminal;

use mx_core::args::MxArgs;
use mx_core::logging::RatatuiLayer;

fn main() -> Result<()> {
    color_eyre::install()?;
    let render_chan = flume::bounded(1024);
    _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive("info".parse()?)
                .from_env_lossy(),
        )
        .with(RatatuiLayer::new(render_chan.0.clone()))
        .try_init();

    let args = MxArgs::parse();
    let (x, y) = crossterm::terminal::size()?;
    let terminal = ratatui::init_with_options(TerminalOptions {
        viewport: Viewport::Inline(args.height as u16 * y / 100),
    });
    let result = App::new(args, render_chan, (x, y)).run(terminal);
    ratatui::restore();
    result
}

#[derive(Debug)]
pub struct App {
    /// cli arguments
    args: MxArgs,
    /// Is the application running?
    running: AtomicBool,
    focused: AtomicBool,
    aspect: (u16, u16),
    /// tells the renderer to update
    render_chan: Chan<RenderMsg>,
    /// tells the parser to update
    parser_chan: Chan<ParserMsg>,
}

pub struct AppFx {
    title_hsl_shift: Option<Effect>,
}

impl AppFx {
    fn running(&self) -> bool {
        !self.title_hsl_shift.done()
    }
    fn advance(&mut self, dt: Duration, frame: &mut Frame) {
        self.title_hsl_shift.process_maybe(dt, frame);
    }
}

trait EffectExt {
    fn process_maybe(&mut self, dt: Duration, frame: &mut Frame);
    fn done(&self) -> bool;
}

impl EffectExt for Option<Effect> {
    fn process_maybe(&mut self, dt: Duration, frame: &mut Frame) {
        if let Some(fx) = self.as_mut()
            && let Some(area) = fx.area()
        {
            frame.render_effect(fx, area, dt);
        }
    }

    fn done(&self) -> bool {
        self.as_ref().map(|fx| fx.done()).unwrap_or(true)
    }
}

#[derive(Debug, Clone)]
pub enum ParserMsg {
    SetSize(u16, u16),
    Read(Box<[u8]>, usize),
    Write([u8; 16], usize),
    Quit,
}

type Chan<T> = (Sender<T>, Receiver<T>);

enum RendererAction {
    ShouldQuit,
    ShouldRender(Box<vt100::Screen>),
    Idle,
}

impl App {
    /// Construct a new instance of [`App`].
    pub fn new(args: MxArgs, render_chan: Chan<RenderMsg>, aspect: (u16, u16)) -> Self {
        Self {
            args,
            running: true.into(),
            aspect,
            render_chan,
            parser_chan: flume::bounded(32),
            focused: true.into(),
        }
    }

    /// Run the application's main loop.
    #[instrument(skip_all)]
    pub fn run(self, mut terminal: DefaultTerminal) -> Result<()> {
        match &self.args.cmd {
            args::MxCommand::Run { path } => {
                // spawn the log collecter
                let port = DevServerLogCollector::start(self.render_chan.0.clone())?;
                // spawn the inner executable
                let pty = NativePtySystem::default();
                let cwd = std::env::current_dir()?;

                // Create a new pty
                let size = self.get_pty_area(terminal.get_frame().area());
                let pair = pty
                    .openpty(PtySize {
                        rows: size.height,
                        cols: size.width,
                        pixel_width: 0,
                        pixel_height: 0,
                    })
                    .map_err(|err| eyre!("{err}"))?;
                let (shell, args) = if cfg!(target_os = "windows") {
                    todo!();
                // (, ["/C"].as_slice())
                } else {
                    (
                        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
                        ["-l", "-c"].as_slice(),
                    )
                };
                let mut cmd = CommandBuilder::new(shell);
                cmd.cwd(cwd);
                cmd.args(args);
                cmd.arg(path);
                cmd.env("MX_DEV_SERVER_PORT", port.to_string());
                let child = pair
                    .slave
                    .spawn_command(cmd)
                    .map_err(|err| eyre!("{err}"))?;

                let parser = vt100::Parser::new(size.height, size.width, 0);

                let parser = RwLock::new(parser);
                let reader = pair.master.try_clone_reader().unwrap();
                let killer = child.clone_killer();
                let writer = pair.master.take_writer().map_err(|err| eyre!("{err}"))?;
                let pair = Mutex::new(pair);

                std::thread::scope(|scope| {
                    scope.spawn(|| self.term_reader(reader, killer));
                    scope.spawn(|| self.parser(&parser, writer, &pair));
                    scope.spawn(|| self.renderer(&parser, terminal));
                });

                Ok(())
            }
        }
    }

    #[instrument(skip_all, ret(level = Level::TRACE), err)]
    fn term_reader(
        &self,
        mut reader: Box<dyn std::io::Read + Send>,
        mut killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    ) -> Result<()> {
        loop {
            if !self.running.load(Ordering::Relaxed) {
                killer.kill()?;
                break;
            }
            let mut temp = [0u8; 124];
            if let Ok(n) = reader.read(&mut temp) {
                if n == 0 {
                    tracing::info!("terminal connection dropped");
                    break;
                }
                // tracing::info!("{n}");
                self.parser_chan.0.send(ParserMsg::Read(temp.into(), n))?;
            }
        }
        Ok(())
    }

    #[instrument(skip_all, ret(level = Level::TRACE), err)]
    fn parser(
        &self,
        parser: &RwLock<vt100::Parser>,
        mut writer: Box<dyn std::io::Write + Send>,
        pair: &Mutex<portable_pty::PtyPair>,
    ) -> Result<()> {
        for msg in self.parser_chan.1.iter() {
            // tracing::info!("{msg:?}");
            if !self.running.load(Ordering::Relaxed) {
                break;
            }
            match msg {
                ParserMsg::SetSize(w, h) => {
                    parser.write().unwrap().set_size(h, w);
                    pair.lock()
                        .unwrap()
                        .master
                        .resize(PtySize {
                            rows: h,
                            cols: w,
                            pixel_width: 0,
                            pixel_height: 0,
                        })
                        .map_err(|err| eyre!("{err}"))?;
                    _ = self.render_chan.0.send(RenderMsg::Draw);
                }
                ParserMsg::Read(buffer, n) => {
                    parser.write().unwrap().process(&buffer[..n]);
                    _ = self.render_chan.0.send(RenderMsg::Draw);
                }
                ParserMsg::Write(buffer, n) => {
                    writer.write_all(&buffer[..n])?;
                }
                ParserMsg::Quit => break,
            }
        }
        Ok(())
    }

    #[instrument(skip_all, ret(level = Level::TRACE), err)]
    fn renderer(
        &self,
        parser: &RwLock<vt100::Parser>,
        mut terminal: DefaultTerminal,
    ) -> Result<()> {
        let mut app_fx = AppFx {
            title_hsl_shift: Some(fx::repeat(
                fx::parallel(&[
                    fx::hsl_shift_fg([0.0, 0.0, 30.0], 1000)
                        .with_pattern(SweepPattern::left_to_right(3)),
                    fx::delay(
                        200,
                        fx::hsl_shift_fg([0.0, 0.0, -30.0], 800)
                            .with_pattern(SweepPattern::left_to_right(3)),
                    ),
                ]),
                RepeatMode::Forever,
            )),
        };
        let mut last_frame = Instant::now();
        let mut screen = None;
        loop {
            if !self.running.load(Ordering::Relaxed) {
                break Ok(());
            }
            if !self.focused.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100).into());
            }
            if crossterm::event::poll(Duration::from_millis(16).into())? {
                self.handle_crossterm_events()?;
            }
            let dt = last_frame.elapsed();
            last_frame = Instant::now();
            for msg in self.render_chan.1.try_iter() {
                match self.handle_msg(msg, parser, &mut terminal, &mut app_fx, dt.into()) {
                    RendererAction::ShouldQuit => break,
                    RendererAction::ShouldRender(sc) => {
                        screen = Some(sc);
                    }
                    RendererAction::Idle => {}
                }
            }
            if let Some(ref screen) = screen {
                let res = terminal.draw(|frame| {
                    self.draw(frame, screen, &mut app_fx, dt.into());
                });
                if let Err(err) = res {
                    tracing::warn!("failed to draw: {err}");
                }
            }
            if let Some(left) = Duration::from_millis(16).checked_sub(dt.into()) {
                std::thread::sleep(left.into());
            }
        }
    }

    fn handle_msg(
        &self,
        msg: RenderMsg,
        parser: &RwLock<vt100::Parser>,
        terminal: &mut DefaultTerminal,
        app_fx: &mut AppFx,
        dt: Duration,
    ) -> RendererAction {
        match msg {
            RenderMsg::Quit => return RendererAction::ShouldQuit,
            RenderMsg::Log(log) => {
                _ = terminal.insert_before(1, |buf| {
                    log.render(buf.area, buf);
                });
            }
            RenderMsg::Draw => {
                // tracing::info!("draw");
                return RendererAction::ShouldRender(Box::new(
                    parser.read().unwrap().screen().clone(),
                ));
            }
        };
        RendererAction::Idle
    }

    fn get_pty_area(&self, area: Rect) -> Rect {
        let width = area.height * self.aspect.0 / self.aspect.1;
        Layout::horizontal([Constraint::Max(width)])
            .flex(Flex::Center)
            .areas::<1>(area)[0]
    }

    fn running_exec(&self) -> Cow<'_, OsStr> {
        match self.args.cmd {
            args::MxCommand::Run { ref path } => path
                .file_stem()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Owned(OsString::new())),
        }
    }

    /// Renders the user interface.
    ///
    /// This is where you add new widgets. See the following resources for more information:
    /// - <https://docs.rs/ratatui/latest/ratatui/widgets/index.html>
    /// - <https://github.com/ratatui/ratatui/tree/master/examples>
    fn draw(&self, frame: &mut Frame, screen: &vt100::Screen, fx: &mut AppFx, dt: Duration) {
        let title_text = format!("running {}", self.running_exec().display());
        let title_len = title_text.len();
        let title_text = format!(" 📺 {} ", title_text);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().dim())
            .padding(Padding::uniform(1))
            .title_top(title_text);
        if let Some(fx) = &mut fx.title_hsl_shift {
            let [title_area] =
                Layout::new(Direction::Vertical, [Constraint::Length(1)]).areas(frame.area());
            let [_, title_area] = Layout::new(
                Direction::Horizontal,
                [
                    Constraint::Length(32 + 2),
                    Constraint::Length(title_len as u16),
                ],
            )
            .areas(title_area);
            frame.render_effect(fx, title_area, dt);
        }
        let area = self.get_pty_area(frame.area()).outer(Margin {
            horizontal: 1,
            vertical: 1,
        });
        frame.render_widget(&block, area);
        let screen_area = block.inner(area);

        let term = PseudoTerminal::new(screen);
        // let term = Paragraph::new("I am terminal").centered();
        frame.render_widget(term, screen_area);
    }

    /// Reads the crossterm events and updates the state of [`App`].
    fn handle_crossterm_events(&self) -> Result<()> {
        let event = crossterm::event::read();
        if let Ok(evt) = &event {
            match evt.clone() {
                Event::FocusLost => {
                    self.focused.store(false, Ordering::Release);
                }
                Event::FocusGained => {
                    self.focused.store(true, Ordering::Release);
                }
                Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key_event(key),
                Event::Mouse(_) => {}
                Event::Resize(w, h) => {
                    let area = self.get_pty_area(Rect {
                        x: 0,
                        y: 0,
                        width: w,
                        height: h,
                    });
                    _ = self
                        .parser_chan
                        .0
                        .send(ParserMsg::SetSize(area.width, area.height));
                }
                _ => {}
            }
        };

        if self.focused.load(Ordering::Relaxed)
            && let Ok(event) = event
        {
            let mut buf = [0; 16];
            let event = to_terminput(event)?;
            let written = event.encode(&mut buf, Encoding::Kitty(KittyFlags::all()));
            if let Ok(written) = written {
                self.parser_chan.0.send(ParserMsg::Write(buf, written))?;
            }
        }

        Ok(())
    }

    /// Handles the key events and updates the state of [`App`].
    fn on_key_event(&self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => self.quit(),
            // Add other key handlers here.
            _ => {}
        }
    }

    /// Set running to false to quit the application.
    fn quit(&self) {
        tracing::info!("quit");
        self.running.store(false, Ordering::Release);
        while self.parser_chan.0.send(ParserMsg::Quit).is_err() {}
        while self.render_chan.0.send(RenderMsg::Quit).is_err() {}
    }
}
