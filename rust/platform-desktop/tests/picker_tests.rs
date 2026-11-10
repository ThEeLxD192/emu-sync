//! `run_external_choosers` con selectores falsos (`/bin/sh -c`); nunca se abren diálogos reales.

#![cfg(unix)]

mod common;

use emusync_domain::execution::{PathPicker, PickerType};
use emusync_platform_desktop::{ExternalChooserResult, FilePicker, run_external_choosers};
use tempfile::tempdir;

fn sh(script: &str) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), script.into()]
}

#[test]
fn cancelled_chooser_stops_chain_without_running_fallback() {
    let dir = tempdir().unwrap();
    let mark = dir.path().join("MARK");
    let fallback = format!("touch '{}'; printf '/fallback'", mark.display());

    let result = run_external_choosers(&[sh("exit 1"), sh(&fallback)]);

    assert_eq!(result, ExternalChooserResult::Cancelled);
    assert!(!mark.exists(), "the fallback chooser must not run");
}

#[test]
fn empty_output_or_unexpected_status_falls_through_to_next_chooser() {
    let result = run_external_choosers(&[
        sh("exit 0"),
        sh("exit 2"),
        vec!["/nonexistent/bin".into()],
        sh("printf '/sel'"),
    ]);

    assert_eq!(result, ExternalChooserResult::Selected("/sel".into()));
}

#[test]
fn selected_path_trims_only_trailing_line_breaks() {
    let result = run_external_choosers(&[sh("printf '/a b /\\r\\n'")]);

    assert_eq!(result, ExternalChooserResult::Selected("/a b /".into()));
}

#[test]
fn empty_command_vectors_are_skipped_and_all_unavailable_reports_not_available() {
    let result = run_external_choosers(&[vec![], vec!["/nonexistent".into()]]);

    assert_eq!(result, ExternalChooserResult::NotAvailable);
    assert_eq!(
        run_external_choosers(&[]),
        ExternalChooserResult::NotAvailable
    );
}

#[test]
fn any_picker_type_returns_none_without_spawning() {
    assert_eq!(FilePicker.pick("x", "", PickerType::Any).unwrap(), None);
}
