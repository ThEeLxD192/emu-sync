//! Signing of a release on the maintainer's machine, so the private key never reaches
//! GitHub. `release-key` creates the key pair once; `sign-and-publish-release` signs the
//! assets of a draft release and publishes it, so that the updater, which holds the
//! public key, can refuse an asset whose hash is not in a manifest signed with it.

use base64::{Engine, engine::general_purpose::STANDARD};
use ring::{
    digest::{Context, SHA256},
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

/// The only repository whose releases are signed, whatever `GH_REPO` or the directory's
/// default repository say.
const REPOSITORY: &str = "ThEeLxD192/emu-sync";

/// The public key the updater trusts (`EMUSYNC_RELEASE_KEY` in
/// `rust/data-updates/src/signature.rs`). A release signed with any other key could
/// never be installed, so no other key signs one.
const UPDATER_PUBLIC_KEY: &str = "AgMChY3jo4CXUcsj1jp2CgTV/4mjULKivus59JBuzEc=";

/// The signed list of asset hashes and its detached signature; the updater looks for
/// these names among the release's assets.
const MANIFEST_NAME: &str = "EmuSync-SHA256SUMS";
const SIGNATURE_NAME: &str = "EmuSync-SHA256SUMS.sig";

/// The assets the updater can install, in the order the manifest lists them.
const SIGNED_ASSETS: [&str; 3] = [
    "EmuSync-x86_64.AppImage",
    "EmuSync-x86_64.msi",
    "EmuSync-x86_64-portable.zip",
];

const USAGE: &str = "usage: cargo xtask release-key <private-key-path> | release-public-key <private-key-path> | sign-and-publish-release <tag> <private-key-path>";

pub fn main(args: &[String]) -> Result<(), Box<dyn Error>> {
    match args {
        [command, key_path] if command == "release-key" => generate_key(Path::new(key_path)),
        [command, key_path] if command == "release-public-key" => {
            println!("{}", public_key(&read_private_key(Path::new(key_path))?));
            Ok(())
        }
        [command, tag, key_path] if command == "sign-and-publish-release" => {
            sign_and_publish_release(tag, Path::new(key_path))
        }
        _ => Err(USAGE.into()),
    }
}

/// Creates the key pair: writes the private key to `key_path`, which must not exist and
/// must be outside the repository, and prints the public key for the updater.
fn generate_key(key_path: &Path) -> Result<(), Box<dyn Error>> {
    if cfg!(not(unix)) {
        return Err(
            "release-key only runs on Unix, where it can make the key readable by its owner alone"
                .into(),
        );
    }
    ensure_outside_repository(key_path)?;
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| "could not generate the signing key")?;
    let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| "the generated signing key is not valid")?;
    write_private_key(key_path, pkcs8.as_ref())?;
    println!(
        "Private key written to {}. Keep it offline and backed up: without it no update can be published, and it must never be committed. `cargo xtask release-public-key` prints its public key again.",
        key_path.display()
    );
    println!("Public key for the updater: {}", public_key(&key_pair));
    Ok(())
}

