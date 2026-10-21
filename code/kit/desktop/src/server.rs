use std::future::Future;
use std::net::Ipv4Addr;
use std::pin::Pin;

use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::Result;

/// Resolves when the app quits: the server should stop taking requests,
/// finish the ones it has and stop its workers.
pub type Shutdown = Pin<Box<dyn Future<Output = ()> + Send>>;

/// The app's own server, on loopback, for the workspaces on this computer.
pub struct LocalServer {
    origin: String,
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

impl LocalServer {
    /// Binds `127.0.0.1:preferred_port`, or any free port if that one is
    /// taken, and runs `serve` on it until [`stop`](Self::stop).
    pub async fn spawn<F, Fut, E>(preferred_port: u16, serve: F) -> Result<Self>
    where
        F: FnOnce(TcpListener, Shutdown) -> Fut,
        Fut: Future<Output = std::result::Result<(), E>> + Send + 'static,
        E: std::fmt::Display,
    {
        let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, preferred_port)).await {
            Ok(listener) => listener,
            Err(_) => TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?,
        };
        let origin = format!("http://{}", listener.local_addr()?);
        let (stop, stopped) = oneshot::channel();
        let shutdown: Shutdown = Box::pin(async move {
            let _ = stopped.await;
        });
        let running = serve(listener, shutdown);
        let task = tokio::spawn(async move {
            if let Err(error) = running.await {
                tracing::error!("the local server stopped: {error}");
            }
        });
        Ok(Self { origin, stop, task })
    }

    /// Where the server is, `http://127.0.0.1:<port>`.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Asks the server to stop and waits until it has.
    pub async fn stop(self) {
        let _ = self.stop.send(());
        let _ = self.task.await;
    }
}
