//! Header values that may be secret (FR-038, research R7).
//!
//! A secret value can be received (from the UI or the CLI) but is never
//! serialized: files and IPC responses only carry `{"secret": true}`. Its
//! `Debug` output is redacted and its memory is wiped on drop.

use secrecy::{ExposeSecret, SecretString};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone)]
pub enum HeaderValue {
    Plain(String),
    /// `None` when loaded from a file: the user must enter it again.
    Secret(Option<SecretString>),
}

impl HeaderValue {
    pub fn secret(value: impl Into<String>) -> Self {
        Self::Secret(Some(SecretString::from(value.into())))
    }

    pub fn is_secret(&self) -> bool {
        matches!(self, Self::Secret(_))
    }

    /// The value to put on the wire, if it is known.
    pub fn expose(&self) -> Option<&str> {
        match self {
            Self::Plain(v) => Some(v),
            Self::Secret(Some(s)) => Some(s.expose_secret()),
            Self::Secret(None) => None,
        }
    }
}

impl std::fmt::Debug for HeaderValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plain(v) => f.debug_tuple("Plain").field(v).finish(),
            Self::Secret(Some(_)) => f.write_str("Secret([REDACTED])"),
            Self::Secret(None) => f.write_str("Secret(missing)"),
        }
    }
}

impl PartialEq for HeaderValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Plain(a), Self::Plain(b)) => a == b,
            (Self::Secret(a), Self::Secret(b)) => {
                a.as_ref().map(|s| s.expose_secret()) == b.as_ref().map(|s| s.expose_secret())
            }
            _ => false,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Wire {
    #[serde(skip_serializing_if = "Option::is_none")]
    plain: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    secret: bool,
    /// Accepted on input only.
    #[serde(default, skip_serializing)]
    value: Option<String>,
}

impl Serialize for HeaderValue {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::Plain(v) => Wire {
                plain: Some(v.clone()),
                secret: false,
                value: None,
            },
            // The secret itself is never written.
            Self::Secret(_) => Wire {
                plain: None,
                secret: true,
                value: None,
            },
        };
        wire.serialize(s)
    }
}

impl<'de> Deserialize<'de> for HeaderValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wire = Wire::deserialize(d)?;
        match (wire.plain, wire.secret) {
            (Some(v), false) => Ok(Self::Plain(v)),
            (None, true) => Ok(Self::Secret(wire.value.map(SecretString::from))),
            _ => Err(D::Error::custom(
                "a header value is either {\"plain\": \"...\"} or {\"secret\": true}",
            )),
        }
    }
}

/// A password or token (spec 025): like [`HeaderValue::Secret`], it is
/// accepted on input but never serialized, redacted in `Debug` and wiped on
/// drop. `None` means not known: loaded from a file, or not entered yet.
#[derive(Clone, Default)]
pub struct Secret(Option<SecretString>);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Some(SecretString::from(value.into())))
    }

    pub fn expose(&self) -> Option<&str> {
        self.0.as_ref().map(|s| s.expose_secret())
    }

    pub fn is_set(&self) -> bool {
        self.0.is_some()
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "Secret([REDACTED])"
        } else {
            "Secret(missing)"
        })
    }
}

impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        self.expose() == other.expose()
    }
}

impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // The value itself is never written.
        Wire {
            plain: None,
            secret: true,
            value: None,
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wire = Wire::deserialize(d)?;
        if wire.plain.is_some() {
            return Err(D::Error::custom("a secret is {\"secret\": true}, never plain"));
        }
        Ok(Self(wire.value.map(SecretString::from)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_is_never_serialized_or_printed() {
        let v = HeaderValue::secret("hunter2");
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, r#"{"secret":true}"#);
        assert!(!format!("{v:?}").contains("hunter2"));
        assert_eq!(v.expose(), Some("hunter2"));
    }

    #[test]
    fn secret_value_is_accepted_on_input_and_missing_from_files() {
        let from_ui: HeaderValue = serde_json::from_str(r#"{"secret":true,"value":"abc"}"#).unwrap();
        assert_eq!(from_ui.expose(), Some("abc"));
        let from_file: HeaderValue = serde_json::from_str(r#"{"secret":true}"#).unwrap();
        assert_eq!(from_file.expose(), None);
    }

    #[test]
    fn password_is_never_serialized() {
        let p = Secret::new("hunter2");
        assert_eq!(serde_json::to_string(&p).unwrap(), r#"{"secret":true}"#);
        assert!(!format!("{p:?}").contains("hunter2"));
        let back: Secret = serde_json::from_str(r#"{"secret":true}"#).unwrap();
        assert!(!back.is_set());
        let typed: Secret = serde_json::from_str(r#"{"secret":true,"value":"pw"}"#).unwrap();
        assert_eq!(typed.expose(), Some("pw"));
        assert!(serde_json::from_str::<Secret>(r#"{"plain":"pw"}"#).is_err());
    }

    #[test]
    fn plain_round_trips() {
        let v = HeaderValue::Plain("abc".into());
        let back: HeaderValue = serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
        assert_eq!(back, v);
    }
}