/// Signs the assets of the draft release `tag` and publishes it. A run cut short leaves
/// the release a draft and can be repeated: the manifest and signature are replaced.
fn sign_and_publish_release(tag: &str, key_path: &Path) -> Result<(), Box<dyn Error>> {
    // Read the key first, so a wrong path stops before anything is downloaded.
    let key_pair = read_private_key(key_path)?;
    if public_key(&key_pair) != UPDATER_PUBLIC_KEY {
        return Err(format!(
            "{} is not the key the updater trusts (its public key is {}, the updater's {UPDATER_PUBLIC_KEY})",
            key_path.display(),
            public_key(&key_pair)
        )
        .into());
    }
    ensure_release_tag(tag)?;
    println!(
        "Signing {tag} of {REPOSITORY} with public key {}",
        public_key(&key_pair)
    );

    let draft = draft_assets(tag)?;
    let directory = repository_root()?
        .join("target")
        .join("release-signing")
        .join(tag);
    if directory.exists() {
        fs::remove_dir_all(&directory)?;
    }
    fs::create_dir_all(&directory)?;
    let directory_arg = directory.to_string_lossy().into_owned();
    let mut hashes = Vec::new();
    for asset in SIGNED_ASSETS {
        gh_release(&["download", tag, "--pattern", asset, "--dir", &directory_arg])?;
        let sha256 = sha256_of_file(&directory.join(asset))?;
        let Some(listed_digest) = draft[asset]["digest"].as_str() else {
            return Err(format!(
                "gh lists no SHA-256 digest for {asset}; update the GitHub CLI to a version whose `release view --json assets` includes `digest`"
            )
            .into());
        };
        if listed_digest != format!("sha256:{sha256}") {
            return Err(format!(
                "{asset} downloaded with SHA-256 {sha256}, but GitHub lists {listed_digest}"
            )
            .into());
        }
        hashes.push((asset, sha256));
    }

    let manifest = manifest(tag, &hashes);
    let signature = sign(&key_pair, &manifest);
    let manifest_path = directory.join(MANIFEST_NAME);
    let signature_path = directory.join(SIGNATURE_NAME);
    fs::write(&manifest_path, &manifest)?;
    fs::write(&signature_path, &signature)?;
    gh_release(&[
        "upload",
        tag,
        &manifest_path.to_string_lossy(),
        &signature_path.to_string_lossy(),
        "--clobber",
    ])?;

    // Publish only what was signed: an asset replaced meanwhile would not match the
    // manifest.
    let signed = draft_assets(tag)?;
    for asset in SIGNED_ASSETS {
        if signed[asset] != draft[asset] {
            return Err(format!(
                "{asset} changed while {tag} was being signed; run the command again"
            )
            .into());
        }
    }
    ensure_newer_than_latest(tag)?;
    gh_release(&["edit", tag, "--draft=false", "--latest"])?;
    print!("{manifest}");
    println!("Published {tag}.");
    Ok(())
}

/// The signed assets of the draft release `tag`, by name: their `id`, `size` and
/// `digest` as GitHub lists them. Fails when the release is not a draft or lacks one.
fn draft_assets(
    tag: &str,
) -> Result<std::collections::HashMap<&'static str, serde_json::Value>, Box<dyn Error>> {
    let release: serde_json::Value =
        serde_json::from_slice(&gh_release(&["view", tag, "--json", "isDraft,assets"])?)?;
    if release["isDraft"] != serde_json::Value::Bool(true) {
        return Err(format!(
            "{tag} is not a draft: only a draft release is signed and then published"
        )
        .into());
    }
    let listed = release["assets"].as_array().cloned().unwrap_or_default();
    let mut assets = std::collections::HashMap::new();
    for name in SIGNED_ASSETS {
        let asset = listed
            .iter()
            .find(|asset| asset["name"] == name)
            .ok_or_else(|| format!("{tag} has no {name}"))?;
        assets.insert(
            name,
            serde_json::json!({
                "id": asset["id"],
                "size": asset["size"],
                "digest": asset["digest"],
            }),
        );
    }
    Ok(assets)
}

/// Refuses to make `tag` the latest release when the published latest is the same version
/// or newer, which would offer users an older update.
fn ensure_newer_than_latest(tag: &str) -> Result<(), Box<dyn Error>> {
    let latest = Command::new("gh")
        .args(["release", "view", "--json", "tagName", "--repo", REPOSITORY])
        .output()
        .map_err(|error| format!("cannot run the GitHub CLI (gh): {error}"))?;
    if !latest.status.success() {
        // No published release yet.
        return Ok(());
    }
    let latest: serde_json::Value = serde_json::from_slice(&latest.stdout)?;
    let latest_tag = latest["tagName"].as_str().unwrap_or_default();
    if release_version(tag) <= release_version(latest_tag) {
        return Err(format!(
            "{tag} is not newer than the published latest release {latest_tag}; it stays a draft"
        )
        .into());
    }
    Ok(())
}

/// The numbers of a `vMAJOR.MINOR.PATCH` tag, for ordering; anything else orders first.
fn release_version(tag: &str) -> Vec<u64> {
    tag.strip_prefix('v')
        .unwrap_or("")
        .split('.')
        .map(|number| number.parse().unwrap_or(0))
        .collect()
}

