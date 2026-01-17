use std::{
    net::{TcpListener, TcpStream},
    process::Stdio,
    sync::atomic::Ordering,
};

use anyhow::Result;
use anyhow::anyhow;
use cargo_metadata::{CargoOpt, MetadataCommand};
use escargot::{CargoBuild, CommandMessages, format::BuildFinished};
use mx_core::RenderMsg;
use portable_pty::{Child, CommandBuilder, PtyPair};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{AppBridge, args};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum IpcMessage {
    Run(args::Run),
    Reload,
    Kill,
    InnerProgressUpdate(InnerProgressUpdate),
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum InnerProgressUpdate {
    Progress,
    BuildStarted(usize, String),
    BuildFinished(BuildFinished),
}

pub(crate) struct IpcInner {
    running: Option<args::Run>,
    stream: TcpStream,
}

impl IpcInner {
    pub fn new() -> Result<Self> {
        let port = std::env::var("MX_IPC_PORT")?.parse::<u16>()?;
        let stream = TcpStream::connect(format!("127.0.0.1:{port}"))?;
        Ok(Self {
            running: None,
            stream,
        })
    }

    pub fn send(&mut self, msg: IpcMessage) -> Result<()> {
        let mut serializer = dlhn::Serializer::new(&mut self.stream);
        msg.serialize(&mut serializer)?;
        Ok(())
    }

    #[instrument(err, skip_all)]
    pub fn run(mut self) -> Result<()> {
        mx_core::init();
        loop {
            let mut deserializer = dlhn::Deserializer::new(&mut self.stream);
            let msg = IpcMessage::deserialize(&mut deserializer)?;
            match msg {
                IpcMessage::InnerProgressUpdate(_) => {}
                IpcMessage::Run(run) => {
                    let build_cmd = || {
                        let mut metadata = MetadataCommand::new();
                        metadata
                            .features(CargoOpt::SomeFeatures(run.features_args.features.clone()));
                        let build_cmd =
                            CargoBuild::new().features(run.features_args.features.join(" "));
                        let build_cmd = if run.features_args.all_features {
                            metadata.features(CargoOpt::AllFeatures);
                            build_cmd.all_features()
                        } else {
                            build_cmd
                        };
                        let build_cmd = if run.features_args.no_default_features {
                            metadata.features(CargoOpt::NoDefaultFeatures);
                            build_cmd.no_default_features()
                        } else {
                            build_cmd
                        };
                        let build_cmd = if let [package, ..] = run.workspace_args.package.as_slice()
                        {
                            // metadata.other_options(["-p".to_string(), package.to_string()]);
                            build_cmd.package(package)
                        } else {
                            build_cmd
                        };
                        (metadata, build_cmd)
                    };
                    let (metadata, cmd) = build_cmd();
                    let metadata = metadata.exec()?;
                    let mut cmd = cmd.into_command();
                    self.send(IpcMessage::InnerProgressUpdate(
                        InnerProgressUpdate::BuildStarted(
                            metadata
                                .resolve
                                .as_ref()
                                .map(|r| r.nodes.len())
                                .unwrap_or(0),
                            run.workspace_args
                                .package
                                .first()
                                .cloned()
                                .or(metadata.root_package().map(|p| p.name.to_string()))
                                .unwrap_or_default(),
                        ),
                    ))?;
                    cmd.stdout(Stdio::piped());
                    cmd.stderr(Stdio::piped());
                    let cmd = CommandMessages::with_command(cmd)?;

                    tracing::trace!("receiving messages");
                    for message in cmd {
                        match message?.decode()? {
                            escargot::format::Message::BuildFinished(build) => {
                                self.send(IpcMessage::InnerProgressUpdate(
                                    InnerProgressUpdate::BuildFinished(build),
                                ))?;
                                break;
                            }
                            escargot::format::Message::CompilerArtifact(_) => {
                                self.send(IpcMessage::InnerProgressUpdate(
                                    InnerProgressUpdate::Progress,
                                ))?;
                            }
                            // TODO: propagate compiler messages
                            escargot::format::Message::CompilerMessage(_) => {}
                            escargot::format::Message::BuildScriptExecuted(_) => {}
                            _ => todo!(),
                        }
                    }
                    let (_, run_cmd) = build_cmd();
                    run_cmd.run()?.command().spawn()?;
                    self.running = Some(run);
                }
                IpcMessage::Reload => todo!(),
                IpcMessage::Kill => return Ok(()),
            }
        }
    }
}

pub(crate) struct OuterIpc {
    listener: TcpListener,
}

impl OuterIpc {
    pub(crate) fn new() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        Ok(Self { listener })
    }

    pub(crate) fn port(&self) -> Result<u16> {
        Ok(self.listener.local_addr()?.port())
    }

    pub(crate) fn spawn(
        &mut self,
        dev_server_port: u16,
        pair: &mut PtyPair,
    ) -> Result<Box<dyn Child + Send + Sync>> {
        let (shell, child_args) = if cfg!(target_os = "windows") {
            todo!();
        // (, ["/C"].as_slice())
        } else {
            (
                std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
                ["-l", "-c"].as_slice(),
            )
        };
        let mut cmd = CommandBuilder::new(shell);
        cmd.args(child_args);
        let cwd = std::env::current_dir()?;

        cmd.cwd(cwd);
        if std::env::var("CARGO").is_ok() {
            cmd.args(["cargo run -p mx-cli -- ipc"]);
        } else {
            cmd.args(["mx ipc"]);
        }
        cmd.env("MX_DEV_SERVER_PORT", dev_server_port.to_string());
        cmd.env("MX_IPC_PORT", self.port()?.to_string());
        tracing::trace!("running ipc: {}", cmd.as_unix_command_line()?);
        let child = pair.slave.spawn_command(cmd)?;
        Ok(child)
    }

    pub(crate) fn run(self, bridge: &AppBridge) -> Result<()> {
        enum EventLoopResult {
            None,
            Quit,
        }
        loop {
            let (mut stream_1, _) = self.listener.accept()?;
            tracing::trace!("accepted connection");
            let mut stream_2 = stream_1.try_clone()?;
            let value = std::thread::scope(|scope| {
                scope.spawn(|| -> Result<()> {
                    let mut deser = dlhn::Deserializer::new(&mut stream_1);
                    loop {
                        if !bridge.running.load(Ordering::Relaxed) {
                            break Ok(());
                        }
                        let msg = IpcMessage::deserialize(&mut deser)?;
                        tracing::trace!("{msg:?}");
                        bridge.ipc_chan.0.send(IpcEvent::Message(msg))?;
                    }
                });
                let event_loop = scope.spawn(|| -> Result<EventLoopResult> {
                    loop {
                        match bridge.ipc_chan.1.recv()? {
                            IpcEvent::Message(msg) => {
                                if let IpcMessage::InnerProgressUpdate(update) = msg {
                                    match update {
                                        InnerProgressUpdate::Progress => bridge
                                            .render_chan
                                            .0
                                            .send(RenderMsg::IpcBuildProgress)?,
                                        InnerProgressUpdate::BuildStarted(count, name) => bridge
                                            .render_chan
                                            .0
                                            .send(RenderMsg::IpcBuildStarted(count, name))?,
                                        InnerProgressUpdate::BuildFinished(_) => {
                                            bridge
                                                .render_chan
                                                .0
                                                .send(RenderMsg::IpcBuildFinished)?;
                                        }
                                    }
                                }
                            }
                            IpcEvent::Quit => {
                                stream_2.shutdown(std::net::Shutdown::Both)?;
                                tracing::trace!("ipc: closed tcp socket");
                                break Ok(EventLoopResult::Quit);
                            }
                            IpcEvent::Request(ipc_message) => {
                                let mut ser = dlhn::Serializer::new(&mut stream_2);
                                ipc_message.serialize(&mut ser)?;
                            }
                        }
                    }
                });
                match event_loop.join() {
                    Ok(inner) => inner,              // inner is already Result<(), anyhow::Error>
                    Err(e) => Err(anyhow!("{e:?}")), // wrap outer error
                }
            });
            let value = value?;
            match value {
                EventLoopResult::None => {}
                EventLoopResult::Quit => break Ok(()),
            }
        }
    }
}

pub(crate) enum IpcEvent {
    Message(IpcMessage),
    Request(IpcMessage),
    Quit,
}
