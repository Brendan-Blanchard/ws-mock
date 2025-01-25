/// A common [`Matcher`] trait and useful implementations for matching on JSON data.
use serde_json::{Map, Value};
use std::fmt::Debug;
use std::mem::discriminant;

/// An implementable trait accepted by [`WsMock`], allowing extension for arbitrary matching.
///
/// Users of this crate can implement any logic required for matching, so long as the implementation
/// is Send + Sync for Tokio async and thread safety.
///
/// [`WsMock`]: crate::ws_mock_server::WsMock
pub trait Matcher: Send + Sync + Debug {
    fn matches(&self, text: &str) -> bool;
}

/// Matches on every message it sees. This will rarely be used in combination
/// with other matchers, since it will respond to all messages.
#[derive(Debug)]
pub struct Any {}

impl Any {
    pub fn new() -> Any {
        Any {}
    }
}

impl Default for Any {
    fn default() -> Self {
        Self::new()
    }
}

impl Matcher for Any {
    fn matches(&self, _: &str) -> bool {
        true
    }
}

/// Matches on arbitrary logic provided by a closure.
///
/// For anything you can fit in an `fn(&str) -> bool` closure, this is a great option to avoid
/// having to create your own custom matcher for some on-the-fly matching that's needed.
///
/// Several provided matchers like [`Any`], [`StringExact`], and [`StringContains`] are easily
/// expressed as closures for [`AnyThat`], but are provided explicitly for clarity and convenience.
///
/// # Example: Matching on Any i64-Parseable Message
/// ```
/// use std::str::FromStr;
/// use ws_mock::matchers::{AnyThat, Matcher};
///
/// let matcher = AnyThat::new(|text| i64::from_str(text).is_ok());
/// let matching_message = "42";
/// let non_number_message = "...";
/// let non_matching_message = "42.000001";
///
/// assert!(matcher.matches(matching_message));
/// assert!(!matcher.matches(non_number_message));
/// assert!(!matcher.matches(non_matching_message));
/// ```
///
/// # Example: Function Pointers
/// Rust allows regular functions to be referred to via function pointers using the same [`fn`]
/// syntax, allowing for more complex logic than you may want to express in an in-line closure.
/// Merely for example, the same closure used above can be expressed as:
/// ```
/// use std::str::FromStr;
/// use ws_mock::matchers::{AnyThat, Matcher};
///
/// fn parses_to_i64(text: &str) -> bool {
///     i64::from_str(text).is_ok()
/// }
///
/// let matcher = AnyThat::new(parses_to_i64);
///
/// let matching_message = "42";
/// let non_matching_message = "42.000001";
///
/// assert!(matcher.matches(matching_message));
/// assert!(!matcher.matches(non_matching_message));
/// ```
/// [`fn`]: https://doc.rust-lang.org/std/primitive.fn.html
#[derive(Debug)]
pub struct AnyThat {
    f: fn(&str) -> bool,
}

impl AnyThat {
    pub fn new(f: fn(&str) -> bool) -> AnyThat {
        AnyThat { f }
    }
}

impl Matcher for AnyThat {
    fn matches(&self, text: &str) -> bool {
        (self.f)(text)
    }
}

/// Matches on any message containing a given string.
///
/// # Example: Matching Any Message Content
/// ```
/// use ws_mock::matchers::{Matcher, StringContains};
///
/// let matcher = StringContains::new("data");
/// let matching_message = "anything with data in it";
/// let non_matching_message = "anything but 'd-a-t-a'";
///
/// assert!(matcher.matches(matching_message));
/// assert!(!matcher.matches(non_matching_message));
/// ```
#[derive(Debug)]
pub struct StringContains<'a> {
    string: &'a str,
}

impl<'a> StringContains<'a> {
    pub fn new(string: &'a str) -> Self {
        Self { string }
    }
}

impl<'a> Matcher for StringContains<'a> {
    fn matches(&self, text: &str) -> bool {
        text.contains(self.string)
    }
}

/// Matches on exact string messages.
#[derive(Debug)]
pub struct StringExact<'a> {
    string: &'a str,
}

impl<'a> StringExact<'a> {
    pub fn new(string: &'a str) -> Self {
        Self { string }
    }
}

impl<'a> Matcher for StringExact<'a> {
    fn matches(&self, text: &str) -> bool {
        text == self.string
    }
}

