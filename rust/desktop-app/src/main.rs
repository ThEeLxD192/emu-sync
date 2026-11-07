use emusync_data_local::ConfigManager;
use emusync_desktop_app::{
    AppEvent, AuthorizedTransferFailure, CloseState, DesktopCoordinator, RuntimeOwner,
    scan_entry_games,
};
use emusync_domain::config::ConfigurationStore;
use emusync_presentation::{
    ActiveDialog, EmuSyncApp,
    actions::UiAction,
    model::{AppStatus, PathTarget},
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
            );
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
            return;
        }
        match self.operations.poll_close() {
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
                            if let Some(selected) = &self.emusync_app.state.selected_entry {
                                self.emusync_app.state.game_items = scan_entry_games(selected);
                            }
                        }
                        AppEvent::EntryGamesScanned { entry_name, games } => {
                            if self
                                .emusync_app
                                .state
                                .selected_entry
                                .as_ref()
                                .is_some_and(|e| e.common().name == entry_name)
                            {
                                self.emusync_app.state.game_items = games;
                            }
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
                        AppEvent::StatusChanged(status) => {
                            self.emusync_app.state.status = status;
                        }
                        AppEvent::Toast(message) => {
                            self.emusync_app.state.toast_message = Some(message);
                        }
                        AppEvent::UpdateStateChanged(update_state) => {
                            self.emusync_app.state.update_state = update_state;
                        }
                        AppEvent::PathPicked { target, path } => {
                            match &mut self.emusync_app.active_dialog {
                                ActiveDialog::Entry(form) => match target {
                                    PathTarget::Executable => form.executable_path = path,
                                    PathTarget::RomsDirectory => form.roms_directory = path,
                                    PathTarget::CoverPath => form.cover_path = path,
                                    PathTarget::SavePath(idx) => {
                                        if let Some(row) = form.saves.rows.get_mut(idx) {
                                            row.path = path;
                                        }
                                    }
                                    _ => {}
                                },
                                ActiveDialog::GameEdit { form, .. } => match target {
                                    PathTarget::CoverPath => form.cover_path = path,
                                    PathTarget::RestoreDestination => {
                                        form.restore_destination_folder = path
                                    }
                                    PathTarget::SavePath(idx) => {
                                        if let Some(row) = form.saves.rows.get_mut(idx) {
                                            row.path = path;
                                        }
                                    }
                                    _ => {}
                                },
                                _ => {}
                            }
                            window.request_redraw();
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
                        if let Some(entry) = entry {
                            self.emusync_app.state.game_items = scan_entry_games(entry);
                        } else {
                            self.emusync_app.state.game_items.clear();
                        }
                        window.request_redraw();
                    }
                    if matches!(action, UiAction::Exit) {
                        self.close_notice = false;
                        self.closing = true;
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let explicit = config_argument(std::env::args_os().skip(1))?;
    let path = emusync_data_local::resolve_config_location(explicit.as_deref())?;
    let configuration = emusync_data_local::ConfigManager::new(&path)?;
    configuration.load()?;
    let event_loop = EventLoop::new()?;

    let (event_tx, event_rx) = channel();

    let mut app = App {
        window: None,
        state: None,
        painter: None,
        emusync_app: EmuSyncApp::new(),
        configuration,
        coordinator: None,
        event_tx,
        event_rx,
        error: None,
        operations: RuntimeOwner::new()?,
        closing: false,
        close_notice: false,
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
