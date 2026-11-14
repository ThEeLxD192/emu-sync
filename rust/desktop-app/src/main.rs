// Release builds on Windows are GUI applications: no console window opens behind the app.
// Debug builds keep the console so panics and diagnostics stay visible.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod perf;

use emusync_data_local::ConfigManager;
use emusync_desktop_app::{
    AppEvent, AuthorizedTransferFailure, CloseState, DesktopCoordinator, RuntimeOwner,
    apply_entry_games_scanned, apply_steam_state_refreshed,
};
use emusync_domain::config::ConfigurationStore;
use emusync_platform_desktop::{SteamShortcutManager, is_gamescope_session};
use emusync_presentation::{
    EmuSyncApp,
    actions::UiAction,
    model::{AppStatus, CloudSyncStatus},
    theme::configure_egui_theme,
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
    window::{Fullscreen, Window, WindowId},
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
    /// Inside gamescope the UI is scaled to fit the output (see `fit_pixels_per_point`).
    gamescope: bool,
    close_notice: bool,
    current_scan_revision: u64,
    current_steam_revision: u64,
    /// Frame metrics, only when `EMUSYNC_PERF_LOG` is set.
    perf: Option<perf::PerfLog>,
    /// When egui asked to be drawn again (an animation or a delayed repaint).
    repaint_at: Option<Instant>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| -> Result<_, Box<dyn std::error::Error>> {
            let mut attributes = Window::default_attributes()
                .with_title("EmuSync")
                .with_inner_size(winit::dpi::LogicalSize::new(1080.0, 720.0));
            if self.gamescope {
                // Gamescope letterboxes smaller windows and pointer/touch input then misses
                // what is drawn; filling its output keeps both in the same coordinates.
                attributes = attributes.with_fullscreen(Some(Fullscreen::Borderless(None)));
            }
            #[cfg(windows)]
            {
                use winit::platform::windows::IconExtWindows;
                // The executable carries the application icon as resource 1 (see build.rs).
                // Declaring it on the window gives the title bar its icon and keeps the
                // taskbar button consistent; without it the window has none of its own.
                if let Ok(icon) = winit::window::Icon::from_resource(1, None) {
                    attributes = attributes.with_window_icon(Some(icon));
                }
            }
            let window = Arc::new(event_loop.create_window(attributes)?);
            let context = egui::Context::default();
            configure_egui_theme(&context);
            // Background work (a decoded cover) asks egui for a repaint; only the window
            // can wake the event loop. Delayed repaints come from each frame's output.
            let repaint_window = window.clone();
            context.set_request_repaint_callback(move |info| {
                if info.delay.is_zero() {
                    repaint_window.request_redraw();
                }
            });

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
        let repaint_at = self.due_repaint();
        if !self.closing {
            event_loop
                .set_control_flow(repaint_at.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
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
            CloseState::Pending => {
                let poll = Instant::now() + Duration::from_millis(50);
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    repaint_at.map_or(poll, |at| at.min(poll)),
                ));
            }
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
        // egui-winit also asks to repaint after a redraw; following that would redraw
        // forever. What a frame needs next comes from its own `repaint_delay`.
        if response.repaint && !matches!(event, WindowEvent::RedrawRequested) {
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
                let frame_began = Instant::now();
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
                        AppEvent::SaveSetupClosed { request_id } => {
                            self.emusync_app.close_save_setup(request_id);
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
                            if let Some(perf) = &mut self.perf {
                                perf.observe_status(status_name(&status));
                            }
                            self.emusync_app.state.status = status;
                        }
                        AppEvent::Toast(message) => {
                            self.emusync_app.state.toast_message = Some(message);
                        }
                        AppEvent::UpdateStateChanged(update_state) => {
                            self.emusync_app.state.update_state = update_state;
                        }
                        AppEvent::PathPicked {
                            request_id,
                            target,
                            mode,
                            path,
                        } => {
                            self.emusync_app
                                .apply_picked_path(request_id, target, mode, path);
                            window.request_redraw();
                        }
                        AppEvent::GameEditReady { item, targets } => {
                            // A dialog opened meanwhile keeps priority.
                            if !self.emusync_app.has_open_dialog() {
                                self.emusync_app.open_edit_game(&item, &targets);
                            }
                        }
                        AppEvent::RestorePathsPreviewed {
                            request_id,
                            destination,
                            paths,
                        } => {
                            self.emusync_app
                                .apply_restore_preview(request_id, &destination, paths);
                        }
                        AppEvent::SaveTargetTypeDetected { index, path, kind } => {
                            self.emusync_app
                                .apply_detected_save_type(index, &path, kind);
                        }
                        AppEvent::ExitRequested => {
                            self.close_notice = false;
                            self.closing = true;
                            if let Some(coordinator) = &self.coordinator {
                                coordinator.begin_close();
                            }
                            self.operations.begin_close();
                        }
                        AppEvent::CloseNotice(notice) => {
                            self.close_notice = true;
                            self.emusync_app.state.status = AppStatus::Error { message: notice };
                        }
                    }
                }

                if self
                    .perf
                    .as_ref()
                    .is_some_and(|perf| perf.scenario() == perf::Scenario::Spinner)
                {
                    self.emusync_app.state.status = AppStatus::Syncing {
                        message: "Syncing...".into(),
                    };
                }

                if let (Some(perf), Some(coordinator)) = (&mut self.perf, &self.coordinator) {
                    let state = &self.emusync_app.state;
                    // The startup check of the selected entry has ended.
                    let checked = state.selected_entry.as_ref().filter(|entry| {
                        state
                            .entry_sync_status
                            .get(&entry.common().name)
                            .is_some_and(|status| {
                                !matches!(status, CloudSyncStatus::Idle | CloudSyncStatus::Checking)
                            })
                    });
                    if perf.take_sync_request(checked.is_some())
                        && let Some(entry) = checked
                    {
                        coordinator.handle_action(UiAction::SyncEntry(entry.clone()));
                    }
                }

                let context = state.egui_ctx().clone();
                if self.gamescope {
                    let size = window.inner_size();
                    if size.width > 0 && size.height > 0 {
                        let zoom = fit_pixels_per_point(size.width, size.height)
                            / window.scale_factor() as f32;
                        if (context.zoom_factor() - zoom).abs() > 0.001 {
                            context.set_zoom_factor(zoom);
                        }
                    }
                }
                let mut input = state.take_egui_input(window);
                let has_games = !self.emusync_app.state.game_items.is_empty();
                let perf_needs_frame = self
                    .perf
                    .as_mut()
                    .is_some_and(|perf| perf.drive(&mut input, has_games));
                let mut output = context.run_ui(input, |ui| {
                    self.emusync_app.update(ui);
                });
                let ui_done = Instant::now();
                let repaint_delay = output
                    .viewport_output
                    .get(&egui::ViewportId::ROOT)
                    .map_or(Duration::MAX, |viewport| viewport.repaint_delay);

                // Drain and dispatch UI actions
                let actions = self.emusync_app.take_actions();
                for action in actions {
                    // Shown at once; the coordinator stores it in the background.
                    if let UiAction::SetLanguage(language) = action {
                        if let Some(config) = &mut self.emusync_app.state.config {
                            config.language = language;
                        }
                        window.request_redraw();
                    }
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

                // Asked before presenting, so the next frame is not one refresh late.
                if repaint_delay.is_zero() || perf_needs_frame {
                    window.request_redraw();
                    self.repaint_at = None;
                } else {
                    // `Duration::MAX` (nothing to animate) overflows: no repaint is due.
                    self.repaint_at = Instant::now().checked_add(repaint_delay);
                }

                state.handle_platform_output(window, output.platform_output);
                let primitives = context.tessellate(output.shapes, output.pixels_per_point);
                window.pre_present_notify();
                painter.paint_and_update_textures(
                    egui::ViewportId::ROOT,
                    output.pixels_per_point,
                    [0.05, 0.07, 0.09, 1.0],
                    &primitives,
                    &mut output.textures_delta,
                    Vec::new(),
                    window,
                );
                if let Some(perf) = &mut self.perf {
                    perf.record(&perf::FrameSample {
                        began: frame_began,
                        ui: ui_done.duration_since(frame_began),
                        paint: ui_done.elapsed(),
                        covers: self.emusync_app.cover_cache.loaded_count(),
                        texture_bytes: self.emusync_app.cover_cache.texture_bytes(),
                        status: status_name(&self.emusync_app.state.status),
                    });
                }
            }
            _ => {}
        }
    }
}

