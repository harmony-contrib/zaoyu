//! Window snapshot sampler, using the sibling hosts' native snapshot API.

use openharmony_ability::WindowSnapshot;
use std::sync::{
    Arc,
    mpsc::{Receiver, Sender, channel},
};
use vectorcraft_ui_egui::screen_color::{Answer, StartSampler};
use winit::platform::ohos::WindowExtOpenHarmony as _;

type Captured = (Sender<Answer>, Result<WindowSnapshot, String>);

pub struct Picker {
    inbox: Receiver<Captured>,
    active: Option<Selection>,
}

struct Selection {
    reply: Sender<Answer>,
    snapshot: WindowSnapshot,
    texture: egui::TextureHandle,
}

pub fn service(window: Option<Arc<winit::window::Window>>, ctx: egui::Context) -> (StartSampler, Picker) {
    let (captured, inbox) = channel();
    let start = Box::new(move || {
        // As in PhotoCraft, resolve the ID when sampling: the bridge may publish it after startup.
        let id = window
            .as_ref()
            .and_then(|window| window.real_window_id())
            .ok_or("No active OpenHarmony window is available for color picking")?;
        let (reply, receiver) = channel();
        let (captured, ctx) = (captured.clone(), ctx.clone());
        std::thread::Builder::new()
            .name("screen-color-picker".into())
            .spawn(move || {
                let snapshot = openharmony_ability::snapshot_window(id).map_err(|e| e.to_string()).and_then(|snapshot| {
                    let pixels = u64::from(snapshot.width)
                        .checked_mul(u64::from(snapshot.height))
                        .filter(|count| *count > 0 && *count <= 64 * 1024 * 1024);
                    if pixels.and_then(|n| n.checked_mul(4)) != Some(snapshot.rgba.len() as u64) {
                        return Err("Screen capture exceeds the memory budget or has invalid pixels".into());
                    }
                    Ok(snapshot)
                });
                // The receiver is gone when the app closes.
                let _ = captured.send((reply, snapshot));
                ctx.request_repaint();
            })
            .map_err(|e| e.to_string())?;
        Ok(receiver)
    });
    (start, Picker { inbox, active: None })
}

impl Picker {
    pub fn show(&mut self, ctx: &egui::Context) {
        for (reply, result) in self.inbox.try_iter() {
            match result {
                Ok(snapshot) => {
                    let size = [snapshot.width as usize, snapshot.height as usize];
                    let image = egui::ColorImage::from_rgba_unmultiplied(size, &snapshot.rgba);
                    let texture = ctx.load_texture("ohos-screen-picker", image, egui::TextureOptions::NEAREST);
                    self.active = Some(Selection { reply, snapshot, texture });
                }
                Err(error) => {
                    // A dismissed sampler has no use for its answer.
                    let _ = reply.send(Err(error));
                }
            }
        }
        let Some(selection) = &self.active else { return };
        let mut answer = None;
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            answer = Some(Ok(None));
        } else {
            let rect = ctx.content_rect();
            egui::Area::new(egui::Id::unique("ohos-screen-picker")).order(egui::Order::Foreground).fixed_pos(rect.min).show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                let response = ui.add(egui::Image::new(&selection.texture).fit_to_exact_size(rect.size()).sense(egui::Sense::click()));
                if response.clicked()
                    && let Some(point) = response.interact_pointer_pos()
                {
                    answer = Some(pixel(&selection.snapshot, response.rect, point));
                }
                response.on_hover_cursor(egui::CursorIcon::Crosshair).on_hover_text("Pick a color; Escape cancels");
            });
        }
        if let Some(answer) = answer
            && let Some(selection) = self.active.take()
        {
            // The command may have been cancelled or its document closed meanwhile.
            let _ = selection.reply.send(answer);
            ctx.request_repaint();
        }
    }
}

fn pixel(snapshot: &WindowSnapshot, rect: egui::Rect, point: egui::Pos2) -> Answer {
    if !rect.is_finite() || rect.width() <= 0.0 || rect.height() <= 0.0 || !point.is_finite() || snapshot.width == 0 || snapshot.height == 0
    {
        return Err("Invalid screen capture coordinates".into());
    }
    super::policy::sample(
        snapshot.width,
        snapshot.height,
        &snapshot.rgba,
        [(point.x - rect.min.x) / rect.width(), (point.y - rect.min.y) / rect.height()],
    )
    .map(Some)
}
