//! Utilities used throughout testing and implementations that may be of use.
use crate::ws_mock_server::WsMockServer;
use futures_util::stream::SplitStream;
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

/// Send a single message to the server, and return the `SplitStream` receiver created by
/// connecting to the server.
///
/// Useful for one-off messages and optionally reading responses afterwards.
pub async fn send_to_server(
    server: &WsMockServer,
    message: String,
) -> SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>> {
    let (stream, _resp) = connect_async(server.uri().await)
        .await
        .expect("Connecting failed");

    let (mut send, recv) = stream.split();

    send.send(Message::from(message)).await.unwrap();

    recv
}

/// Given a `SplitStream` receiver, receive messages until timing out and return all messages in a `Vec<String>`.
///
/// Useful for receiving a batch of messages and later making assertions about them.
pub async fn collect_all_messages(
    mut ws_recv: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    timeout: Duration,
) -> Vec<Message> {
    let mut received = Vec::new();

    while let Ok(Some(Ok(message))) = tokio::time::timeout(timeout, ws_recv.next()).await {
        received.push(message);
    }
    received
}
