### v0.2.2

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