/// Accepts only the `vMAJOR.MINOR.PATCH` tags that the release workflow creates, so a tag
/// can neither add lines to the manifest nor leave `target/release-signing`.
fn ensure_release_tag(tag: &str) -> Result<(), Box<dyn Error>> {
    let numbers: Vec<&str> = tag.strip_prefix('v').unwrap_or("").split('.').collect();
    let is_release_tag = numbers.len() == 3
        && numbers
            .iter()
            .all(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()));
    if !is_release_tag {
        return Err(format!("{tag:?} is not a release tag like v2.2.0").into());
    }
    Ok(())
}

/// The manifest of `tag`: a line naming the release, then one `<sha256>  <asset>` line per
/// asset. The version line binds the hashes to this release, so the manifest of an older
/// one cannot pass for it.
fn manifest(tag: &str, hashes: &[(&str, String)]) -> String {
    let mut text = format!("EmuSync release {tag}\n");
    for (asset, sha256) in hashes {
        text.push_str(&format!("{sha256}  {asset}\n"));
    }
    text
}

/// The detached Ed25519 signature of `manifest`, in base64.
fn sign(key_pair: &Ed25519KeyPair, manifest: &str) -> String {
    format!(
        "{}\n",
        STANDARD.encode(key_pair.sign(manifest.as_bytes()).as_ref())
    )
}

fn public_key(key_pair: &Ed25519KeyPair) -> String {
    STANDARD.encode(key_pair.public_key().as_ref())
}

