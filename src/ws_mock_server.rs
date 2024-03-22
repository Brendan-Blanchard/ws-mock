use crate::matchers::Matcher;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::select;
use tokio::sync::broadcast::Receiver as BroadcastReceiver;
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::sync::mpsc::Sender as MpscSender;
use tokio::sync::mpsc::{Receiver as MpscReceiver, Sender};
use tokio::sync::{broadcast, mpsc, Notify, RwLock};
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async, WebSocketStream};

use tracing::debug;

const INCOMPLETE_MOCK_PANIC: &str = "A mock must have a response or expected number of calls, or forwarding_channel set. Add `.expect(...)`, `.forward_from_channel()`, or `.respond_with(...)` before mounting the mock.";

/// An individual mock that matches on one or more matchers, and expects a particular number of
/// calls and/or responds with configured data.
///
/// Each [`WsMock`] can have many [`Matcher`]s added to it before mounting it to a [`WsMockServer`],
/// and will only respond if all the added matchers match successfully. A mock must either have an
/// expected number of calls, or respond with data before being mounted.
///
/// Mocks *must* be mounted to a [WsMockServer] to have any effect! Failing to call
/// `server.verify().await` will also erroneously pass tests, as this is how the server is told to
/// verify that all mocks were called as expected. If you rely only on the response from the server
/// as part of a test and have no `.expect(...)` call, the call to `.verify()` can be omitted.
///
/// # Example
/// The below [WsMock] will match on any incoming data and respond with "Hello World". In this case,
/// it expects no messages, since we don't send it any.
///
/// ```
/// use ws_mock::matchers::Any;
/// use ws_mock::ws_mock_server::{WsMock, WsMockServer};
///
/// #[tokio::main]
/// async fn main() -> () {
///     let server = WsMockServer::start().await;
///
///     WsMock::new()
///         .matcher(Any::new())
///         .respond_with("Hello World".to_string())
///         .expect(0)
///         .mount(&server)
///         .await;
///
///     server.verify().await;
/// }
/// ```
#[derive(Debug)]
pub struct WsMock {
    matchers: Vec<Box<dyn Matcher>>,
    response_data: Option<String>,
    forwarding_channel: Option<MpscReceiver<Message>>,
    expected_calls: Option<usize>,
    calls: usize,
}

impl Default for WsMock {
    fn default() -> Self {
        Self::new()
    }
}

impl WsMock {
    pub fn new() -> WsMock {
        WsMock {
            matchers: Vec::new(),
            response_data: None,
            forwarding_channel: None,
            expected_calls: None,
            calls: 0,
        }
    }

