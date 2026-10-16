//! The Trellis HTTP server (spec §5.1). All agents of a repository must share one server:
//! session, task, and lease locks are in-process.

mod error;
mod routes;
mod state;

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub use error::ApiError;
pub use state::{AppState, Config};

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub fn router(state: Arc<AppState>) -> axum::Router {
    routes::router(state)
}

/// Serves until Ctrl-C.
pub fn run(config: Config) -> Result<(), BoxError> {
    let state = Arc::new(AppState::open(&config)?);
    state.spawn_validators(Arc::new(AtomicBool::new(false)));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(config.addr).await?;
        eprintln!("trellis listening on http://{}", listener.local_addr()?);
        axum::serve(listener, router(state))
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?;
        Ok(())
    })
}

/// A server on a background thread (tests, embedding). Dropping it signals shutdown.
pub struct Background {
    pub addr: SocketAddr,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    stop: Arc<AtomicBool>,
}

impl Background {
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

pub fn start_background(config: Config) -> Result<Background, BoxError> {
    let state = Arc::new(AppState::open(&config)?);
    let stop = Arc::new(AtomicBool::new(false));
    state.spawn_validators(Arc::clone(&stop));
    let (addr_tx, addr_rx) = std::sync::mpsc::channel::<Result<SocketAddr, String>>();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let addr = config.addr;
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(e) => {
                let _ = addr_tx.send(Err(e.to_string()));
                return;
            }
        };
        runtime.block_on(async move {
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => listener,
                Err(e) => {
                    let _ = addr_tx.send(Err(e.to_string()));
                    return;
                }
            };
            let _ = addr_tx.send(listener.local_addr().map_err(|e| e.to_string()));
            let _ = axum::serve(listener, router(state))
                .with_graceful_shutdown(async {
                    let _ = stop_rx.await;
                })
                .await;
        });
    });
    let addr = addr_rx.recv().map_err(|e| e.to_string())??;
    Ok(Background {
        addr,
        shutdown: Some(stop_tx),
        stop,
    })
}
