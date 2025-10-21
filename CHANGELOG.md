### v0.4.0

- Update Rust Edition to 2024
- Update tokio-tungstenite to 0.28.0

- ### v0.3.2

- Update tokio-tungstenite to 0.27.0

### v0.3.1

- Update Tokio to 1.44.2 for security vulnerability fix

### v0.3.0

- Update `JsonExact` and `JsonPartial` to not panic on non-parseable JSON
- BREAKING - Update to tokio-tungstenite 0.26.2
    - The underlying types of `tungstenite::protocol::Message` have changed, but should be easy to migrate using
      From/Into

### v0.2.1

- Update tokio-tungstenite to v0.24.0
- Other minor version bumps

### v0.2.0

- Update tokio-tungstenite to v0.23.1
- Update tokio to 1.39.2
- Update serde-with to 3.9.0