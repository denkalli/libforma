//! Stable identifiers for pages, nodes and assets.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

macro_rules! uuid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Creates a new random identifier.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wraps an existing UUID.
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Returns the underlying UUID.
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
    };
}

uuid_id!(
    /// Identifies a [`Page`](crate::model::Page) within a document.
    PageId
);

uuid_id!(
    /// Identifies a [`Node`](crate::model::Node) within a document.
    NodeId
);

/// Identifies an asset by the BLAKE3 hash of its content.
///
/// Two identical images share the same id, so they are stored only once.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetId([u8; 32]);

impl AssetId {
    /// Computes the id of the given content.
    pub fn for_content(data: &[u8]) -> Self {
        Self(*blake3::hash(data).as_bytes())
    }

    /// Returns the id as a lowercase hexadecimal string.
    pub fn to_hex(&self) -> String {
        blake3::Hash::from_bytes(self.0).to_hex().to_string()
    }

    /// Parses an id from its hexadecimal form.
    pub fn from_hex(hex: &str) -> Option<Self> {
        blake3::Hash::from_hex(hex)
            .ok()
            .map(|h| Self(*h.as_bytes()))
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AssetId({})", self.to_hex())
    }
}

impl Serialize for AssetId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for AssetId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let hex = String::deserialize(deserializer)?;
        Self::from_hex(&hex).ok_or_else(|| serde::de::Error::custom("invalid asset id"))
    }
}
