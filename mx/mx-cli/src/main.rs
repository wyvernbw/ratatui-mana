#![feature(try_blocks)]

use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Instant;

use anyhow::Result;
use anyhow::anyhow;
use args::MxArgs;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use flume::Receiver;
use flume::Sender;
use mx_core::RenderMsg;
use mx_core::logging::DevServerLogCollector;
use notify::PollWatcher;
use notify::Watcher;
use portable_pty::{NativePtySystem, PtySize, PtySystem};
use ratatui::DefaultTerminal;
use ratatui::TerminalOptions;
use ratatui::Viewport;
use ratatui::crossterm;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use tachyonfx::Duration;
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

pub mod args;
pub mod ipc;
pub mod tui;

use mx_core::logging::RatatuiLayer;

use crate::ipc::IpcEvent;
use crate::ipc::OuterIpc;
use crate::tui::AppFx;

static SERVING: &str = r#" ----------------------------------------------------------
        💫 Serving your application!
        Press C-c to use the `mx` menu.
        Your app will automatically reload if you change the code.
        ----------------------------------------------------------"#;

fn main() -> Result<()> {
    let render_chan = flume::bounded(1024);
    let args = MxArgs::parse();
    let (x, y) = crossterm::terminal::size()?;
    let result = AppBridge::new(args, render_chan, (x, y)).run();
    ratatui::restore();
    result
}

#[derive(Debug)]
pub struct AppBridge {
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
    /// gives instructions to the ipc (mainly quit)
    ipc_chan: Chan<IpcEvent>,
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
    ShouldRun,
    Idle,
}

impl AppBridge {
    /// Construct a new instance of [`App`].
    pub fn new(args: MxArgs, render_chan: Chan<RenderMsg>, aspect: (u16, u16)) -> Self {
        Self {
            args,
            running: true.into(),
            aspect,
            render_chan,
            parser_chan: flume::bounded(32),
            focused: true.into(),
            ipc_chan: flume::bounded(32),
        }
    }

