# ws-mock
A mock server for websocket testing with the ability to match arbitrarily on received messages and respond accordingly.

![badge](https://github.com/Brendan-Blanchard/ws-mock/actions/workflows/main.yml/badge.svg)[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

WS-Mock allows you to set up a mockserver to test websocket code against, with dynamic responses based on the received
data. 

# Example: Responding to a Heartbeat Message
Using the `JsonExact` matcher, `WsMock` will match on messages that contain exactly the same json as it was given,
and respond with the string "heartbeat". 

Many mocks can be mounted on a single `WsMockServer`, and calling `server.verify().await` will check that every mock's
expectations were met by incoming messages. Any failures will cause a panic, detailing what messages were seen and 
expected.

Either `.respond_with(...)` or `expect(...)` is required for a mock, since mounting a mock that does not respond or 
expect any calls will have no discernible effects. This produces a panic if a `WsMock` is mounted without a response or 
expected number of calls. It's also perfectly valid to `.expect(0)` calls to a mock if verifying a certain type of data 
was never received. 

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
        .respond_with("heartbeat".to_string())
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

# Contributions
Please reach out or submit a PR! Particularly if you have new general purpose `Matcher` implementations that would 
benefit users. 