use crate::util::AppImageEnvironment;
use emusync_domain::execution::{PathPicker, PickerType};
use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalChooserResult {
    Selected(String),
    Cancelled,
    NotAvailable,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FilePicker;

impl PathPicker for FilePicker {
    type Error = io::Error;

    fn pick(
        &self,
        label: &str,
        initial_path: &str,
        kind: PickerType,
    ) -> Result<Option<String>, Self::Error> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".into());

        let start_path = if initial_path.trim().is_empty() {
            PathBuf::from(home)
        } else {
            PathBuf::from(initial_path)
        };

        let parent_dir = if start_path.is_dir() {
            start_path.clone()
        } else {
            start_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        };

        match kind {
            PickerType::File => Ok(pick_file(label, &parent_dir, &start_path)),
            PickerType::Directory => Ok(pick_directory(label, &parent_dir)),
            PickerType::Any => Ok(None),
        }
    }
}

fn pick_file(label: &str, parent_dir: &Path, start_file: &Path) -> Option<String> {
    let initial_path = if start_file.is_file() {
        start_file.to_string_lossy().to_string()
    } else {
        format!("{}/", parent_dir.display())
    };

    let commands = [
        vec![
            "kdialog".to_string(),
            "--title".to_string(),
            format!("Select {label}"),
            "--getopenfilename".to_string(),
            initial_path.clone(),
        ],
        vec![
            "zenity".to_string(),
            "--file-selection".to_string(),
            "--title".to_string(),
            format!("Select {label}"),
            format!("--filename={initial_path}"),
        ],
    ];

    match run_external_choosers(&commands) {
        ExternalChooserResult::Selected(path) => Some(path),
        ExternalChooserResult::Cancelled | ExternalChooserResult::NotAvailable => None,
    }
}

fn pick_directory(label: &str, parent_dir: &Path) -> Option<String> {
    let parent_str = parent_dir.to_string_lossy().to_string();
    let commands = [
        vec![
            "kdialog".to_string(),
            "--title".to_string(),
            format!("Select {label}"),
            "--getexistingdirectory".to_string(),
            parent_str.clone(),
        ],
        vec![
            "zenity".to_string(),
            "--file-selection".to_string(),
            "--directory".to_string(),
            "--title".to_string(),
            format!("Select {label}"),
            format!("--filename={parent_str}/"),
        ],
    ];

    match run_external_choosers(&commands) {
        ExternalChooserResult::Selected(path) => Some(path),
        ExternalChooserResult::Cancelled | ExternalChooserResult::NotAvailable => None,
    }
}

pub fn run_external_choosers(commands: &[Vec<String>]) -> ExternalChooserResult {
    for cmd in commands {
        if cmd.is_empty() {
            continue;
        }
        let result = run_single_chooser(&cmd[0], &cmd[1..]);
        if result != ExternalChooserResult::NotAvailable {
            return result;
        }
    }
    ExternalChooserResult::NotAvailable
}

fn run_single_chooser(binary: &str, args: &[String]) -> ExternalChooserResult {
    let mut command = Command::new(binary);
    command.args(args);
    AppImageEnvironment::clean_command(&mut command);
    let output = match command.output() {
        Ok(out) => out,
        Err(_) => return ExternalChooserResult::NotAvailable,
    };

    match output.status.code() {
        Some(0) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !stdout.is_empty() {
                ExternalChooserResult::Selected(stdout)
            } else {
                ExternalChooserResult::NotAvailable
            }
        }
        Some(1) => ExternalChooserResult::Cancelled,
        _ => ExternalChooserResult::NotAvailable,
    }
}
