//! Signed release manifests for tests, made with a test key in the format that
//! `cargo xtask sign-and-publish-release` writes (as in the `data-updates` tests).

use base64::{Engine, engine::general_purpose::STANDARD};
use emusync_data_updates::{MANIFEST_NAME, ReleaseKey, SIGNATURE_NAME};
use ring::{
    digest::{SHA256, digest},
    signature::{Ed25519KeyPair, KeyPair},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};

/// The seed of RFC 8032, section 7.1, test 1: a published key, for tests only.
const TEST_SEED: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];

fn test_key_pair() -> Ed25519KeyPair {
    Ed25519KeyPair::from_seed_unchecked(&TEST_SEED).expect("the RFC 8032 seed is a valid key")
}

/// The public key that verifies what [`signed_manifest`] signs.
pub fn test_release_key() -> ReleaseKey {
    ReleaseKey::from_base64(&STANDARD.encode(test_key_pair().public_key().as_ref()))
        .expect("the test public key is valid")
}

/// The manifest of `tag` listing each asset's SHA-256, and its signature by the test key.
pub fn signed_manifest(tag: &str, assets: &[(&str, &[u8])]) -> (Vec<u8>, Vec<u8>) {
    let mut manifest = format!("EmuSync release {tag}\n");
    for (name, bytes) in assets {
        let sha256: String = digest(&SHA256, bytes)
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        manifest.push_str(&format!("{sha256}  {name}\n"));
    }
    let signature = format!(
        "{}\n",
        STANDARD.encode(test_key_pair().sign(manifest.as_bytes()).as_ref())
    );
    (manifest.into_bytes(), signature.into_bytes())
}

/// A loopback server answering `/EmuSync-SHA256SUMS` and `/EmuSync-SHA256SUMS.sig` with
/// the given bytes, and 404 to anything else, until dropped.
pub struct SignedFilesServer {
    pub manifest_url: String,
    pub signature_url: String,
    task: JoinHandle<()>,
}

impl SignedFilesServer {
    pub async fn serve(manifest: Vec<u8>, signature: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind signed files listener");
        let base = format!(
            "http://{}",
            listener.local_addr().expect("signed files address")
        );
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut head = Vec::new();
                let mut byte = [0; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    match stream.read(&mut byte).await {
                        Ok(1) => head.push(byte[0]),
                        _ => break,
                    }
                }
                let head = String::from_utf8_lossy(&head);
                let target = head.split_whitespace().nth(1).unwrap_or_default();
                let body = if target == format!("/{MANIFEST_NAME}") {
                    Some(&manifest)
                } else if target == format!("/{SIGNATURE_NAME}") {
                    Some(&signature)
                } else {
                    None
                };
                let response = match body {
                    Some(body) => {
                        let mut response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        response.extend_from_slice(body);
                        response
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = stream.write_all(&response).await;
                let _ = stream.shutdown().await;
            }
        });
        Self {
            manifest_url: format!("{base}/{MANIFEST_NAME}"),
            signature_url: format!("{base}/{SIGNATURE_NAME}"),
            task,
        }
    }
}

impl Drop for SignedFilesServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
