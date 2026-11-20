//! `signed_sha256` against hostile manifests and signatures: only the exact bytes the
//! release key signed, in the exact format `cargo xtask sign-and-publish-release` writes,
//! may yield the hash of an asset.

use crate::common::signed_release::{sign_with_test_key, test_release_key};
use base64::{Engine, engine::general_purpose::STANDARD};
use emusync_data_updates::{ReleaseKey, UpdateError, signed_sha256};
use ring::signature::Ed25519KeyPair;

const APPIMAGE: &str = "EmuSync-x86_64.AppImage";
const MSI: &str = "EmuSync-x86_64.msi";

/// SHA-256 of the empty string (FIPS 180-2 test vector).
const EMPTY_SHA256_HEX: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const EMPTY_SHA256: [u8; 32] = [
    0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9, 0x24,
    0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55,
];
/// SHA-256 of "abc" (FIPS 180-2 test vector).
const ABC_SHA256_HEX: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const ABC_SHA256: [u8; 32] = [
    0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22, 0x23,
    0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00, 0x15, 0xad,
];

/// The seed of RFC 8032, section 7.1, test 2: a key other than the test release key.
const OTHER_SEED: [u8; 32] = [
    0x4c, 0xcd, 0x08, 0x9b, 0x28, 0xff, 0x96, 0xda, 0x9d, 0xb6, 0xc3, 0x46, 0xec, 0x11, 0x4e, 0x0f,
    0x5b, 0x8a, 0x31, 0x9f, 0x35, 0xab, 0xa6, 0x24, 0xda, 0x8c, 0xf6, 0xed, 0x4f, 0xb8, 0xa6, 0xfb,
];

/// The public key of RFC 8032, section 7.1, test 1, as the RFC prints it.
const RFC_8032_PUBLIC_KEY_BASE64: &str = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";
/// Python `cryptography` signing `b"EmuSync release v2.2.0\n"` with the RFC 8032 test 1
/// key (the same fixture as `xtask/src/release_signing.rs`).
const HEADER_ONLY_SIGNATURE_BASE64: &str =
    "r75D6ZZvnaLqhsK31uk2W3rQm/NXhtK+0/vwFMWmfUTrHmGs1+MOg1jdFM/MOFYjra7cxgwvpkisbIyYxdcwCw==";

/// The manifest of v2.0.0 listing the empty string as the AppImage and "abc" as the MSI.
fn two_asset_manifest() -> String {
    format!("EmuSync release v2.0.0\n{EMPTY_SHA256_HEX}  {APPIMAGE}\n{ABC_SHA256_HEX}  {MSI}\n")
}

fn assert_refused(result: Result<[u8; 32], UpdateError>, case: &str) {
    match result {
        Err(UpdateError::Verification(_)) => {}
        other => panic!("{case}: expected a verification error, got {other:?}"),
    }
}

/// `manifest` signed by the test key, then checked for `asset` of `tag`.
fn check_signed(manifest: &[u8], tag: &str, asset: &str) -> Result<[u8; 32], UpdateError> {
    let signature = sign_with_test_key(manifest);
    signed_sha256(&test_release_key(), tag, asset, manifest, &signature)
}

#[test]
fn a_signed_manifest_yields_the_hash_listed_for_each_asset() {
    let manifest = two_asset_manifest();

    let appimage_sha256 = check_signed(manifest.as_bytes(), "v2.0.0", APPIMAGE)
        .expect("the AppImage is listed in a signed manifest");
    let msi_sha256 =
        check_signed(manifest.as_bytes(), "v2.0.0", MSI).expect("the MSI is listed too");

    assert_eq!(appimage_sha256, EMPTY_SHA256);
    assert_eq!(msi_sha256, ABC_SHA256);
}

#[test]
fn every_hex_digit_of_the_listed_hash_is_decoded() {
    let manifest = format!(
        "EmuSync release v2.0.0\n{}  {APPIMAGE}\n",
        "0123456789abcdef".repeat(4)
    );

    let sha256 = check_signed(manifest.as_bytes(), "v2.0.0", APPIMAGE)
        .expect("a manifest of lowercase hex is valid");

    assert_eq!(
        sha256,
        [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
            0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67,
            0x89, 0xab, 0xcd, 0xef,
        ]
    );
}

