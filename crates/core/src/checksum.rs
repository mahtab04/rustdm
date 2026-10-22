use anyhow::{Result, ensure};
use ring::digest::{Context, SHA256};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// A validated SHA-256 digest, represented as 64 lowercase hexadecimal characters.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Sha256Checksum(String);

impl FromStr for Sha256Checksum {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        ensure!(
            value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "SHA-256 must contain exactly 64 hexadecimal characters"
        );
        Ok(Self(value.to_ascii_lowercase()))
    }
}

impl TryFrom<String> for Sha256Checksum {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self> {
        value.parse()
    }
}

impl From<Sha256Checksum> for String {
    fn from(value: Sha256Checksum) -> Self {
        value.0
    }
}

impl fmt::Display for Sha256Checksum {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Incremental verification of the exact bytes written to the assembled file.
pub(crate) struct ChecksumVerifier {
    expected: Sha256Checksum,
    context: Context,
}

impl ChecksumVerifier {
    pub(crate) fn new(expected: &Sha256Checksum) -> Self {
        Self {
            expected: expected.clone(),
            context: Context::new(&SHA256),
        }
    }
    pub(crate) fn update(&mut self, bytes: &[u8]) {
        self.context.update(bytes);
    }
    pub(crate) fn verify(self) -> Result<()> {
        let actual: String = self
            .context
            .finish()
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        ensure!(
            actual == self.expected.0,
            "SHA-256 mismatch: expected {}, actual {}. Output was not published; partial files retained. Use a fresh output path to download again if saved data is corrupt",
            self.expected,
            actual
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/core/checksum.rs"]
mod tests;