/// Matches on exact JSON data. This will be most useful when the exact contents
/// of a message are important for matching, and any failure to match should cause an error.
///
/// # Example
/// ```
/// use serde_json::json;
/// use ws_mock::matchers::{JsonExact, Matcher};
///
/// let matching_data = r#"{ "data": 42 }"#;
/// let non_matching_data = r#"{ "data": 0 }"#;
///
/// let expected = json!({"data": 42});
///
/// let matcher = JsonExact::new(expected);
///
/// assert!(matcher.matches(matching_data));
/// assert!(!matcher.matches(non_matching_data));
/// ```
#[derive(Debug)]
pub struct JsonExact {
    json: Value,
}

impl JsonExact {
    pub fn new(json: Value) -> Self {
        JsonExact { json }
    }
}

impl Matcher for JsonExact {
    fn matches(&self, text: &str) -> bool {
        if let Ok(json) = serde_json::from_str::<Value>(text) {
            json == self.json
        } else {
            false
        }
    }
}

/// Matches on JSON patterns, useful for matching on all messages that have a
/// certain field, or matching data of only some type.
///
/// [`JsonPartial`] takes a `serde_json` [`Value`], and will match on anything that exhibits the same
/// structure and matches all primitives and arrays given by the pattern. Objects are matched recursively,
/// meaning that any keys present in the pattern must be present and compare equally to those in the
/// object being matched, but the object being matched can have other keys not present in the pattern.
///
/// # Example: Matching by Message Type
/// Matching on only data with a particular field (without caring about additional data) can be useful
/// for matching messages of a particular type, such as disregarding heartbeats or metadata to
/// focus on data messages only.
/// ```
/// use serde_json::json;
/// use ws_mock::matchers::{JsonExact, JsonPartial, Matcher};
///
/// let heartbeat = r#"{"type": "heartbeat"}"#;
/// let data = r#"{"type": "data", "data": [0, 1, 0]}"#;
/// let metadata = r#"{"type": "metadata", "data": "details"}"#;
///
/// let pattern = json!({"type": "data"});
/// let matcher = JsonPartial::new(pattern);
///
/// assert!(!matcher.matches(heartbeat));
/// assert!(matcher.matches(data));
/// assert!(!matcher.matches(metadata));
///
///
/// ```
/// ['Value']: https://docs.rs/serde_json/latest/serde_json/value/enum.Value.html
#[derive(Debug)]
pub struct JsonPartial {
    pattern: Value,
}

impl JsonPartial {
    pub fn new(pattern: Value) -> Self {
        JsonPartial { pattern }
    }

    fn match_json(data: &Value, pattern: &Value) -> bool {
        if discriminant(data) == discriminant(pattern) {
            match pattern {
                Value::Null => data.is_null(),
                Value::Bool(b) => *b == data.as_bool().unwrap(),
                Value::Number(n) => Some(n) == data.as_number(),
                Value::String(s) => Some(s.as_str()) == data.as_str(),
                Value::Array(a) => Some(a) == data.as_array(),
                Value::Object(o) => Self::match_object(data.as_object().unwrap(), o),
            }
        } else {
            false
        }
    }

    fn match_object(data: &Map<String, Value>, pattern: &Map<String, Value>) -> bool {
        pattern.keys().all(|k| {
            data.contains_key(k) && Self::match_json(data.get(k).unwrap(), pattern.get(k).unwrap())
        })
    }
}

