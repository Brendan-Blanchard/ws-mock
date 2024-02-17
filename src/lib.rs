//! A simple websocket mock framework heavily inspired by [`Wiremock`] in Rust.
//!
//! Ws-Mock is meant to provide a simple framework for expecting, verifying, and responding to
//! messages for tests.
//!
//! [`Wiremock`]: https://docs.rs/wiremock/latest/wiremock/

/// A common trait and useful implementations for matching against received messages.
pub mod matchers;

/// The mock server implementation that handles `WsMock`s, expecting, verifying, and responding to
/// messages.
pub mod ws_mock_server;