    /// Add a [Matcher] to this [WsMock] instance.
    ///
    /// All attached matchers must match for this mock to respond with any data or record a call.
    pub fn matcher<T: Matcher + 'static>(mut self, matcher: T) -> Self {
        self.matchers.push(Box::new(matcher));
        self
    }

    /// Respond with a message, if/when all attached matchers match on a message.
    pub fn respond_with(mut self, data: String) -> Self {
        self.response_data = Some(data);
        self
    }

    /// Forward any messages from the provided mpsc `Receiver`.
    ///
    /// This provides the ability for "live" streams common in websockets, since you can provide any
    /// messages directly to control the stream of messages.
    ///
    /// Forwarding can be used in conjunction with `.respond_with(...)` and/or `.expect(...)`, but
    /// neither is required to use forwarding. Calling `.matcher(...)` has no effect on what values
    /// will be forwarded from the provided channel.
    ///
    /// # Example: Passing Messages Through
    /// Tests may mock the data sent by the server, and need to assert they're parsed correctly by a
    /// client. Since this doesn't require sending data to the server and receiving a response,
    /// another mechanism aside from `.respond_with(...)` is needed. `forward_from_channel` fills
    /// this need.
    ///
    /// Here, a channel is added to a [WsMock] that has no expectations or response data, and used
    /// to send two messages through to the client. Any subsequent client behavior canbe tested
    /// based on the received data.
    ///
    /// ```rust
    /// use ws_mock::matchers::Any;
    /// use ws_mock::utils::collect_all_messages;
    /// use ws_mock::ws_mock_server::{WsMock, WsMockServer};
    /// use futures_util::StreamExt;
    /// use tokio::sync::mpsc;
    ///
    /// #[tokio::main]
    /// pub async fn main() {
    ///     use std::time::Duration;
    ///     use futures_util::SinkExt;
    ///     use tokio_tungstenite::connect_async;
    ///     use tokio_tungstenite::tungstenite::Message;
    ///     let server = WsMockServer::start().await;
    ///
    ///     let (mpsc_send, mpsc_recv) = mpsc::channel::<Message>(32);
    ///
    ///     WsMock::new()
    ///         .forward_from_channel(mpsc_recv)
    ///         .mount(&server)
    ///         .await;
    ///
    ///     let (stream, _resp) = connect_async(server.uri().await)
    ///         .await
    ///         .expect("Connecting failed");
    ///
    ///     let (_send, ws_recv) = stream.split();
    ///
    ///     mpsc_send.send(Message::Text("message-1".to_string())).await.unwrap();
    ///     mpsc_send.send(Message::Text("message-2".into())).await.unwrap();
    ///
    ///     let received = collect_all_messages(ws_recv, Duration::from_millis(250)).await;
    ///
    ///     server.verify().await;
    ///     assert_eq!(vec!["message-1", "message-2"], received);
    /// }
    /// ```
    ///
    /// [`tokio::sync:mpsc`]: https://docs.rs/tokio/1.36.0/tokio/sync/mpsc/index.html
    /// [`Receiver`]: https://docs.rs/tokio/1.36.0/tokio/sync/mpsc/struct.Receiver.html
    pub fn forward_from_channel(mut self, receiver: MpscReceiver<Message>) -> Self {
        self.forwarding_channel = Some(receiver);
        self
    }

    /// Expect for this mock to be matched against `n` times.
    ///
    /// Calling `server.verify().await` will panic if this mock did not match accordingly.
    pub fn expect(mut self, n: usize) -> Self {
        self.expected_calls = Some(n);
        self
    }

    /// Mount this mock to an instance of [WsMockServer]
    ///
    /// Mounting a mock without having called `.respond_with(...)`, `.forward_from_channel(...)`, or
    /// `.expect(...)` will panic, since the mock by definition can have no effects.
    pub async fn mount(self, server: &WsMockServer) {
        if self.response_data.is_none()
            && self.expected_calls.is_none()
            && self.forwarding_channel.is_none()
        {
            panic!("{}", INCOMPLETE_MOCK_PANIC);
        }

        let mut state = server.state.write().await;
        state.mount(self);
    }

    /// Check if all attached [Matcher]s match on the given text, used to determine if the server
    /// should log a call and respond with data (if set).
    #[doc(hidden)]
    fn matches_all(&self, text: &str) -> bool {
        self.matchers.iter().all(|m| m.matches(text))
    }
}

/// The internal state of the mock, generally passed around as `Arc<RwLock<MockHandle>>`.
///
/// `ready_notify` allows for a server in a different task/thread to communicate its readiness.
#[doc(hidden)]
struct ServerState {
    connection_string: String,
    ready_notify: Arc<Notify>,
    mocks: Vec<WsMock>,
    calls: Vec<String>,
    close_sender: BroadcastSender<()>,
}

impl ServerState {
    pub fn new(url: String, port: u16, notify: Arc<Notify>) -> ServerState {
        let (close_sender, _) = broadcast::channel::<()>(1);

        ServerState {
            connection_string: format!("{}:{}", url, port),
            ready_notify: notify,
            mocks: Vec::new(),
            calls: Vec::new(),
            close_sender,
        }
    }

    /// Mount a [WsMock] to the server's internal state.
    #[doc(hidden)]
    fn mount(&mut self, mock: WsMock) {
        self.mocks.push(mock);
    }
}

/// A mock server that exposes a uri and accepts connections.
///
/// Once mocks are mounted to a [WsMockServer], if matched against, they will log calls and respond
/// according to their configuration.
///
/// # Example: Creating and Matching
/// Here we start a [WsMockServer], create a [WsMock] that will match on any incoming messages and
/// respond with "Hello World", and mount it to the server. Once mounted, any messages sent to the
/// server will trigger a response and be recorded.
///
/// ```rust
/// use futures_util::{SinkExt, StreamExt};
/// use std::time::Duration;
/// use tokio::time::timeout;
/// use tokio_tungstenite::connect_async;
/// use tokio_tungstenite::tungstenite::Message;
/// use ws_mock::matchers::Any;
/// use ws_mock::ws_mock_server::{WsMock, WsMockServer};
///
/// #[tokio::main]
/// pub async fn main() {
///     let server = WsMockServer::start().await;
///
///     WsMock::new()
///         .matcher(Any::new())
///         .respond_with("Hello World".to_string())
///         .expect(1)
///         .mount(&server)
///         .await;
///
///     let (stream, _resp) = connect_async(server.uri().await)
///         .await
///         .expect("Connecting failed");
///
///     let (mut send, mut recv) = stream.split();
///
///     send.send(Message::from("some message")).await.unwrap();
///
///     let mut received = Vec::new();
///     
///     // this times out and continues after receiving one response from the server
///     while let Ok(Some(Ok(message))) = timeout(Duration::from_millis(100), recv.next()).await {
///         received.push(message.to_string());
///     }
///
///     server.verify().await;
///     assert_eq!(vec!["Hello World"], received);
/// }
/// ```
pub struct WsMockServer {
    state: Arc<RwLock<ServerState>>,
}

