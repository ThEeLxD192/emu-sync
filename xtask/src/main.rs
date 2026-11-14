mod perf;
mod perf_drive;

use cargo_metadata::{DependencyKind, MetadataCommand};
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

fn architecture() -> Result<(), Box<dyn Error>> {
    let metadata = MetadataCommand::new()
        .no_deps()
        .other_options(vec!["--locked".into()])
        .exec()?;
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
    if !matches!(command.as_str(), "quality" | "test" | "check") {
        return Err("usage: cargo xtask quality|test|check|perf".into());
    }
    // `quality` and `test` are the two halves of `check`, so CI can run them in parallel.
    if command != "test" {
        architecture()?;
        run(&["fmt", "--all", "--", "--check"])?;
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
