mod perf;
mod perf_drive;
mod release_signing;
mod test_layout;

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand};
use std::{error::Error, process::Command};

fn allowed(source: &str, target: &str) -> bool {
    match source {
        "emusync-domain" => false,
        "emusync-application"
        | "emusync-data-local"
        | "emusync-data-drive"
        | "emusync-data-updates"
        | "emusync-platform-desktop" => target == "emusync-domain",
        "emusync-presentation" => matches!(target, "emusync-domain" | "emusync-application"),
        "emusync-desktop-app" => target.starts_with("emusync-"),
        "xtask" => false,
        _ => false,
    }
}

fn architecture(metadata: &Metadata) -> Result<(), Box<dyn Error>> {
    for package in metadata.workspace_packages() {
        for dependency in &package.dependencies {
            if dependency.kind != DependencyKind::Normal {
                continue;
            }
            let target = dependency.name.as_str();
            let internal = metadata
                .workspace_packages()
                .iter()
                .any(|p| p.name.as_str() == target);
            if (internal && !allowed(package.name.as_str(), target))
                || (package.name.as_str() == "emusync-domain")
                || (package.name.as_str() == "emusync-application" && !internal)
            {
                return Err(format!("forbidden dependency: {} -> {target}", package.name).into());
            }
        }
    }
    println!("Architecture dependencies verified");
    Ok(())
}

fn package_tests(metadata: &Metadata) -> Vec<test_layout::PackageTests> {
    metadata
        .workspace_packages()
        .into_iter()
        .filter_map(|package| {
            let dir = package.manifest_path.parent()?.as_std_path().to_path_buf();
            let test_targets = package
                .targets
                .iter()
                .filter(|target| target.is_test())
                .map(|target| test_layout::TestTarget {
                    name: target.name.clone(),
                    source_path: target.src_path.as_std_path().to_path_buf(),
                })
                .collect();
            Some(test_layout::PackageTests {
                name: package.name.to_string(),
                dir,
                test_targets,
            })
        })
        .collect()
}

fn run(args: &[&str]) -> Result<(), Box<dyn Error>> {
    println!("cargo {}", args.join(" "));
    if !Command::new("cargo").args(args).status()?.success() {
        return Err(format!("cargo {} failed", args.join(" ")).into());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let command = std::env::args().nth(1).unwrap_or_default();
    if command == "perf" {
        let args: Vec<String> = std::env::args().skip(2).collect();
        return perf::main(&args);
    }
    if matches!(
        command.as_str(),
        "release-key" | "release-public-key" | "sign-and-publish-release"
    ) {
        let args: Vec<String> = std::env::args().skip(1).collect();
        return release_signing::main(&args);
    }
    if !matches!(command.as_str(), "quality" | "test" | "check") {
        return Err("usage: cargo xtask quality|test|check|perf|release-key|release-public-key|sign-and-publish-release".into());
    }
    // `quality` and `test` are the two halves of `check`, so CI can run them in parallel.
    if command != "test" {
        let metadata = MetadataCommand::new()
            .no_deps()
            .other_options(vec!["--locked".into()])
            .exec()?;
        architecture(&metadata)?;
        run(&["fmt", "--all", "--", "--check"])?;
        // After the format check, so that a `main.rs` that does not parse gets rustfmt's
        // diagnostic first.
        test_layout::verify(&package_tests(&metadata))?;
        run(&[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ])?;
    }
    if command != "quality" {
        // Run every test binary so one failure does not hide the others.
        run(&["test", "--workspace", "--locked", "--no-fail-fast"])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::allowed;

    #[test]
    fn layer_dependencies_follow_the_contract() {
        assert!(allowed("emusync-application", "emusync-domain"));
        assert!(allowed("emusync-presentation", "emusync-application"));
        assert!(!allowed("emusync-domain", "emusync-application"));
        assert!(!allowed("emusync-presentation", "emusync-data-local"));
        assert!(!allowed("emusync-data-drive", "emusync-application"));
    }
}
