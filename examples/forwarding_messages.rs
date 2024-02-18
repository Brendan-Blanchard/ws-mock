use futures_util::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use ws_mock::utils::collect_all_messages;
use ws_mock::ws_mock_server::{WsMock, WsMockServer};

#[tokio::main]
pub async fn main() {
    let server = WsMockServer::start().await;

    let (mpsc_send, mpsc_recv) = mpsc::channel::<String>(32);

    WsMock::new()
        .forward_from_channel(mpsc_recv)
        .mount(&server)
        .await;

    let (stream, _resp) = connect_async(server.uri().await)
        .await
        .expect("Connecting failed");

    let (_send, ws_recv) = stream.split();

    mpsc_send.send("message-1".to_string()).await.unwrap();
    mpsc_send.send("message-2".into()).await.unwrap();

    let received = collect_all_messages(ws_recv, Duration::from_millis(250)).await;

    server.verify().await;
    assert_eq!(vec!["message-1", "message-2"], received);
}
