use emusync_data_local::ConfigManager;
use emusync_desktop_app::{
    AppEvent, AuthorizedTransferFailure, CloseState, DesktopCoordinator, RuntimeOwner,
    apply_entry_games_scanned, apply_steam_state_refreshed,
};
use emusync_domain::config::ConfigurationStore;
use emusync_platform_desktop::SteamShortcutManager;
use emusync_presentation::{
    EmuSyncApp, actions::UiAction, model::AppStatus, theme::configure_egui_theme,
};
use std::{
    ffi::OsString,
    num::NonZeroU32,
    sync::{
        Arc,
        mpsc::{Receiver, Sender, channel},
    },
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

struct App {
    window: Option<Arc<Window>>,
    state: Option<egui_winit::State>,
    painter: Option<egui_wgpu::winit::Painter>,
    emusync_app: EmuSyncApp,
    configuration: ConfigManager,
    coordinator: Option<DesktopCoordinator>,
    event_tx: Sender<AppEvent>,
    event_rx: Receiver<AppEvent>,
    error: Option<String>,
    operations: RuntimeOwner<AuthorizedTransferFailure<ConfigManager>>,
    closing: bool,
    close_notice: bool,
    current_scan_revision: u64,
    current_steam_revision: u64,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| -> Result<_, Box<dyn std::error::Error>> {
            let window = Arc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title("EmuSync")
                        .with_inner_size(winit::dpi::LogicalSize::new(1080.0, 720.0)),
                )?,
            );
            let context = egui::Context::default();
            configure_egui_theme(&context);

            let state = egui_winit::State::new(
                context.clone(),
                egui::ViewportId::ROOT,
                &*window,
                Some(window.scale_factor() as f32),
                window.theme(),
                None,
            );
            let mut painter = pollster::block_on(egui_wgpu::winit::Painter::new(
                context,
                Default::default(),
                false,
                Default::default(),
            ));
            pollster::block_on(painter.set_window(egui::ViewportId::ROOT, Some(window.clone())))?;

            let w_clone = window.clone();
            let redraw = Arc::new(move || {
                w_clone.request_redraw();
            });

            let coordinator = DesktopCoordinator::new(
                self.configuration.clone(),
                self.event_tx.clone(),
                redraw,
                self.operations.handle().clone(),
            )
            .with_supervisor(self.operations.supervisor());
            coordinator.initialize();

            Ok((window, state, painter, coordinator))
        })();
        match result {
            Ok((window, state, painter, coordinator)) => {
                self.window = Some(window);
                self.state = Some(state);
                self.painter = Some(painter);
                self.coordinator = Some(coordinator);
            }
            Err(error) => {
                self.error = Some(error.to_string());
                event_loop.exit();
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if !self.closing {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        let coord_close = self
            .coordinator
            .as_ref()
            .map(|c| c.poll_close())
            .unwrap_or(CloseState::Ready);
        let ops_close = self.operations.poll_close();
        let close_state = match (coord_close, ops_close) {
            (CloseState::RecoveryRequired, _) | (_, CloseState::RecoveryRequired) => {
                CloseState::RecoveryRequired
            }
            (CloseState::Pending, _) | (_, CloseState::Pending) => CloseState::Pending,
            (CloseState::Ready, CloseState::Ready) => CloseState::Ready,
        };
        match close_state {
            CloseState::Pending => event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(50),
            )),
            CloseState::RecoveryRequired => {
                self.closing = false;
                self.close_notice = true;
                self.emusync_app.state.status = AppStatus::Error {
                    message: "A sync operation needs recovery before closing.".into(),
                };
                event_loop.set_control_flow(ControlFlow::Wait);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            CloseState::Ready => event_loop.exit(),
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let (Some(window), Some(state), Some(painter)) =
            (&self.window, &mut self.state, &mut self.painter)
        else {
            return;
        };
        if window.id() != id {
            return;
        }
        let response = state.on_window_event(window, &event);
        if response.repaint {
            window.request_redraw();
        }
        match event {
            WindowEvent::CloseRequested => {
                self.close_notice = false;
                self.closing = true;
                if let Some(coordinator) = &self.coordinator {
                    coordinator.begin_close();
                }
                self.operations.begin_close();
            }
            WindowEvent::Resized(size) => {
                if let (Some(width), Some(height)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                {
                    painter.on_window_resized(egui::ViewportId::ROOT, width, height);
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                // Ingest background events
                while let Ok(app_event) = self.event_rx.try_recv() {
                    match app_event {
                        AppEvent::ConfigReloaded(cfg) => {
                            let was_empty = self.emusync_app.state.selected_entry.is_none();
                            self.emusync_app.state.config = Some(cfg.clone());
                            if was_empty || self.emusync_app.state.selected_entry.is_none() {
                                self.emusync_app.state.selected_entry =
                                    cfg.entries.first().cloned();
                            } else if let Some(selected) = &self.emusync_app.state.selected_entry {
                                self.emusync_app.state.selected_entry = cfg
                                    .entries
                                    .iter()
                                    .find(|e| e.common().name == selected.common().name)
                                    .cloned()
                                    .or_else(|| cfg.entries.first().cloned());
                            }
                            if self.emusync_app.state.selected_entry.is_none() {
                                self.emusync_app.state.game_items.clear();
                            }
                            let current_names: std::collections::HashSet<_> = cfg
                                .entries
                                .iter()
                                .map(|e| e.common().name.clone())
                                .collect();
                            self.emusync_app
                                .state
                                .entry_sync_status
                                .retain(|k, _| current_names.contains(k));
                            self.emusync_app
                                .state
                                .steam_registered_entries
                                .retain(|k| current_names.contains(k));
                            if let Some(coordinator) = &self.coordinator {
                                coordinator.track_task(
                                    coordinator
                                        .platform
                                        .refresh_steam_state_async(cfg.entries.clone()),
                                );
                            }

                            if let Some(selected) = &self.emusync_app.state.selected_entry
                                && let Some(coordinator) = &self.coordinator
                            {
                                coordinator
                                    .handle_action(UiAction::SelectEntry(Some(selected.clone())));
                            }
                        }
                        AppEvent::EntryGamesScanned {
                            entry_name,
                            games,
                            revision,
                        } => {
                            let selected_name = self
                                .emusync_app
                                .state
                                .selected_entry
                                .as_ref()
                                .map(|e| e.common().name.as_str());
                            apply_entry_games_scanned(
                                &mut self.current_scan_revision,
                                &mut self.emusync_app.state.game_items,
                                selected_name,
                                &entry_name,
                                games,
                                revision,
                            );
                        }
                        AppEvent::SaveSetupRequested { request } => {
                            self.emusync_app.open_save_setup(&request);
                            window.request_redraw();
                        }
                        AppEvent::SyncStatusUpdated { entry_name, status } => {
                            self.emusync_app
                                .state
                                .entry_sync_status
                                .insert(entry_name, status);
                        }
                        AppEvent::SteamRegistrationUpdated {
                            entry_name,
                            registered,
                        } => {
                            if registered {
                                self.emusync_app
                                    .state
                                    .steam_registered_entries
                                    .insert(entry_name);
                            } else {
                                self.emusync_app
                                    .state
                                    .steam_registered_entries
                                    .remove(&entry_name);
                            }
                        }
                        AppEvent::SteamStateRefreshed {
                            available,
                            registered,
                            revision,
                        } => {
                            let state = &mut self.emusync_app.state;
                            apply_steam_state_refreshed(
                                &mut self.current_steam_revision,
                                &mut state.steam_available,
                                &mut state.steam_registered_entries,
                                available,
                                registered,
                                revision,
                            );
                        }
                        AppEvent::StatusChanged(status) => {
                            self.emusync_app.state.status = status;
                        }
                        AppEvent::Toast(message) => {
                            self.emusync_app.state.toast_message = Some(message);
                        }
                        AppEvent::UpdateStateChanged(update_state) => {
                            self.emusync_app.state.update_state = update_state;
                        }
                        AppEvent::PathPicked { target, mode, path } => {
                            self.emusync_app.apply_picked_path(target, mode, path);
                            window.request_redraw();
                        }
                        AppEvent::GameEditReady { item, targets } => {
                            // A dialog opened meanwhile keeps priority.
                            if !self.emusync_app.has_open_dialog() {
                                self.emusync_app.open_edit_game(&item, &targets);
                            }
                        }
                        AppEvent::RestorePathsPreviewed { destination, paths } => {
                            self.emusync_app.apply_restore_preview(&destination, paths);
                        }
                        AppEvent::SaveTargetTypeDetected { index, path, kind } => {
                            self.emusync_app
                                .apply_detected_save_type(index, &path, kind);
                        }
                        AppEvent::CloseNotice(notice) => {
                            self.close_notice = true;
                            self.emusync_app.state.status = AppStatus::Error { message: notice };
                        }
                    }
                }

                let context = state.egui_ctx().clone();
                let input = state.take_egui_input(window);
                let mut output = context.run_ui(input, |ui| {
                    self.emusync_app.update(ui);
                });

                // Drain and dispatch UI actions
                let actions = self.emusync_app.take_actions();
                for action in actions {
                    if let UiAction::SelectEntry(ref entry) = action {
                        self.emusync_app.state.selected_entry = entry.clone();
                        if entry.is_none() {
                            self.emusync_app.state.game_items.clear();
                        }
                        window.request_redraw();
                    }
                    if matches!(action, UiAction::Exit) {
                        self.close_notice = false;
                        self.closing = true;
                        if let Some(coordinator) = &self.coordinator {
                            coordinator.begin_close();
                        }
                        self.operations.begin_close();
                    } else if let Some(coordinator) = &self.coordinator {
                        coordinator.handle_action(action);
                    }
                }

                state.handle_platform_output(window, output.platform_output);
                let primitives = context.tessellate(output.shapes, output.pixels_per_point);
                painter.paint_and_update_textures(
                    egui::ViewportId::ROOT,
                    output.pixels_per_point,
                    [0.05, 0.07, 0.09, 1.0],
                    &primitives,
                    &mut output.textures_delta,
                    Vec::new(),
                    window,
                );
            }
            _ => {}
        }
    }
}

fn config_argument(
    args: impl IntoIterator<Item = OsString>,
) -> Result<Option<OsString>, &'static str> {
    let mut args = args.into_iter();
    let mut config = None;
    while let Some(arg) = args.next() {
        if arg == "--config" {
            if config.is_some() {
                return Err("--config may only be specified once");
            }
            let value = args.next().ok_or("--config requires a path")?;
            if value.is_empty() || value.to_string_lossy().trim().is_empty() {
                return Err("--config requires a path");
            }
            config = Some(value);
        } else {
            return Err("unknown command-line argument");
        }
    }
    Ok(config)
}

/// Debug builds only: `--preview-update=<state>` opens the update dialog with a
/// synthetic update so it can be reviewed. Downloads and restarts stay disabled in
/// this build, so the preview cannot change the installation.
#[cfg(debug_assertions)]
fn take_preview_update(
    args: Vec<OsString>,
) -> Result<(Vec<OsString>, Option<emusync_application::UpdateState>), &'static str> {
    let mut rest = Vec::new();
    let mut preview = None;
    for arg in args {
        match arg
            .to_str()
            .and_then(|a| a.strip_prefix("--preview-update="))
        {
            Some(name) => {
                if preview.is_some() {
                    return Err("--preview-update may only be specified once");
                }
                preview = Some(preview_update_state(name).ok_or(
                    "--preview-update expects available, downloading, ready, ready-steam or error",
                )?);
            }
            None => rest.push(arg),
        }
    }
    Ok((rest, preview))
}

#[cfg(debug_assertions)]
fn preview_update_state(name: &str) -> Option<emusync_application::UpdateState> {
    use emusync_application::UpdateState;
    let info = emusync_domain::update::UpdateInfo {
        version: "9.9.9".to_string(),
        tag_name: "v9.9.9".to_string(),
        release_notes: "Preview release notes.\n\n- First change\n- Second change\n- A longer line that wraps inside the release notes box to check the layout."
            .to_string(),
        download_url: String::new(),
        asset_size: 0,
        asset_name: "EmuSync-preview".to_string(),
    };
    let ready = |is_steam_game_mode| UpdateState::ReadyToRestart {
        downloaded_path: "/nonexistent/emusync-preview-update".to_string(),
        is_steam_game_mode,
    };
    Some(match name {
        "available" => UpdateState::Available(info),
        "downloading" => UpdateState::Downloading {
            progress: 0.42,
            info,
        },
        "ready" => ready(false),
        "ready-steam" => ready(true),
        "error" => UpdateState::Error("Preview: the download could not be verified.".to_string()),
        _ => return None,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    #[cfg(debug_assertions)]
    let (args, preview_update) = take_preview_update(args)?;
    let explicit = config_argument(args)?;
    let path = emusync_data_local::resolve_config_location(explicit.as_deref())?;
    let configuration = emusync_data_local::ConfigManager::new(&path)?;
    configuration.load()?;
    let event_loop = EventLoop::new()?;

    let (event_tx, event_rx) = channel();

    let mut emusync_app = EmuSyncApp::new();
    emusync_app.fullscreen_detector = SteamShortcutManager::detect_fullscreen_args;
    emusync_app.state.config_path = path.display().to_string();
    #[cfg(debug_assertions)]
    if let Some(state) = preview_update {
        emusync_app.state.update_state = state;
        emusync_app.state.show_update_dialog = true;
    }
    let mut app = App {
        window: None,
        state: None,
        painter: None,
        emusync_app,
        configuration,
        coordinator: None,
        event_tx,
        event_rx,
        error: None,
        operations: RuntimeOwner::new()?,
        closing: false,
        close_notice: false,
        current_scan_revision: 0,
        current_steam_revision: 0,
    };
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn config_argument_requires_one_path() {
        use super::config_argument;
        use std::ffi::OsString;
        assert_eq!(
            config_argument([OsString::from("--config"), OsString::from("my config.json")])
                .unwrap(),
            Some(OsString::from("my config.json"))
        );
        assert!(config_argument([OsString::from("--config")]).is_err());
        assert!(config_argument([OsString::from("--other")]).is_err());
    }
    #[test]
    fn preview_update_is_taken_out_before_parsing_config() {
        use super::{config_argument, take_preview_update};
        use std::ffi::OsString;

        let (rest, preview) = take_preview_update(vec![
            OsString::from("--config"),
            OsString::from("c.json"),
            OsString::from("--preview-update=ready-steam"),
        ])
        .unwrap();
        assert!(matches!(
            preview,
            Some(emusync_application::UpdateState::ReadyToRestart {
                is_steam_game_mode: true,
                ..
            })
        ));
        assert_eq!(
            config_argument(rest).unwrap(),
            Some(OsString::from("c.json"))
        );
        assert!(take_preview_update(vec![OsString::from("--preview-update=nope")]).is_err());
        assert!(
            take_preview_update(vec![
                OsString::from("--preview-update=error"),
                OsString::from("--preview-update=ready"),
            ])
            .is_err()
        );
        // Without the debug-only prefix the argument is still rejected.
        assert!(config_argument([OsString::from("--preview-update")]).is_err());
    }
    #[test]
    fn default_fonts_render_the_preview_text() {
        let context = egui::Context::default();
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            let font = egui::FontId::proportional(16.0);
            ui.fonts_mut(|fonts| {
                assert!(fonts.has_glyphs(&font, "EmuSync"));
            });
        });
        output.drop_without_applying_deltas();
    }
}
