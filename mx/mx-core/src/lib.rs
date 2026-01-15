pub mod args;
pub mod logging;

pub use logging::layer;

use crate::logging::Trace;

#[derive(Debug, Clone)]
pub enum RenderMsg {
    Log(Trace),
    Draw,
    Quit,
}

use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::logging::DevClientLayer;

pub fn init() {
    tracing_subscriber::registry()
        .with(DevClientLayer::new())
        .with(
            EnvFilter::builder()
                .with_default_directive("info".parse().unwrap())
                .from_env_lossy(),
        )
        .try_init()
        .unwrap();
}