#[test]
fn a_signature_made_by_another_ed25519_implementation_is_verified() {
    let key = ReleaseKey::from_base64(RFC_8032_PUBLIC_KEY_BASE64).expect("the RFC key is valid");
    let manifest = b"EmuSync release v2.2.0\n";

    let result = signed_sha256(
        &key,
        "v2.2.0",
        APPIMAGE,
        manifest,
        HEADER_ONLY_SIGNATURE_BASE64.as_bytes(),
    );

    // The signature is accepted; the manifest then fails only because it lists no asset.
    match result {
        Err(error @ UpdateError::Verification(_)) => assert_eq!(
            error.to_string(),
            format!("Update verification failed: the release manifest does not list {APPIMAGE}")
        ),
        other => panic!("expected the header-only manifest to list nothing, got {other:?}"),
    }
}

#[test]
fn the_signature_may_be_surrounded_by_whitespace() {
    let manifest = two_asset_manifest();
    let signature = sign_with_test_key(manifest.as_bytes());
    let signature = String::from_utf8(signature).expect("base64 is text");
    let signature = signature.trim();

    for (case, framed_signature) in [
        ("bare", signature.to_owned()),
        ("leading spaces", format!("  {signature}")),
        ("tabs and CRLF", format!("\t{signature} \r\n")),
        ("blank lines", format!("\n\n{signature}\n\n")),
    ] {
        let sha256 = signed_sha256(
            &test_release_key(),
            "v2.0.0",
            MSI,
            manifest.as_bytes(),
            framed_signature.as_bytes(),
        )
        .unwrap_or_else(|error| panic!("{case}: expected the signature to verify: {error}"));
        assert_eq!(sha256, ABC_SHA256, "{case}");
    }
}

#[test]
fn a_signature_that_is_not_one_ed25519_signature_in_base64_is_refused() {
    let manifest = two_asset_manifest();
    let signature = sign_with_test_key(manifest.as_bytes());
    let signature = String::from_utf8(signature).expect("base64 is text");
    let signature_bytes = STANDARD
        .decode(signature.trim())
        .expect("the test signature is base64");

    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("only whitespace", b" \n".to_vec()),
        ("not base64", b"!!!! not a signature !!!!".to_vec()),
        (
            "trailing text",
            format!("{} extra", signature.trim()).into_bytes(),
        ),
        (
            "whitespace inside",
            format!("{} {}", &signature[..40], &signature.trim()[40..]).into_bytes(),
        ),
        (
            "one byte short",
            STANDARD.encode(&signature_bytes[..63]).into_bytes(),
        ),
        (
            "one byte too many",
            STANDARD
                .encode([signature_bytes.as_slice(), &[0]].concat())
                .into_bytes(),
        ),
        ("raw bytes instead of base64", signature_bytes.clone()),
        ("not UTF-8", {
            let mut bytes = signature.clone().into_bytes();
            bytes.insert(0, 0xff);
            bytes
        }),
    ];

    for (case, signature) in cases {
        assert_refused(
            signed_sha256(
                &test_release_key(),
                "v2.0.0",
                MSI,
                manifest.as_bytes(),
                &signature,
            ),
            case,
        );
    }
}

#[test]
fn a_manifest_signed_by_another_key_is_refused() {
    let manifest = two_asset_manifest();
    let other_key_pair =
        Ed25519KeyPair::from_seed_unchecked(&OTHER_SEED).expect("the RFC 8032 seed is valid");
    let other_signature = STANDARD.encode(other_key_pair.sign(manifest.as_bytes()).as_ref());

    assert_refused(
        signed_sha256(
            &test_release_key(),
            "v2.0.0",
            MSI,
            manifest.as_bytes(),
            other_signature.as_bytes(),
        ),
        "signed by another key",
    );
    // The key built into the app is not the published test key.
    assert_refused(
        signed_sha256(
            &ReleaseKey::emusync(),
            "v2.0.0",
            MSI,
            manifest.as_bytes(),
            &sign_with_test_key(manifest.as_bytes()),
        ),
        "signed by the test key, checked with the EmuSync key",
    );
}

