use std::{num::NonZeroU32, sync::Arc};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    state: Option<egui_winit::State>,
    painter: Option<egui_wgpu::winit::Painter>,
    text: String,
    error: Option<String>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| -> Result<_, Box<dyn std::error::Error>> {
            let window = Arc::new(
                event_loop.create_window(Window::default_attributes().with_title("EmuSync"))?,
            );
            let context = egui::Context::default();
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
            Ok((window, state, painter))
        })();
        match result {
            Ok((window, state, painter)) => {
                self.window = Some(window);
                self.state = Some(state);
                self.painter = Some(painter);
            }
            Err(error) => {
                self.error = Some(error.to_string());
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
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
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let (Some(width), Some(height)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                {
                    painter.on_window_resized(egui::ViewportId::ROOT, width, height);
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let context = state.egui_ctx().clone();
                let input = state.take_egui_input(window);
                let mut output = context.run_ui(input, |ui| {
                    ui.heading("EmuSync");
                    ui.label("Rust desktop preview");
                    ui.text_edit_singleline(&mut self.text);
                });
                state.handle_platform_output(window, output.platform_output);
                let primitives = context.tessellate(output.shapes, output.pixels_per_point);
                painter.paint_and_update_textures(
                    egui::ViewportId::ROOT,
                    output.pixels_per_point,
                    [0.08, 0.08, 0.08, 1.0],
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    Ok(())
}