impl WsMockServer {
    /// Start the server on a random port assigned by the operating system.
    ///
    /// This creates a new internal state object, starts the server as a task, and waits for the
    /// handler to signal readiness before returning the server to the caller.
    pub async fn start() -> WsMockServer {
        let ready_notify = Arc::new(Notify::new());
        let state = Arc::new(RwLock::new(ServerState::new(
            "127.0.0.1".to_string(),
            0,
            ready_notify.clone(),
        )));

        let server = WsMockServer::new(state.clone());

        tokio::spawn(async move { Self::listen(state).await });

        ready_notify.notified().await;

        server
    }

    /// Create a new instance using the given state.
    #[doc(hidden)]
    fn new(state: Arc<RwLock<ServerState>>) -> WsMockServer {
        WsMockServer { state }
    }

    /// Returns the ip address and port of the server as `format!("{}:{}", ip, port)`
    pub async fn get_connection_string(&self) -> String {
        let state = self.state.read().await;
        state.connection_string.clone()
    }

    /// Returns the uri necessary for a client to connect to this mock server instance.
    pub async fn uri(&self) -> String {
        format!("ws://{}", self.get_connection_string().await)
    }

    /// Using the provided state, listen and accept connections.
    ///
    /// This is static to avoid any ownership issues, with the expectation that the caller has
    /// cloned `state` if they have other uses for it.
    #[doc(hidden)]
    async fn listen(state: Arc<RwLock<ServerState>>) {
        let listener = Self::get_listener(state.clone()).await;

        if let Ok((stream, _peer)) = listener.accept().await {
            let state = state.clone();
            tokio::spawn(WsMockServer::handle_connection(stream, state));
        }
    }

    /// Creates the TcpListener needed to accept connections. Once connected, it signals readiness
    /// via the `ready_notify` instance on the provided state before returning.
    #[doc(hidden)]
    async fn get_listener(state: Arc<RwLock<ServerState>>) -> TcpListener {
        let mut state = state.write().await;
        let listener = TcpListener::bind(state.connection_string.as_str())
            .await
            .expect("Failed to listen to port");

        let listener_addr = listener
            .local_addr()
            .expect("Listener had no local address");

        // may connect using 0 to get automatic port from OS
        //  re-assign the real port that was bound
        state.connection_string = format!("{}:{}", listener_addr.ip(), listener_addr.port());

        state.ready_notify.notify_one();

        listener
    }

    /// Handles a single connection using the provided `TcpStream` and `MockHandle`.
    ///
    /// This is responsible for launching several listening tasks, and monitoring incoming messages
    /// to check if mocks match. Upon matching, it optionally sends data to be put on the outgoing
    /// websocket messages, and also updates any call counts.
    #[doc(hidden)]
    async fn handle_connection(stream: TcpStream, state: Arc<RwLock<ServerState>>) {
        let ws_stream = accept_async(stream)
            .await
            .expect("Failed to accept connection");

        let (send, mut recv) = ws_stream.split();

        let (mpsc_send, mpsc_recv) = mpsc::channel::<Message>(32);

        Self::spawn_forwarding_tasks(state.clone(), mpsc_send.clone()).await;

        {
            let state_guard = state.read().await;
            let broad_recv = state_guard.close_sender.subscribe();

            tokio::spawn(Self::outbound_message_task(send, mpsc_recv, broad_recv));
        }

        while let Some(Ok(msg)) = recv.next().await {
            let text = msg.to_text().expect("Message was not text").to_string();
            debug!("Received: '{:?}'", text);

            Self::match_mocks(state.clone(), mpsc_send.clone(), text.as_str()).await;
        }
    }

    /// Spawn a task for any [WsMock] in `state` that has a forwarding_channel set.
    #[doc(hidden)]
    async fn spawn_forwarding_tasks(state: Arc<RwLock<ServerState>>, sender: MpscSender<Message>) {
        let mut state_guard = state.write().await;

        for mock in &mut state_guard.mocks {
            if let Some(forwarding_channel) = mock.forwarding_channel.take() {
                tokio::spawn(Self::forward_messages_task(
                    forwarding_channel,
                    sender.clone(),
                ));
            }
        }
    }

    /// Forwards all incoming messages to this mock's forwarding channel to the provided outgoing
    /// sender.
    #[doc(hidden)]
    async fn forward_messages_task(
        mut incoming: MpscReceiver<Message>,
        outgoing: MpscSender<Message>,
    ) {
        while let Some(msg) = incoming.recv().await {
            outgoing.send(msg).await.unwrap();
        }
    }