fn sha256_of_file(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut file =
        File::open(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut context = Context::new(&SHA256);
    let mut buffer = vec![0; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        context.update(&buffer[..read]);
    }
    Ok(context
        .finish()
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Writes the PKCS#8 key in base64, never over an existing file. On Unix only its owner
/// can read it; `generate_key` refuses other systems.
fn write_private_key(key_path: &Path, pkcs8: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(key_path).map_err(|error| {
        format!(
            "cannot create {} ({error}); an existing key is never overwritten",
            key_path.display()
        )
    })?;
    file.write_all(format!("{}\n", STANDARD.encode(pkcs8)).as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn read_private_key(key_path: &Path) -> Result<Ed25519KeyPair, Box<dyn Error>> {
    let text = fs::read_to_string(key_path)
        .map_err(|error| format!("cannot read the key {}: {error}", key_path.display()))?;
    let pkcs8 = STANDARD
        .decode(text.trim())
        .map_err(|_| format!("{} is not a key written by release-key", key_path.display()))?;
    Ok(Ed25519KeyPair::from_pkcs8(&pkcs8)
        .map_err(|_| format!("{} is not an Ed25519 key", key_path.display()))?)
}

/// Refuses a key path inside the repository, where it could end up committed.
fn ensure_outside_repository(key_path: &Path) -> Result<(), Box<dyn Error>> {
    let parent = match key_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let parent = parent
        .canonicalize()
        .map_err(|error| format!("cannot use {}: {error}", parent.display()))?;
    if parent.starts_with(repository_root()?) {
        return Err(format!(
            "{} is inside the repository; keep the private key outside it",
            key_path.display()
        )
        .into());
    }
    Ok(())
}

fn repository_root() -> Result<PathBuf, Box<dyn Error>> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask has no parent directory")?
        .canonicalize()?)
}

/// Runs `gh release <args> --repo` on the EmuSync repository and returns its output.
fn gh_release(args: &[&str]) -> Result<Vec<u8>, Box<dyn Error>> {
    let output = Command::new("gh")
        .arg("release")
        .args(args)
        .args(["--repo", REPOSITORY])
        .output()
        .map_err(|error| format!("cannot run the GitHub CLI (gh): {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "gh release {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{ED25519, UnparsedPublicKey};

    /// RFC 8032, section 7.1, test 1: secret seed, public key and the signature of the
    /// empty message.
    const RFC_8032_SEED_HEX: &str =
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
    const RFC_8032_PUBLIC_KEY_BASE64: &str = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";
    const RFC_8032_EMPTY_MESSAGE_SIGNATURE_BASE64: &str =
        "5VZDAMNgrHKQhuLMgG6CioSHfx645dl02HPgZSJJAVVfuIIVkKM7rMYeOXAc+bRr0lv18FlbviRlUUFDjnoQCw==";
    /// The RFC 8032 key as a PKCS#8 v2 document (RFC 5958 with the public key), in base64:
    /// the format a key file written by `release-key` holds. Built with Python, not ring.
    const RFC_8032_KEY_FILE_BASE64: &str = "MFECAQEwBQYDK2VwBCIEIJ1hsZ3v/VpguoRK9JLsLMREScVpezJpGXA7rAMcrn9ggSEA11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";
    /// The same seed as a PKCS#8 v1 document (what `openssl genpkey` writes): real key
    /// material that `release-key` did not write.
    const RFC_8032_PKCS8_V1_BASE64: &str =
        "MC4CAQAwBQYDK2VwBCIEIJ1hsZ3v/VpguoRK9JLsLMREScVpezJpGXA7rAMcrn9g";

    /// A directory removed on drop; `new` makes one under the system temporary directory.
    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new(purpose: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "emusync-xtask-{purpose}-{}-{}",
                std::process::id(),
                next_unique_number()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn next_unique_number() -> usize {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }

    /// A file name that no other test or person uses, for paths inside the repository
    /// that must never be created.
    fn unique_key_name() -> String {
        format!(
            "adversarial-release-{}-{}.key",
            std::process::id(),
            next_unique_number()
        )
    }

    fn rfc_8032_key_pair() -> Ed25519KeyPair {
        let seed: Vec<u8> = (0..RFC_8032_SEED_HEX.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&RFC_8032_SEED_HEX[i..i + 2], 16).unwrap())
            .collect();
        Ed25519KeyPair::from_seed_and_public_key(
            &seed,
            &STANDARD.decode(RFC_8032_PUBLIC_KEY_BASE64).unwrap(),
        )
        .unwrap()
    }

    /// Fails the test, after removing it, if `path` was created.
    fn assert_never_created(path: &Path) {
        if path.symlink_metadata().is_ok() {
            let _ = fs::remove_file(path);
            panic!("{} was created", path.display());
        }
    }

    #[cfg(unix)]
    #[test]
    fn release_key_writes_a_key_that_only_its_owner_can_read_and_that_reads_back() {
        let scratch = ScratchDir::new("generate");
        let key_path = scratch.0.join("release.key");

        main(&["release-key".to_owned(), key_path.display().to_string()]).unwrap();

        let text = fs::read_to_string(&key_path).unwrap();
        assert!(
            text.ends_with('\n') && text.lines().count() == 1,
            "{text:?}"
        );
        assert!(read_private_key(&key_path).is_ok());
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&key_path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(not(unix))]
    #[test]
    fn release_key_refuses_to_run_where_the_key_could_not_be_kept_private() {
        let scratch = ScratchDir::new("not-unix");
        let key_path = scratch.0.join("release.key");

        assert!(main(&["release-key".to_owned(), key_path.display().to_string()]).is_err());
        assert_never_created(&key_path);
    }

    #[test]
    fn release_public_key_prints_the_key_of_an_existing_key_file() {
        let scratch = ScratchDir::new("public-key");
        let key_path = scratch.0.join("release.key");
        fs::write(&key_path, format!("{RFC_8032_KEY_FILE_BASE64}\n")).unwrap();

        assert!(
            main(&[
                "release-public-key".to_owned(),
                key_path.display().to_string()
            ])
            .is_ok()
        );
        assert_eq!(
            public_key(&read_private_key(&key_path).unwrap()),
            RFC_8032_PUBLIC_KEY_BASE64
        );
    }

    #[test]
    fn the_signing_key_is_the_one_the_updater_trusts() {
        let updater_source = fs::read_to_string(
            repository_root()
                .unwrap()
                .join("rust/data-updates/src/signature.rs"),
        )
        .unwrap();
        assert!(
            updater_source.contains(&format!(
                "const EMUSYNC_RELEASE_KEY: &str = \"{UPDATER_PUBLIC_KEY}\";"
            )),
            "xtask and the updater must name the same release key"
        );
    }

    #[test]
    fn a_key_other_than_the_updaters_is_refused_before_touching_the_signing_directory() {
        let scratch = ScratchDir::new("other-key");
        let key_path = scratch.0.join("release.key");
        fs::write(&key_path, format!("{RFC_8032_KEY_FILE_BASE64}\n")).unwrap();
        // A tag no real release has, unique to this run, whose signing directory holds an
        // asset signed earlier. Signing empties that directory before downloading, so the
        // asset survives alone only if the wrong key stops everything first. The real
        // directories of published tags are never touched.
        let tag = format!("v0.0.{}", std::process::id());
        let signing_directory = ScratchDir(
            repository_root()
                .unwrap()
                .join("target")
                .join("release-signing")
                .join(&tag),
        );
        fs::create_dir_all(&signing_directory.0).unwrap();
        let earlier_asset = signing_directory.0.join("EmuSync-x86_64.AppImage");
        fs::write(&earlier_asset, b"signed earlier").unwrap();

        let message = main(&[
            "sign-and-publish-release".to_owned(),
            tag,
            key_path.display().to_string(),
        ])
        .unwrap_err()
        .to_string();

        assert!(
            message.contains("is not the key the updater trusts"),
            "{message}"
        );
        let entries: Vec<_> = fs::read_dir(&signing_directory.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(entries, ["EmuSync-x86_64.AppImage"]);
        assert_eq!(fs::read(&earlier_asset).unwrap(), b"signed earlier");
    }

    #[test]
    fn release_versions_order_by_number_not_by_text() {
        assert!(release_version("v2.10.0") > release_version("v2.9.9"));
        assert!(release_version("v3.0.0") > release_version("v2.99.99"));
        assert!(release_version("v2.2.0") == release_version("v2.2.0"));
        assert!(release_version("v2.2.1") > release_version(""));
    }

    #[test]
    fn only_release_tags_are_signed() {
        for tag in ["v2.2.0", "v0.0.1", "v10.20.30"] {
            assert!(ensure_release_tag(tag).is_ok(), "{tag}");
        }
        for tag in [
            "2.2.0",
            "v2.2",
            "v2.2.0.1",
            "v2.2.0-rc1",
            "v2..0",
            "v../../x",
            "v2.2.0\nabc  EmuSync-x86_64.AppImage",
            "-v2.2.0",
            "",
        ] {
            assert!(ensure_release_tag(tag).is_err(), "{tag:?}");
        }
    }

    #[test]
    fn release_key_over_an_existing_key_fails_and_keeps_the_old_key() {
        let scratch = ScratchDir::new("existing");
        let key_path = scratch.0.join("release.key");
        let existing_key = format!("{RFC_8032_KEY_FILE_BASE64}\n");
        fs::write(&key_path, &existing_key).unwrap();

        assert!(main(&["release-key".to_owned(), key_path.display().to_string()]).is_err());

        assert_eq!(fs::read_to_string(&key_path).unwrap(), existing_key);
    }

    #[cfg(unix)]
    #[test]
    fn release_key_does_not_follow_a_dangling_link_into_the_repository() {
        let scratch = ScratchDir::new("dangling");
        let target_inside = repository_root().unwrap().join(unique_key_name());
        let key_path = scratch.0.join("release.key");
        std::os::unix::fs::symlink(&target_inside, &key_path).unwrap();

        let result = generate_key(&key_path);

        assert_never_created(&target_inside);
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn release_key_does_not_follow_a_link_to_an_existing_file_and_keeps_it() {
        let scratch = ScratchDir::new("link-existing");
        let existing_path = scratch.0.join("old.key");
        fs::write(&existing_path, "old key\n").unwrap();
        let key_path = scratch.0.join("release.key");
        std::os::unix::fs::symlink(&existing_path, &key_path).unwrap();

        assert!(generate_key(&key_path).is_err());

        assert_eq!(fs::read_to_string(&existing_path).unwrap(), "old key\n");
    }

    #[cfg(unix)]
    #[test]
    fn release_key_refuses_a_directory_outside_that_links_into_the_repository() {
        let scratch = ScratchDir::new("dir-link");
        let xtask_directory = repository_root().unwrap().join("xtask");
        let link_path = scratch.0.join("looks-outside");
        std::os::unix::fs::symlink(&xtask_directory, &link_path).unwrap();
        let key_name = unique_key_name();

        let result = generate_key(&link_path.join(&key_name));

        assert_never_created(&xtask_directory.join(&key_name));
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn release_key_refuses_dot_dot_after_a_link_that_lands_in_the_repository() {
        // Lexically `<scratch>/link/../<name>` is `<scratch>/<name>`, outside; the
        // system resolves the link first, so it really is `<repository>/<name>`.
        let scratch = ScratchDir::new("dot-dot-link");
        let repository = repository_root().unwrap();
        let link_path = scratch.0.join("link");
        std::os::unix::fs::symlink(repository.join("xtask"), &link_path).unwrap();
        let key_name = unique_key_name();

        let result = generate_key(&link_path.join("..").join(&key_name));

        assert_never_created(&repository.join(&key_name));
        assert_never_created(&scratch.0.join(&key_name));
        assert!(result.is_err());
    }

    #[test]
    fn release_key_refuses_relative_paths_that_resolve_inside_the_repository() {
        let repository = repository_root().unwrap();
        let working_directory = std::env::current_dir().unwrap().canonicalize().unwrap();
        assert!(
            working_directory.starts_with(&repository),
            "cargo runs these tests from {}, expected inside {}",
            working_directory.display(),
            repository.display()
        );

        for prefix in ["", "./", "../", "src/../", "../xtask/"] {
            let key_name = unique_key_name();
            let relative_path = format!("{prefix}{key_name}");

            let result = generate_key(Path::new(&relative_path));

            assert_never_created(&working_directory.join(&relative_path));
            assert!(result.is_err(), "{relative_path} was accepted");
        }
    }

    #[test]
    fn release_key_refuses_dot_dot_that_climbs_back_into_the_repository() {
        let repository = repository_root().unwrap();
        let key_name = unique_key_name();
        let key_path = repository
            .join("xtask")
            .join("..")
            .join("rust")
            .join("..")
            .join(&key_name);

        let result = generate_key(&key_path);

        assert_never_created(&repository.join(&key_name));
        assert!(result.is_err());
    }

    #[test]
    fn release_key_refuses_a_missing_directory_inside_the_repository_without_creating_it() {
        let repository = repository_root().unwrap();
        let missing_directory = repository.join(unique_key_name());

        let result = generate_key(&missing_directory.join("release.key"));

        assert!(
            !missing_directory.exists(),
            "{} was created",
            missing_directory.display()
        );
        assert!(result.is_err());
    }

    #[test]
    fn a_file_that_is_not_a_release_key_fails_clearly_without_showing_its_contents() {
        let scratch = ScratchDir::new("invalid-key");
        let cases: [(&str, Vec<u8>, Option<&str>); 6] = [
            ("plain-text", b"hunter2-secret-token\n".to_vec(), Some("hunter2")),
            // Base64 of "hunter2-not-a-key".
            (
                "base64-not-a-key",
                b"aHVudGVyMi1ub3QtYS1rZXk=\n".to_vec(),
                Some("aHVudGVyMi1ub3QtYS1rZXk"),
            ),
            (
                "pkcs8-v1",
                format!("{RFC_8032_PKCS8_V1_BASE64}\n").into_bytes(),
                Some("MC4CAQAwBQYDK2VwBCIEIJ1hsZ3v"),
            ),
            (
                "pem",
                format!(
                    "-----BEGIN PRIVATE KEY-----\n{RFC_8032_PKCS8_V1_BASE64}\n-----END PRIVATE KEY-----\n"
                )
                .into_bytes(),
                Some("MC4CAQAwBQYDK2VwBCIEIJ1hsZ3v"),
            ),
            ("binary", vec![0xff, 0xfe, 0x00, 0x9d, 0x61, 0xb1], Some("\u{fffd}")),
            ("empty", Vec::new(), None),
        ];

        for (file_name, contents, secret_fragment) in cases {
            let key_path = scratch.0.join(file_name);
            fs::write(&key_path, &contents).unwrap();

            let message = match read_private_key(&key_path) {
                Ok(_) => panic!("{file_name} was accepted as a key"),
                Err(error) => error.to_string(),
            };

            assert!(
                message.contains(&key_path.display().to_string()),
                "{file_name}: the error does not name the file: {message}"
            );
            if let Some(secret_fragment) = secret_fragment {
                assert!(
                    !message.contains(secret_fragment),
                    "{file_name}: the error shows the file's contents: {message}"
                );
            }
        }
    }

    #[test]
    fn sign_release_with_an_invalid_key_stops_before_preparing_the_release() {
        let scratch = ScratchDir::new("sign-invalid");
        let key_path = scratch.0.join("release.key");
        fs::write(&key_path, "hunter2-secret-token\n").unwrap();
        let tag = format!("v0.0.0-adversarial-{}", std::process::id());

        let message = main(&[
            "sign-and-publish-release".to_owned(),
            tag.clone(),
            key_path.display().to_string(),
        ])
        .unwrap_err()
        .to_string();

        assert!(!message.contains("hunter2"), "{message}");
        assert!(
            message.contains(&key_path.display().to_string()),
            "{message}"
        );
        let release_directory = repository_root()
            .unwrap()
            .join("target")
            .join("release-signing")
            .join(&tag);
        assert!(!release_directory.exists());
    }

    #[test]
    fn a_key_file_in_the_release_key_format_reads_as_the_same_key() {
        let scratch = ScratchDir::new("key-format");
        let key_path = scratch.0.join("release.key");
        let pkcs8 = STANDARD.decode(RFC_8032_KEY_FILE_BASE64).unwrap();

        write_private_key(&key_path, &pkcs8).unwrap();

        assert_eq!(
            fs::read_to_string(&key_path).unwrap(),
            format!("{RFC_8032_KEY_FILE_BASE64}\n")
        );
        assert_eq!(
            public_key(&read_private_key(&key_path).unwrap()),
            RFC_8032_PUBLIC_KEY_BASE64
        );
    }

    #[test]
    fn signature_is_standard_ed25519_of_the_manifest_bytes() {
        let key_pair = rfc_8032_key_pair();

        assert_eq!(
            sign(&key_pair, ""),
            format!("{RFC_8032_EMPTY_MESSAGE_SIGNATURE_BASE64}\n")
        );
        // Expected from Python `cryptography` signing b"EmuSync release v2.2.0\n" with
        // the RFC 8032 key.
        assert_eq!(
            sign(&key_pair, "EmuSync release v2.2.0\n"),
            "r75D6ZZvnaLqhsK31uk2W3rQm/NXhtK+0/vwFMWmfUTrHmGs1+MOg1jdFM/MOFYjra7cxgwvpkisbIyYxdcwCw==\n"
        );
    }

    #[test]
    fn signature_rejects_every_manifest_but_the_signed_one_and_every_other_key() {
        let signed_manifest = "EmuSync release v2.2.0\n\
             e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  EmuSync-x86_64.AppImage\n\
             bf718b6f653bebc184e1479f1935b8da974d701b893afcf49e701f3e2f9f9c5a  EmuSync-x86_64.msi\n";
        let key_pair = rfc_8032_key_pair();
        let signature = STANDARD
            .decode(sign(&key_pair, signed_manifest).trim_end())
            .unwrap();
        let public_key_bytes = STANDARD.decode(RFC_8032_PUBLIC_KEY_BASE64).unwrap();
        let verifier = UnparsedPublicKey::new(&ED25519, &public_key_bytes);
        assert!(
            verifier
                .verify(signed_manifest.as_bytes(), &signature)
                .is_ok()
        );

        let altered_manifests = [
            // One hash digit changed.
            signed_manifest.replacen("e3b0c442", "e3b0c443", 1),
            // Hash in upper case.
            signed_manifest.replacen("e3b0c44298fc", "E3B0C44298FC", 1),
            // Lines swapped.
            "EmuSync release v2.2.0\n\
             bf718b6f653bebc184e1479f1935b8da974d701b893afcf49e701f3e2f9f9c5a  EmuSync-x86_64.msi\n\
             e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  EmuSync-x86_64.AppImage\n"
                .to_owned(),
            // Hashes moved to the other asset.
            signed_manifest
                .replacen("EmuSync-x86_64.AppImage", "EmuSync-x86_64.tmp", 1)
                .replacen("EmuSync-x86_64.msi", "EmuSync-x86_64.AppImage", 1)
                .replacen("EmuSync-x86_64.tmp", "EmuSync-x86_64.msi", 1),
            // Last line dropped, final newline dropped, a line added.
            signed_manifest
                .lines()
                .take(2)
                .map(|line| format!("{line}\n"))
                .collect(),
            signed_manifest.trim_end_matches('\n').to_owned(),
            format!("{signed_manifest}\n"),
            // Windows line endings, one space instead of two, another release.
            signed_manifest.replace('\n', "\r\n"),
            signed_manifest.replace("  ", " "),
            signed_manifest.replacen("v2.2.0", "v2.2.1", 1),
        ];
        for altered_manifest in &altered_manifests {
            assert!(
                verifier
                    .verify(altered_manifest.as_bytes(), &signature)
                    .is_err(),
                "the signature verified {altered_manifest:?}"
            );
        }

        let other_key_pair = Ed25519KeyPair::from_pkcs8(
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .unwrap()
                .as_ref(),
        )
        .unwrap();
        let other_verifier = UnparsedPublicKey::new(&ED25519, other_key_pair.public_key().as_ref());
        assert!(
            other_verifier
                .verify(signed_manifest.as_bytes(), &signature)
                .is_err()
        );
    }

    #[test]
    fn file_hash_is_lowercase_sha256_across_read_buffer_boundaries() {
        let scratch = ScratchDir::new("hash-sizes");
        // Expected values from `head -c <size> /dev/zero | tr '\0' a | sha256sum`.
        let cases = [
            (
                0,
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                65_536,
                "bf718b6f653bebc184e1479f1935b8da974d701b893afcf49e701f3e2f9f9c5a",
            ),
            (
                200_001,
                "299c0ddb1d4e35143ab7ea9f57c435d09dc425da1ad5d941c7028a234f4af719",
            ),
        ];

        for (size_bytes, expected_hash) in cases {
            let asset_path = scratch.0.join(format!("asset-{size_bytes}"));
            fs::write(&asset_path, vec![b'a'; size_bytes]).unwrap();

            assert_eq!(
                sha256_of_file(&asset_path).unwrap(),
                expected_hash,
                "{size_bytes} bytes"
            );
        }
    }

    #[test]
    fn manifest_lists_the_three_updater_assets_in_a_fixed_order() {
        let scratch = ScratchDir::new("full-manifest");
        let asset_sizes_bytes = [0, 65_536, 200_001];
        let mut hashes = Vec::new();
        for (asset, size_bytes) in SIGNED_ASSETS.into_iter().zip(asset_sizes_bytes) {
            let asset_path = scratch.0.join(asset);
            fs::write(&asset_path, vec![b'a'; size_bytes]).unwrap();
            hashes.push((asset, sha256_of_file(&asset_path).unwrap()));
        }

        assert_eq!(
            manifest("v2.2.0", &hashes),
            "EmuSync release v2.2.0\n\
             e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  EmuSync-x86_64.AppImage\n\
             bf718b6f653bebc184e1479f1935b8da974d701b893afcf49e701f3e2f9f9c5a  EmuSync-x86_64.msi\n\
             299c0ddb1d4e35143ab7ea9f57c435d09dc425da1ad5d941c7028a234f4af719  EmuSync-x86_64-portable.zip\n"
        );
    }

    #[test]
    fn wrong_argument_counts_print_usage_and_create_no_key() {
        let scratch = ScratchDir::new("usage");
        let key_path = scratch.0.join("release.key");
        let key_arg = key_path.display().to_string();

        let wrong_invocations = [
            vec!["release-key".to_owned()],
            vec![
                "release-key".to_owned(),
                key_arg.clone(),
                "extra".to_owned(),
            ],
            vec!["sign-and-publish-release".to_owned(), key_arg.clone()],
            vec![
                "release-key".to_owned(),
                "v2.2.0".to_owned(),
                key_arg.clone(),
            ],
        ];
        for arguments in wrong_invocations {
            let message = main(&arguments).unwrap_err().to_string();
            assert!(message.starts_with("usage:"), "{arguments:?}: {message}");
        }
        assert_never_created(&key_path);
    }
}
