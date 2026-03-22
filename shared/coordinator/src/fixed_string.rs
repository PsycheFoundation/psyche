use std::fmt::Display;

use anchor_lang::{AnchorDeserialize, AnchorSerialize, InitSpace, prelude::borsh};
use bytemuck::Zeroable;

#[derive(Clone, Copy, AnchorSerialize, AnchorDeserialize, PartialEq, Eq, InitSpace, Zeroable)]
#[cfg_attr(
    feature = "client",
    derive(serde::Serialize, serde::Deserialize, ts_rs::TS)
)]
pub struct FixedString<const L: usize>(
    #[cfg_attr(
        feature = "client",
        serde(
            serialize_with = "serde_serialize_string",
            deserialize_with = "serde_deserialize_string"
        )
    )]
    #[cfg_attr(feature = "client", ts(as = "String"))]
    [u8; L],
);

impl<const L: usize> std::fmt::Debug for FixedString<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let used_bytes = match self.0.iter().position(|&b| b == 0) {
            Some(null_pos) => null_pos,
            None => L,
        };

        let zero_bytes = L - used_bytes;

        let string_content = String::from(self);

        write!(
            f,
            "\"{string_content}\" ({used_bytes}/{L} bytes, {zero_bytes} zeroes)"
        )
    }
}

impl<const L: usize> Display for FixedString<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from(self))
    }
}

impl<const L: usize> Default for FixedString<L> {
    fn default() -> Self {
        Self([0u8; L])
    }
}

impl<const L: usize> TryFrom<&str> for FixedString<L> {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let bytes = value.as_bytes();
        if bytes.len() > L {
            return Err("str does not fit in FixedString");
        }
        Ok(Self::from_str_truncated(value))
    }
}

impl<const L: usize> TryFrom<&String> for FixedString<L> {
    type Error = &'static str;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        let bytes = value.as_bytes();
        if bytes.len() > L {
            return Err("str does not fit in FixedString");
        }
        Ok(Self::from_str_truncated(value))
    }
}

impl<const L: usize> FixedString<L> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn from_str_truncated(s: &str) -> Self {
        let mut array = [0u8; L];
        let bytes = s.as_bytes();
        let len = bytes.len().min(L);
        array[..len].copy_from_slice(&bytes[..len]);
        Self(array)
    }

    pub fn is_empty(&self) -> bool {
        self.0[0] == 0
    }
}

impl<const L: usize> From<&FixedString<L>> for String {
    fn from(value: &FixedString<L>) -> Self {
        let sliced = match value.0.iter().position(|&b| b == 0) {
            Some(null_pos) => &value.0[0..null_pos],
            None => &value.0,
        };
        String::from_utf8_lossy(sliced).to_string()
    }
}

impl<const L: usize> From<[u8; L]> for FixedString<L> {
    fn from(value: [u8; L]) -> Self {
        Self(value)
    }
}

impl<const L: usize> From<FixedString<L>> for [u8; L] {
    fn from(value: FixedString<L>) -> Self {
        value.0
    }
}

#[cfg(feature = "client")]
pub fn serde_serialize_string<S>(
    run_id: &[u8],
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    // Convert bytes to string, trimming null bytes
    let s = String::from_utf8_lossy(run_id)
        .trim_matches(char::from(0))
        .to_string();
    serializer.serialize_str(&s)
}

#[cfg(feature = "client")]
pub fn serde_deserialize_string<'de, D, const N: usize>(
    deserializer: D,
) -> std::result::Result<[u8; N], D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = <std::string::String as serde::Deserialize>::deserialize(deserializer)?;
    let mut bytes = [0u8; N];
    let len = std::cmp::min(s.len(), N);
    bytes[..len].copy_from_slice(&s.as_bytes()[..len]);
    Ok(bytes)
}

#[cfg(all(test, feature = "client"))]
mod test {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct MyStrStruct {
        #[serde(
            serialize_with = "serde_serialize_string",
            deserialize_with = "serde_deserialize_string"
        )]
        field: [u8; 64],
    }

    #[test]
    fn test_serialize_deserialize_string() {
        let my_struct = MyStrStruct { field: [1u8; 64] };

        let bytes = postcard::to_stdvec(&my_struct).unwrap();
        let deserialized_struct: MyStrStruct = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(my_struct, deserialized_struct);
    }
}
