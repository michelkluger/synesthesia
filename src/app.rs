use egui::{Color32, RichText};

use crate::cymatics::CymaticsScene;
use crate::theremin::ThereminScene;
use crate::gravity::GravityScene;
use crate::fluiddrum::FluidDrumScene;

#[derive(PartialEq, Clone, Copy)]
enum SceneId { Cymatics, Theremin, Gravity, FluidDrum }

enum ActiveScene {
    Cymatics(CymaticsScene),
    Theremin(ThereminScene),
    Gravity(GravityScene),
    FluidDrum(FluidDrumScene),
}

pub struct SoundArtApp {
    scene:     ActiveScene,
    active_id: SceneId,
}

impl SoundArtApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            scene:     ActiveScene::Cymatics(CymaticsScene::new()),
            active_id: SceneId::Cymatics,
        }
    }

    fn switch_to(&mut self, id: SceneId) {
        if self.active_id == id { return; }
        self.active_id = id;
        // Dropping the old scene stops its audio stream automatically.
        self.scene = match id {
            SceneId::Cymatics  => ActiveScene::Cymatics(CymaticsScene::new()),
            SceneId::Theremin  => ActiveScene::Theremin(ThereminScene::new()),
            SceneId::Gravity   => ActiveScene::Gravity(GravityScene::new()),
            SceneId::FluidDrum => ActiveScene::FluidDrum(FluidDrumScene::new()),
        };
    }
}

impl eframe::App for SoundArtApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // ── Top navigation bar ────────────────────────────────────────────────
        egui::TopBottomPanel::top("nav_bar")
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(8, 8, 18))
                    .inner_margin(egui::Margin::symmetric(14.0, 7.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("◈  Sound Art")
                            .size(17.0)
                            .color(Color32::from_rgb(200, 175, 110))
                            .strong(),
                    );
                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(12.0);

                    let tabs = [
                        (SceneId::Cymatics,  "Cymatics",   Color32::from_rgb(255, 200, 60)),
                        (SceneId::Theremin,  "Theremin",   Color32::from_rgb(80,  200, 255)),
                        (SceneId::Gravity,   "Gravity",    Color32::from_rgb(180, 120, 255)),
                        (SceneId::FluidDrum, "Fluid Drum", Color32::from_rgb(80,  220, 160)),
                    ];
                    for (id, label, color) in tabs {
                        let active = self.active_id == id;
                        let text = RichText::new(label).size(14.0).color(
                            if active { color } else { Color32::from_gray(130) },
                        );
                        if ui.selectable_label(active, text).clicked() {
                            self.switch_to(id);
                        }
                        ui.add_space(4.0);
                    }
                });
            });

        // ── Delegate to active scene ──────────────────────────────────────────
        match &mut self.scene {
            ActiveScene::Cymatics(s)  => s.update(ctx, frame),
            ActiveScene::Theremin(s)  => s.update(ctx, frame),
            ActiveScene::Gravity(s)   => s.update(ctx, frame),
            ActiveScene::FluidDrum(s) => s.update(ctx, frame),
        }
    }
}
