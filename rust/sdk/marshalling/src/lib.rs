#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub mod bytes;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SerializationError(String);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeserializationError(String);

pub fn to_vec<T: Serialize>(val: &T) -> Result<Vec<u8>, SerializationError> {
    let mut bytes = Vec::new();
    match ciborium::ser::into_writer(val, &mut bytes) {
        Ok(_) => Ok(bytes),
        Err(e) => Err(SerializationError(e.to_string())),
    }
}

pub fn from_slice<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, DeserializationError> {
    ciborium::de::from_reader(bytes).map_err(|e| DeserializationError(e.to_string()))
}
