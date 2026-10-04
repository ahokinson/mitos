use std::fmt;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn id() -> String {
    Uuid::new_v4().to_string()
}

pub fn now() -> Timestamp {
    Timestamp(Utc::now().to_rfc3339())
}

macro_rules! text_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl std::str::FromStr for $name {
            type Err = std::convert::Infallible;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Ok(Self(value.to_owned()))
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.pad(&self.0)
            }
        }
    };
}

macro_rules! generated_id {
    ($(#[$meta:meta])* $name:ident) => {
        text_newtype!($(#[$meta])* $name);

        impl $name {
            pub fn generate() -> Self {
                Self(id())
            }
        }
    };
}

generated_id!(ThreadId);
generated_id!(WorkspaceId);
generated_id!(TurnId);
generated_id!(RequestId);

text_newtype! {
    /// RFC 3339, compared as text.
    Timestamp
}

#[cfg(test)]
mod tests {
    use super::{RequestId, ThreadId, Timestamp, now};

    #[test]
    fn ids_are_unique_and_display_as_their_text() {
        let (first, second) = (ThreadId::generate(), ThreadId::generate());
        assert_ne!(first, second);
        assert_eq!(first.to_string(), first.as_str());
        assert_eq!(format!("{:<4}|", ThreadId::from("t")), "t   |");
    }

    #[test]
    fn ids_serialize_as_plain_strings() {
        let id = RequestId::from("r1");
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"r1\"");
        assert_eq!(serde_json::from_str::<RequestId>("\"r1\"").unwrap(), id);
    }

    #[test]
    fn later_timestamps_order_after_earlier_ones() {
        let earlier = Timestamp::from("2026-01-01T00:00:00+00:00");
        assert!(earlier < now());
    }
}