#[test]
fn a_manifest_with_any_byte_changed_after_signing_is_refused() {
    let manifest = two_asset_manifest().into_bytes();
    let signature = sign_with_test_key(&manifest);
    signed_sha256(&test_release_key(), "v2.0.0", MSI, &manifest, &signature)
        .expect("the untouched manifest verifies");

    for position in 0..manifest.len() {
        let mut altered_manifest = manifest.clone();
        altered_manifest[position] ^= 0x01;
        assert_refused(
            signed_sha256(
                &test_release_key(),
                "v2.0.0",
                MSI,
                &altered_manifest,
                &signature,
            ),
            &format!("bit 0 of byte {position} flipped"),
        );
    }

    let truncated = &manifest[..manifest.len() - 1];
    assert_refused(
        signed_sha256(&test_release_key(), "v2.0.0", MSI, truncated, &signature),
        "last byte removed",
    );
    let extended = [manifest.as_slice(), b"\n"].concat();
    assert_refused(
        signed_sha256(&test_release_key(), "v2.0.0", MSI, &extended, &signature),
        "line break appended",
    );
}

#[test]
fn a_signed_manifest_of_another_release_is_refused() {
    let manifest = two_asset_manifest();

    for tag in [
        "v1.9.0", "2.0.0", "V2.0.0", "v2.0.0 ", " v2.0.0", "v2.0", "",
    ] {
        assert_refused(
            check_signed(manifest.as_bytes(), tag, MSI),
            &format!("manifest of v2.0.0 checked for {tag:?}"),
        );
    }
    let manifest_without_v =
        format!("EmuSync release 2.0.0\n{ABC_SHA256_HEX}  {MSI}\n").into_bytes();
    assert_refused(
        check_signed(&manifest_without_v, "v2.0.0", MSI),
        "manifest of 2.0.0 checked for v2.0.0",
    );
}

