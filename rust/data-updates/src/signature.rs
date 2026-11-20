//! Verification of the manifest that `cargo xtask sign-and-publish-release` signs and adds
//! to every release: `EmuSync release <tag>`, then one `<sha256>  <asset>` line per asset.

use crate::error::UpdateError;
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::signature::{ED25519, UnparsedPublicKey};
use std::fmt;

/// The names under which a release carries its manifest and the manifest's signature.
pub const MANIFEST_NAME: &str = "EmuSync-SHA256SUMS";
pub const SIGNATURE_NAME: &str = "EmuSync-SHA256SUMS.sig";

/// The public half of the Ed25519 key that signs EmuSync releases. Its private half is
/// kept offline by the maintainer.
const EMUSYNC_RELEASE_KEY: &str = "AgMChY3jo4CXUcsj1jp2CgTV/4mjULKivus59JBuzEc=";

/// The public key a release manifest must be signed with.
#[derive(Clone, PartialEq, Eq)]
pub struct ReleaseKey([u8; 32]);

impl ReleaseKey {
    /// The key of the EmuSync releases on GitHub.
    pub fn emusync() -> Self {
        Self::from_base64(EMUSYNC_RELEASE_KEY).expect("the embedded release key is valid")
    }

    /// A key in the base64 form that `cargo xtask release-key` prints.
    pub fn from_base64(text: &str) -> Result<Self, UpdateError> {
        let bytes = STANDARD
            .decode(text.trim())
            .map_err(|_| UpdateError::Verification("the release key is not base64".into()))?;
        let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
            UpdateError::Verification("the release key is not an Ed25519 public key".into())
        })?;
        Ok(Self(bytes))
    }
}

impl fmt::Debug for ReleaseKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ReleaseKey")
            .field(&STANDARD.encode(self.0))
            .finish()
    }
}

/// The SHA-256 of `asset` that the manifest of release `tag` lists, once `signature`
/// proves that `key` signed exactly `manifest`. Anything else is an error: another
/// release's manifest, a malformed line, or an asset the manifest does not list.
pub fn signed_sha256(
    key: &ReleaseKey,
    tag: &str,
    asset: &str,
    manifest: &[u8],
    signature: &[u8],
) -> Result<[u8; 32], UpdateError> {
    let invalid = |reason: &str| UpdateError::Verification(reason.to_owned());
    let signature = std::str::from_utf8(signature)
        .ok()
        .and_then(|text| STANDARD.decode(text.trim()).ok())
        .ok_or_else(|| invalid("the release signature is not base64"))?;
    UnparsedPublicKey::new(&ED25519, key.0)
        .verify(manifest, &signature)
        .map_err(|_| invalid("the release manifest is not signed with the trusted release key"))?;

    let manifest =
        std::str::from_utf8(manifest).map_err(|_| invalid("the release manifest is not text"))?;
    let lines = manifest
        .strip_suffix('\n')
        .ok_or_else(|| invalid("the release manifest does not end with a line break"))?;
    let mut lines = lines.split('\n');
    if lines.next() != Some(format!("EmuSync release {tag}").as_str()) {
        return Err(invalid("the release manifest belongs to another release"));
    }
    let mut found = None;
    for line in lines {
        let (hash, name) = line
            .split_once("  ")
            .filter(|(hash, name)| {
                hash.len() == 64
                    && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
                    && !name.is_empty()
            })
            .ok_or_else(|| invalid("the release manifest has a malformed line"))?;
        if name == asset {
            if found.is_some() {
                return Err(invalid("the release manifest lists the asset twice"));
            }
            found = Some(decode_hex(hash));
        }
    }
    found.ok_or_else(|| {
        UpdateError::Verification(format!("the release manifest does not list {asset}"))
    })
}

/// The bytes of 64 lowercase hex digits, already checked by the caller.
fn decode_hex(hex: &str) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    for (byte, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks(2)) {
        let digit = |c: u8| match c {
            b'0'..=b'9' => c - b'0',
            _ => c - b'a' + 10,
        };
        *byte = digit(pair[0]) << 4 | digit(pair[1]);
    }
    bytes
}
