# ws-mock

A mock server for websocket testing with the ability to match arbitrarily on received messages and respond accordingly.

![badge](https://github.com/Brendan-Blanchard/ws-mock/actions/workflows/main.yml/badge.svg) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT) [![codecov](https://codecov.io/gh/Brendan-Blanchard/ws-mock/graph/badge.svg?token=WKZGL0DVH4)](https://codecov.io/gh/Brendan-Blanchard/ws-mock)

WS-Mock allows you to set up a mockserver to test websocket code against, with dynamic responses based on the received
data.

# Example: Responding to a Heartbeat Message

Using the `JsonExact` matcher, `WsMock` will match on messages that contain exactly the same json as it was given,
and respond with the string "heartbeat".

Many mocks can be mounted on a single `WsMockServer`, and calling `server.verify().await` will check that every mock's
expectations were met by incoming messages. Any failures will cause a panic, detailing what messages were seen and
expected.

Either `.respond_with(...)`, `.forward_from_channel(...)`, or `expect(...)` is required for a mock, since mounting a
mock that does not respond, forward messages, or expect any calls will have no discernible effects. This produces a
panic if a `WsMock` is mounted without a response or expected number of calls. It's also perfectly valid to `.expect(0)`
calls to a mock if verifying a certain type of data was never received.

```rust
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use ws_mock_test::matchers::JsonExact;
use ws_mock_test::ws_mock_server::{WSMock, WSMockServer};

#[tokio::main]
pub async fn main() {
    let expected_json = json!({"message": "heartbeat"});
    let json_msg = serde_json::to_string(&expected_json).expect("Failed to serialize message");

    let server = WSMockServer::start().await;

    WsMock::new()
        .matcher(JsonExact::new(expected_json))
        .respond_with(Message::Text("heartbeat".to_string()))
        .expect(1)
        .mount(&server)
        .await;

    let (stream, _resp) = connect_async(server.uri().await)
        .await
        .expect("Connecting failed");

    let (mut send, _recv) = stream.split();

    send.send(Message::from(json_msg)).await.unwrap();

    server.verify().await;
}
```

# Example: Simulating Live Streaming Data

Many websocket examples don't rely on responding to requests via Websocket, but instead stream data from the server with
no client input. Testing this requires the server accepting messages and sending them to the client.
`WsMock.forward_from_channel(...)` accomplishes this by letting the user send arbitrary messages that the server then
relays to the client.

In the below example, the test simulates streaming data by sending messages into the channel configured on the [WsMock].
The mock has no `.expect(...)`, or `.respond_with(...)` calls, since its only use is forwarding messages that simulate
a live server.

```rust
use futures_util::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
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

    mpsc_send.send(Message::Text("message-1".to_string())).await.unwrap();
    mpsc_send.send(Message::Text("message-2".into())).await.unwrap();

    let received = collect_all_messages(ws_recv, Duration::from_millis(250)).await;

    server.verify().await;
    assert_eq!(vec![
        Message::Text("message-1".to_string()),
        Message::Text("message-2".to_string())
    ], received);
}
```

# Contributions

Please reach out or submit a PR! Particularly if you have new general purpose `Matcher` implementations that would
benefit other users.