    /// This task has sole access to the websocket sender, and is responsible for receiving messages
    /// from an incoming `tokio::sync::mpsc::Receiver` and passing any incoming messages on to the
    /// websocket. It will exit upon receiving input on the `tokio::sync::broadcast::Receiver`.
    #[doc(hidden)]
    async fn outbound_message_task(
        mut sender: SplitSink<WebSocketStream<TcpStream>, Message>,
        mut receiver: MpscReceiver<Message>,
        mut close: BroadcastReceiver<()>,
    ) {
        loop {
            select! {
                Some(msg) = receiver.recv() => sender.send(msg).await.unwrap(),
                Ok(_) = close.recv() => break,
                else => break
            }
        }
    }

    /// Check if a given message matches any mocks, update call counts, and send any configured responses.
    #[doc(hidden)]
    async fn match_mocks(state: Arc<RwLock<ServerState>>, mpsc_send: Sender<Message>, text: &str) {
        let mut state_guard = state.write().await;

        state_guard.calls.push(text.to_string());

        for mock in &mut state_guard.mocks {
            if mock.matches_all(text) {
                mock.calls += 1;
                if let Some(data) = &mock.response_data {
                    mpsc_send.send(Message::text(data)).await.unwrap();
                }
            }
        }
    }

    /// Verify the status of all mocks, and panic if expectations have not been met.
    ///
    /// This must be called in order for mock expectations to be verified. Failure to do so, if not
    /// also relying on messages sent by the server to verify behavior, will result in faulty tests.
    pub async fn verify(&self) {
        sleep(Duration::from_millis(100)).await;
        let state_guard = self.state.read().await;

        let mut results = Vec::new();

        for mock in &state_guard.mocks {
            if let Some(expected) = mock.expected_calls {
                if expected != mock.calls {
                    results.push(format!(
                        "Expected {} matching calls, but received {}\nCalled With:",
                        expected, mock.calls
                    ));
                }
            }
        }

        if !results.is_empty() {
            for mock_call in &state_guard.calls {
                results.push(format!("\t{}", mock_call));
            }
            panic!("{}", results.join("\n"));
        }
    }

    /// Shutdown the server and all associated tasks.
    pub async fn shutdown(&mut self) {
        let state_guard = self.state.read().await;
        // ignore outcome, since failure means receivers dropped anyway
        _ = state_guard.close_sender.send(());
    }
}

#[cfg(test)]
mod tests {
    use crate::matchers::Any;
    use crate::utils::{collect_all_messages, send_to_server};
    use crate::ws_mock_server::{WsMock, WsMockServer};
    use futures_util::StreamExt;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    #[tokio::test]
    async fn test_wss_mockserver() {
        let server = WsMockServer::start().await;

        WsMock::new()
            .matcher(Any::new())
            // no response given is okay
            .expect(1)
            .mount(&server)
            .await;

        // ::default() is same as ::new()
        WsMock::default()
            .matcher(Any::new())
            .respond_with("Mock-2".to_string())
            .expect(1)
            .mount(&server)
            .await;

        let recv = send_to_server(&server, "{ data: [42] }".into()).await;

        let received = collect_all_messages(recv, Duration::from_millis(250)).await;

        server.verify().await;
        assert_eq!(vec!["Mock-2"], received);
    }

    #[tokio::test]
    async fn test_forwarding_channel() {
        let server = WsMockServer::start().await;

        let (mpsc_send, mpsc_recv) = mpsc::channel::<Message>(32);

        WsMock::new()
            .matcher(Any::new())
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
        assert_eq!(vec!["message-1", "message-2"], received);
    }

    #[tokio::test]
    async fn test_shutdown_with_active_channel() {
        let mut server = WsMockServer::start().await;

        let (_, mpsc_recv) = mpsc::channel::<Message>(32);

        WsMock::new()
            .matcher(Any::new())
            .forward_from_channel(mpsc_recv)
            .mount(&server)
            .await;

        server.verify().await;
        server.shutdown().await;
    }

    #[should_panic(expected = "Expected 2 matching calls, but received 1\nCalled With:\n\t{}")]
    #[tokio::test]
    async fn test_ws_mockserver_verify_failure() {
        let server = WsMockServer::start().await;

        WsMock::new()
            .matcher(Any::new())
            .respond_with("Mock-1".to_string())
            .expect(2)
            .mount(&server)
            .await;

        let _recv = send_to_server(&server, "{}".into()).await;
        server.verify().await;
    }

    #[should_panic(
        expected = "A mock must have a response or expected number of calls, or forwarding_channel set. Add `.expect(...)`, `.forward_from_channel()`, or `.respond_with(...)` before mounting the mock."
    )]
    #[tokio::test]
    async fn test_incomplete_mock_failure() {
        let server = WsMockServer::start().await;

        WsMock::new().matcher(Any::new()).mount(&server).await;
    }
}