impl Matcher for JsonPartial {
    fn matches(&self, text: &str) -> bool {
        if let Ok(json) = serde_json::from_str::<Value>(text) {
            Self::match_json(&json, &self.pattern)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::matchers::{
        Any, AnyThat, JsonExact, JsonPartial, Matcher, StringContains, StringExact,
    };
    use serde_json;
    use serde_json::{json, Value};
    use std::str::FromStr;

    #[test]
    fn any_matches_anything() {
        // ::default() is same as ::new()
        let matcher = Any::default();

        assert!(matcher.matches(""));
        assert!(matcher.matches("[42]"));
        assert!(matcher.matches("AnyText"));
    }

    #[test]
    fn string_contains() {
        let matcher = StringContains::new("heartbeat");
        let matching_message = "heartbeats keep websockets alive";
        let non_matching_message = "typos don't match: heatbeat";

        assert!(matcher.matches(matching_message));
        assert!(!matcher.matches(non_matching_message));
    }

    #[test]
    fn string_exact() {
        let matcher = StringExact::new("typos");
        let matching_message = "typos";
        let non_matching_message = "typo";
        let non_matching_message_2 = "typographical issue";

        assert!(matcher.matches(matching_message));
        assert!(!matcher.matches(non_matching_message));
        assert!(!matcher.matches(non_matching_message_2));
    }

    #[test]
    fn any_that() {
        let matcher = AnyThat::new(|text| text.contains(' '));
        let matching_message = "contains spaces";
        let non_matching_message = "doesNotContainSpaces";

        assert!(matcher.matches(matching_message));
        assert!(!matcher.matches(non_matching_message));
    }

    #[test]
    fn any_that_parses_to_i64() {
        let matcher = AnyThat::new(|text| i64::from_str(text).is_ok());
        let matching_message = "42";
        let invalid_message = "...";
        let non_matching_message = "42.000001";

        assert!(matcher.matches(matching_message));
        assert!(!matcher.matches(invalid_message));
        assert!(!matcher.matches(non_matching_message));
    }

    #[test]
    fn json_exact_matches_only_exact_json() {
        let expected_json = json!(["A", "B"]);
        let unexpected_json = json!(["A", "B", "Z"]);
        let serialized_expected = serde_json::to_string(&expected_json).unwrap();
        let serialized_unexpected = serde_json::to_string(&unexpected_json).unwrap();

        let matcher = JsonExact::new(expected_json);

        assert!(matcher.matches(serialized_expected.as_str()));
        assert!(!matcher.matches(serialized_unexpected.as_str()));
    }

    #[test]
    fn json_exact_does_not_match_on_invalid_data() {
        let expected_json = json!({});
        let matcher = JsonExact::new(expected_json);

        assert!(!matcher.matches("-"));
    }

    #[test]
    fn json_partial_does_not_match_on_invalid_data() {
        let expected_json = json!({});
        let matcher = JsonPartial::new(expected_json);

        assert!(!matcher.matches("-"));
    }

    #[test]
    fn json_partial_null() {
        let data = json!(null);
        let other = json!("someString");

        assert_partial_matching_and_non_matching(&data, &other);
    }

    #[test]
    fn json_partial_string() {
        let data = json!("someString");
        let other = json!("someOtherString");

        assert_partial_matching_and_non_matching(&data, &other);
    }

    #[test]
    fn json_partial_number() {
        let data = json!(42);
        let other = json!(17);

        assert_partial_matching_and_non_matching(&data, &other);
    }

    #[test]
    fn json_partial_bool() {
        let data = json!(true);
        let other = json!(false);

        assert_partial_matching_and_non_matching(&data, &other);
    }

    #[test]
    fn json_partial_arrays() {
        let data = json!([1, 2, 3]);
        let other = json!([1, 2, 3, 4]);

        assert_partial_matching_and_non_matching(&data, &other);
    }

    #[test]
    fn json_partial_object_matching() {
        let data = json!({"a": 0, "b": [1, 2]});
        let matching_pattern = json!({"a": 0});

        let matcher = JsonPartial::new(matching_pattern.clone());

        let serialized_exact = serde_json::to_string(&matching_pattern).unwrap();
        let serialized_partial = serde_json::to_string(&data).unwrap();

        assert!(matcher.matches(&serialized_exact));
        assert!(matcher.matches(&serialized_partial));
    }

    #[test]
    fn json_partial_object_non_matching() {
        let data = json!({"a": 0, "b": [1, 2]});
        let non_matching_pattern = json!({"a": 0, "c": 1});

        let matcher = JsonPartial::new(non_matching_pattern.clone());

        let serialized_exact = serde_json::to_string(&non_matching_pattern).unwrap();
        let serialized_unexpected = serde_json::to_string(&data).unwrap();

        assert!(matcher.matches(&serialized_exact));
        assert!(!matcher.matches(&serialized_unexpected));
    }

    fn assert_partial_matching_and_non_matching(data: &Value, other: &Value) {
        let matcher = JsonPartial::new(data.clone());

        let serialized_expected = serde_json::to_string(&data).unwrap();
        let serialized_unexpected = serde_json::to_string(&other).unwrap();

        assert!(matcher.matches(&serialized_expected));
        assert!(!matcher.matches(&serialized_unexpected));
    }
}