impl App {
    /// Requests the repaint egui asked for once it is due, and returns when the next one is.
    fn due_repaint(&mut self) -> Option<Instant> {
        let at = self.repaint_at?;
        if at > Instant::now() {
            return Some(at);
        }
        self.repaint_at = None;
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        None
    }
}

/// A short name of `status` for the frame metrics.
fn status_name(status: &AppStatus) -> &'static str {
    match status {
        AppStatus::Idle => "idle",
        AppStatus::Loading => "loading",
        AppStatus::Syncing { .. } => "syncing",
        AppStatus::Playing => "playing",
        AppStatus::Error { .. } => "error",
        _ => "waiting",
    }
}

/// Size, in points, the window layout is designed for.
const DESIGN_SIZE: (f32, f32) = (1080.0, 720.0);

/// Pixels per point that fit the designed layout in an output of this size.
///
/// Gamescope gives X11 clients no DPI setting, so winit derives one from the panel's
/// physical size (2.25 on the Steam Deck) and the layout no longer fits its screen.
fn fit_pixels_per_point(width_px: u32, height_px: u32) -> f32 {
    let fit = (width_px as f32 / DESIGN_SIZE.0).min(height_px as f32 / DESIGN_SIZE.1);
    fit.clamp(0.75, 4.0)
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
    let started = Instant::now();
    let perf = perf::PerfLog::from_env(started)?;
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
        gamescope: is_gamescope_session(),
        close_notice: false,
        current_scan_revision: 0,
        current_steam_revision: 0,
        perf,
        repaint_at: None,
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
    fn gamescope_layout_fits_the_output_whatever_its_physical_size() {
        use super::fit_pixels_per_point;
        // Steam Deck: the whole 1080x720 layout fits, close to its desktop size.
        let deck = fit_pixels_per_point(1280, 800);
        assert!((deck - 1.111).abs() < 0.001, "{deck}");
        assert!(1280.0 / deck >= 1079.99 && 800.0 / deck >= 719.99);
        // Docked to a TV, the layout grows with the output instead of staying tiny.
        assert_eq!(fit_pixels_per_point(1920, 1080), 1.5);
        assert_eq!(fit_pixels_per_point(3840, 2160), 3.0);
        // Odd outputs stay usable.
        assert_eq!(fit_pixels_per_point(100, 100), 0.75);
    }
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