    /// Run the application's main loop.
    #[instrument(skip_all)]
    pub fn run(self) -> Result<()> {
        match &self.args.cmd {
            args::MxCommand::Serve(run) => {
                _ = tracing_subscriber::registry()
                    .with(
                        tracing_subscriber::EnvFilter::builder()
                            .with_default_directive("info".parse()?)
                            .from_env_lossy(),
                    )
                    .with(RatatuiLayer::new(self.render_chan.0.clone()))
                    .try_init();

                let mut terminal = ratatui::init_with_options(TerminalOptions {
                    viewport: Viewport::Inline(run.args.height as u16 * self.aspect.1 / 100),
                });
                // spawn the log collecter
                let dev_server_port = DevServerLogCollector::start(self.render_chan.0.clone())?;
                // spawn the inner executable
                let pty = NativePtySystem::default();

                // Create a new pty
                let size = self.get_pty_area(terminal.get_frame().area());
                let mut pair = pty
                    .openpty(PtySize {
                        rows: size.height,
                        cols: size.width,
                        pixel_width: 0,
                        pixel_height: 0,
                    })
                    .map_err(|err| anyhow!("{err}"))?;

                let parser = vt100::Parser::new(size.height, size.width, 0);

                let parser = RwLock::new(parser);
                let reader = pair.master.try_clone_reader().unwrap();
                let mut outer_ipc = ipc::OuterIpc::new()?;
                let child = outer_ipc.spawn(dev_server_port, &mut pair)?;
                let killer = child.clone_killer();
                let writer = pair.master.take_writer().map_err(|err| anyhow!("{err}"))?;
                let pair = Mutex::new(pair);

                std::thread::scope(|scope| -> Result<()> {
                    scope.spawn(|| self.term_reader(reader, killer));
                    tracing::trace!("started term reader");
                    scope.spawn(|| self.parser(&parser, writer, &pair));
                    tracing::trace!("started parser");
                    scope.spawn(|| self.run_ipc(outer_ipc));
                    tracing::trace!("started outer ipc");
                    scope.spawn(|| self.renderer(&parser, terminal));
                    tracing::trace!("started renderer");

                    self.ipc_chan
                        .0
                        .send(IpcEvent::Request(ipc::IpcMessage::Run(run.clone())))?;

                    Ok(())
                })
            }
            args::MxCommand::Ipc => ipc::IpcInner::new()?.run(),
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
                        .map_err(|err| anyhow!("{err}"))?;
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
        // set up state
        // DONE: refactor into a struct
        let mut state = RendererState::new();
        let ipc_sender = self.ipc_chan.0.clone();
        let mut watcher = PollWatcher::new(
            move |event: Result<notify::Event, _>| {
                let Ok(event) = event else {
                    return;
                };
                let is_rust_file = |paths: &[PathBuf]| {
                    paths
                        .iter()
                        .any(|path| path.extension().is_some_and(|ext| ext == "rs"))
                };
                match event.kind {
                    notify::EventKind::Create(_)
                    | notify::EventKind::Modify(_)
                    | notify::EventKind::Remove(_) => {
                        if is_rust_file(event.paths.as_slice()) {
                            _ = ipc_sender.send(IpcEvent::Request(ipc::IpcMessage::Reload));
                        }
                    }
                    _ => {}
                }
            },
            notify::Config::default()
                .with_manual_polling()
                .with_compare_contents(true),
        )?;
        watcher.watch(Path::new("."), notify::RecursiveMode::Recursive)?;
        loop {
            if !self.running.load(Ordering::Relaxed) {
                break Ok(());
            }
            if !self.focused.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100).into());
            }
            if crossterm::event::poll(Duration::from_millis(16).into())? {
                self.handle_crossterm_events(&mut state)?;
            }
            let dt = state.last_frame.elapsed();
            state.last_frame = Instant::now();
            for msg in self.render_chan.1.try_iter() {
                match self.handle_msg(msg, parser, &mut terminal, &mut state) {
                    RendererAction::ShouldQuit => break,
                    RendererAction::ShouldRender(sc) => {
                        state.screen = Some(sc);
                    }
                    RendererAction::Idle => {}
                    RendererAction::ShouldRun => {
                        tracing::info!("{}", SERVING);
                        std::thread::sleep(Duration::from_millis(500).into());
                        state.stage = AppStage::Running;
                    }
                }
            }
            let res = terminal.draw(|frame| {
                self.draw(frame, &mut state, dt.into());
            });
            if let Err(err) = res {
                tracing::warn!("failed to draw: {err}");
            }
            watcher.poll()?;
            if let Some(left) = Duration::from_millis(16).checked_sub(dt.into()) {
                std::thread::sleep(left.into());
            }
        }
    }

    #[instrument(skip_all, ret(level = Level::TRACE), err)]
    pub(crate) fn run_ipc(&self, ipc: OuterIpc) -> Result<()> {
        ipc.run(self)?;
        Ok(())
    }

    fn handle_msg(
        &self,
        msg: RenderMsg,
        parser: &RwLock<vt100::Parser>,
        terminal: &mut DefaultTerminal,
        state: &mut RendererState,
    ) -> RendererAction {
        match (msg, &mut state.stage) {
            (RenderMsg::Quit, _) => return RendererAction::ShouldQuit,
            (RenderMsg::Log(mut log), _) => {
                let height = log.create_line_and_get_height();
                _ = terminal.insert_before(height, |buf| {
                    log.render(buf.area, buf);
                });
            }
            (RenderMsg::Draw, _) => {
                // tracing::info!("draw");
                return RendererAction::ShouldRender(Box::new(
                    parser.read().unwrap().screen().clone(),
                ));
            }
            (RenderMsg::IpcBuildStarted(count, name), AppStage::StaringIpc) => {
                state.running_app = Some(name);
                state.start_build(count);
            }
            (RenderMsg::IpcBuildProgress, AppStage::Building(build_state)) => match build_state {
                RendererBuildState::Building { build_progress, .. } => {
                    *build_progress += 1;
                }
                RendererBuildState::Idle => {}
            },
            (RenderMsg::IpcBuildFinished, AppStage::Building(_)) => {
                state.finish_build();
                return RendererAction::ShouldRun;
            }
            _ => {}
        };
        RendererAction::Idle
    }

    /// Reads the crossterm events and updates the state of [`App`].
    fn handle_crossterm_events(&self, state: &mut RendererState) -> Result<()> {
        let event = crossterm::event::read();
        if let Ok(evt) = &event {
            match evt.clone() {
                Event::FocusLost => {
                    self.focused.store(false, Ordering::Release);
                }
                Event::FocusGained => {
                    self.focused.store(true, Ordering::Release);
                }
                Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key_event(state, key),
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

        if !state.mx_menu_open
            && self.focused.load(Ordering::Relaxed)
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
    fn on_key_event(&self, state: &mut RendererState, key: KeyEvent) {
        match (state.mx_menu_open, key.modifiers, key.code) {
            (true, _, KeyCode::Esc | KeyCode::Char('q')) => self.quit(),
            (false, KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => {
                state.mx_menu_open = true;
            }
            (true, KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => {
                state.mx_menu_open = false;
            }
            _ => {}
        }
    }

    /// Set running to false to quit the application.
    fn quit(&self) {
        tracing::info!("quit");
        self.running.store(false, Ordering::Release);
        while self.parser_chan.0.send(ParserMsg::Quit).is_err() {}
        while self.render_chan.0.send(RenderMsg::Quit).is_err() {}
        while self.ipc_chan.0.send(IpcEvent::Quit).is_err() {}
    }
}

pub(crate) struct RendererState {
    app_fx: AppFx,
    last_frame: Instant,
    screen: Option<Box<vt100::Screen>>,
    running_app: Option<String>,
    stage: AppStage,
    build_start: Instant,
    build_duration: Duration,
    mx_menu_open: bool,
}

pub(crate) enum AppStage {
    StaringIpc,
    Building(RendererBuildState),
    Running,
}

pub(crate) enum RendererBuildState {
    Building {
        build_max_progress: usize,
        build_progress: usize,
    },
    Idle,
}

impl RendererState {
    pub(crate) fn new() -> Self {
        let app_fx = AppFx {
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
            text_glitch_progress: 0.0,
        };
        Self {
            build_start: Instant::now(),
            build_duration: Duration::ZERO,
            app_fx,
            last_frame: Instant::now(),
            screen: None,
            running_app: None,
            stage: AppStage::StaringIpc,
            mx_menu_open: false,
        }
    }

    pub(crate) fn start_build(&mut self, build_max_progress: usize) {
        self.stage = AppStage::Building(RendererBuildState::Building {
            build_max_progress,
            build_progress: 0,
        });
        self.build_start = Instant::now();
    }

    pub(crate) fn finish_build(&mut self) {
        self.stage = AppStage::Building(RendererBuildState::Idle);
        self.build_duration = self.build_start.elapsed().into();
    }
}