#[test]
fn a_signed_manifest_that_breaks_the_line_format_is_refused() {
    let appimage_line = format!("{EMPTY_SHA256_HEX}  {APPIMAGE}");
    let msi_line = format!("{ABC_SHA256_HEX}  {MSI}");
    let header = "EmuSync release v2.0.0";

    let cases: Vec<(&str, String)> = vec![
        (
            "CRLF line endings",
            format!("{header}\r\n{appimage_line}\r\n{msi_line}\r\n"),
        ),
        (
            "no final line break",
            format!("{header}\n{appimage_line}\n{msi_line}"),
        ),
        (
            "blank last line",
            format!("{header}\n{appimage_line}\n{msi_line}\n\n"),
        ),
        (
            "blank line between assets",
            format!("{header}\n{appimage_line}\n\n{msi_line}\n"),
        ),
        (
            "header after the assets",
            format!("{appimage_line}\n{msi_line}\n{header}\n"),
        ),
        (
            "header in lower case",
            format!("emusync release v2.0.0\n{msi_line}\n"),
        ),
        (
            "header with a trailing space",
            format!("{header} \n{msi_line}\n"),
        ),
        (
            "header with a leading space",
            format!(" {header}\n{msi_line}\n"),
        ),
        ("byte order mark", format!("\u{feff}{header}\n{msi_line}\n")),
        ("header only", format!("{header}\n")),
        ("empty manifest", String::new()),
        (
            "upper-case hash",
            format!("{header}\n{}  {MSI}\n", ABC_SHA256_HEX.to_uppercase()),
        ),
        (
            "one upper-case hex digit",
            format!(
                "{header}\n{}  {MSI}\n",
                ABC_SHA256_HEX.replacen("ba", "bA", 1)
            ),
        ),
        (
            "63 hex digits",
            format!("{header}\n{}  {MSI}\n", &ABC_SHA256_HEX[..63]),
        ),
        (
            "65 hex digits",
            format!("{header}\n{ABC_SHA256_HEX}0  {MSI}\n"),
        ),
        (
            "non-hex digit",
            format!(
                "{header}\n{}  {MSI}\n",
                ABC_SHA256_HEX.replacen('b', "g", 1)
            ),
        ),
        (
            "one space before the name",
            format!("{header}\n{ABC_SHA256_HEX} {MSI}\n"),
        ),
        (
            "tab before the name",
            format!("{header}\n{ABC_SHA256_HEX}\t{MSI}\n"),
        ),
        (
            "three spaces before the name",
            format!("{header}\n{ABC_SHA256_HEX}   {MSI}\n"),
        ),
        (
            "space after the name",
            format!("{header}\n{ABC_SHA256_HEX}  {MSI} \n"),
        ),
        (
            "binary-mode marker of sha256sum",
            format!("{header}\n{ABC_SHA256_HEX} *{MSI}\n"),
        ),
        (
            "leading space before the hash",
            format!("{header}\n {ABC_SHA256_HEX}  {MSI}\n"),
        ),
        (
            "empty name on another line",
            format!("{header}\n{EMPTY_SHA256_HEX}  \n{msi_line}\n"),
        ),
        (
            "malformed line of another asset",
            format!("{header}\n{EMPTY_SHA256_HEX} {APPIMAGE}\n{msi_line}\n"),
        ),
        (
            "text line among the assets",
            format!("{header}\n# checksums\n{msi_line}\n"),
        ),
        (
            "asset listed twice with the same hash",
            format!("{header}\n{msi_line}\n{msi_line}\n"),
        ),
        (
            "asset listed twice with different hashes",
            format!("{header}\n{msi_line}\n{EMPTY_SHA256_HEX}  {MSI}\n"),
        ),
        ("asset not listed", format!("{header}\n{appimage_line}\n")),
        (
            "only look-alike names",
            format!(
                "{header}\n{ABC_SHA256_HEX}  {MSI}.old\n{ABC_SHA256_HEX}  x{MSI}\n\
                 {ABC_SHA256_HEX}  emusync-x86_64.msi\n{ABC_SHA256_HEX}  ./{MSI}\n"
            ),
        ),
    ];

    for (case, manifest) in cases {
        assert_refused(check_signed(manifest.as_bytes(), "v2.0.0", MSI), case);
    }
}

#[test]
fn a_signed_manifest_that_is_not_utf8_is_refused() {
    let mut manifest = format!("EmuSync release v2.0.0\n{ABC_SHA256_HEX}  {MSI}\n").into_bytes();
    // A Latin-1 name on another line: valid as a line in any 8-bit encoding, not as UTF-8.
    manifest.extend_from_slice(EMPTY_SHA256_HEX.as_bytes());
    manifest.extend_from_slice(b"  EmuSync-espa\xf1ol.AppImage\n");

    assert_refused(
        check_signed(&manifest, "v2.0.0", MSI),
        "manifest with a Latin-1 byte",
    );
}

#[test]
fn a_release_key_is_exactly_32_bytes_of_base64() {
    let rfc_key = ReleaseKey::from_base64(RFC_8032_PUBLIC_KEY_BASE64).expect("the RFC key");
    assert_eq!(
        ReleaseKey::from_base64(&format!(" {RFC_8032_PUBLIC_KEY_BASE64}\n"))
            .expect("surrounding whitespace is ignored"),
        rfc_key
    );
    assert_eq!(rfc_key, test_release_key());

    for (case, text) in [
        ("empty", String::new()),
        ("not base64", "not a key".to_owned()),
        ("31 bytes", STANDARD.encode([7u8; 31])),
        ("33 bytes", STANDARD.encode([7u8; 33])),
        ("64-byte signature", HEADER_ONLY_SIGNATURE_BASE64.to_owned()),
    ] {
        match ReleaseKey::from_base64(&text) {
            Err(UpdateError::Verification(_)) => {}
            other => panic!("{case}: expected a verification error, got {other:?}"),
        }
    }
}
