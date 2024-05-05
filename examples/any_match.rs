use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use ws_mock::matchers::Any;
use ws_mock::ws_mock_server::{WsMock, WsMockServer};

#[tokio::main]
pub async fn main() {
    let server = WsMockServer::start().await;

    WsMock::new()
        .matcher(Any::new())
        .respond_with(Message::Text("Hello World".to_string()))
        .expect(1)
        .mount(&server)
        .await;

    let (stream, _resp) = connect_async(server.uri().await)
        .await
        .expect("Connecting failed");

    let (mut send, mut recv) = stream.split();

    send.send(Message::from("some message")).await.unwrap();

    let mut received = Vec::new();

    while let Ok(Some(Ok(message))) = timeout(Duration::from_millis(100), recv.next()).await {
        received.push(message);
    }

    server.verify().await;
    assert_eq!(vec![Message::Text("Hello World".to_string())], received);
}
