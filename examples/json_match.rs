use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use ws_mock::matchers::JsonExact;
use ws_mock::ws_mock_server::{WsMock, WsMockServer};

#[tokio::main]
pub async fn main() {
    let expected_json = json!({"message": "heartbeat"});
    let json_msg = serde_json::to_string(&expected_json).expect("Failed to serialize message");

    let server = WsMockServer::start().await;

    WsMock::new()
        .matcher(JsonExact::new(expected_json))
        .respond_with(Message::Text("heartbeat".to_string()))
        .expect(1)
        .mount(&server)
        .await;

    let (stream, _resp) = connect_async(server.uri().await)
        .await
        .expect("Connecting failed");

    let (mut send, mut recv) = stream.split();

    send.send(Message::from(json_msg)).await.unwrap();

    let mut received = Vec::new();

    while let Ok(Some(Ok(message))) = timeout(Duration::from_millis(100), recv.next()).await {
        received.push(message.to_string());
    }

    server.verify().await;
    assert_eq!(vec!["heartbeat"], received);

    server.verify().await;
}
