//! The app's server on loopback: it binds, serves, and stops when asked.

use code_villain_desktop::LocalServer;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn serves_on_loopback_until_stopped() {
    let server = LocalServer::spawn(0, |listener, shutdown| async move {
        tokio::select! {
            _ = shutdown => Ok::<_, std::io::Error>(()),
            accepted = listener.accept() => {
                let (mut socket, _) = accepted?;
                socket.write_all(b"hello").await
            }
        }
    })
    .await
    .unwrap();
    assert!(server.origin().starts_with("http://127.0.0.1:"));

    let address = server.origin().trim_start_matches("http://").to_string();
    let mut socket = tokio::net::TcpStream::connect(&address).await.unwrap();
    let mut reply = String::new();
    socket.read_to_string(&mut reply).await.unwrap();
    assert_eq!(reply, "hello");
    server.stop().await;
}

#[tokio::test]
async fn takes_another_port_when_the_preferred_one_is_busy() {
    let busy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = busy.local_addr().unwrap().port();
    let server = LocalServer::spawn(port, |_listener, shutdown| async move {
        shutdown.await;
        Ok::<_, std::io::Error>(())
    })
    .await
    .unwrap();
    assert_ne!(server.origin(), format!("http://127.0.0.1:{port}"));
    server.stop().await;
}
