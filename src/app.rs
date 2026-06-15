//! The egui application: recording control, the editable step list, the
//! library panel, and the playback target/options.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use eframe::egui;

use crate::capture::{CaptureHandle, Captured};
use crate::library;
use crate::model::{Action, Macro, MouseButton, Step};
use crate::replay::{self, PlayMode, PlayOptions};
use crate::winutil::{self, WindowInfo};

pub struct App {
    capture: CaptureHandle,

    // Recording state
    recording: bool,
    capture_moves: bool,
    hotkey_vk: u32,
    record_anchor: Option<Instant>, // time of previous captured event

    // The macro being edited
    current: Macro,
    selected_step: Option<usize>,

    // Library
    library: Vec<Macro>,

    // Playback
    windows: Vec<WindowInfo>,
    target_idx: Option<usize>,
    play_mode: PlayMode,
    speed: f32,
    repeat: u32,
    playback: Option<Arc<AtomicBool>>,

    status: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, capture: CaptureHandle) -> Self {
        // A bit more comfortable default spacing.
        cc.egui_ctx.set_pixels_per_point(1.15);

        capture.set_hotkey(0x78); // F9
        capture.set_capture_moves(false);

        App {
            capture,
            recording: false,
            capture_moves: false,
            hotkey_vk: 0x78,
            record_anchor: None,
            current: Macro::new("Untitled"),
            selected_step: None,
            library: library::load_all(),
            windows: winutil::list_windows(),
            target_idx: None,
            play_mode: PlayMode::Blocking,
            speed: 1.0,
            repeat: 1,
            playback: None,
            status: "Press the record hotkey (F9) or the Record button to start.".into(),
        }
    }

    /// Drain captured events from the hook thread into the current macro.
    fn pump_capture(&mut self) {
        while let Ok(ev) = self.capture.rx.try_recv() {
            match ev.what {
                Captured::ToggleHotkey => self.toggle_recording(ev.at),
                Captured::Input(action) => {
                    if self.recording {
                        let delay = self
                            .record_anchor
                            .map(|prev| ev.at.duration_since(prev).as_millis() as u64)
                            .unwrap_or(0);
                        self.record_anchor = Some(ev.at);
                        self.current.steps.push(Step { delay_ms: delay, action });
                    }
                }
            }
        }
    }

    fn toggle_recording(&mut self, at: Instant) {
        if self.recording {
            // Stop
            self.recording = false;
            self.capture.set_recording(false);
            self.status = format!(
                "Recorded {} step(s), {} ms total.",
                self.current.steps.len(),
                self.current.duration_ms()
            );
        } else {
            // Start fresh
            self.recording = true;
            self.record_anchor = Some(at);
            self.current.steps.clear();
            self.selected_step = None;
            self.capture.set_recording(true);
            self.status = "Recording… press the hotkey again to stop.".into();
        }
    }

    fn start_recording_button(&mut self) {
        // Mirror the hotkey toggle from a button press.
        self.toggle_recording(Instant::now());
    }

    fn play_current(&mut self) {
        if self.current.steps.is_empty() {
            self.status = "Nothing to play — record or load a macro first.".into();
            return;
        }
        let target = self.target_idx.and_then(|i| self.windows.get(i)).map(|w| w.handle());
        if self.play_mode == PlayMode::Background && target.is_none() {
            self.status = "Background playback needs a target window.".into();
            return;
        }
        let opts = PlayOptions {
            mode: self.play_mode,
            target,
            speed: self.speed,
            repeat: self.repeat,
        };
        self.playback = Some(replay::play(self.current.clone(), opts));
        self.status = "Playing…".into();
    }

    fn stop_playback(&mut self) {
        if let Some(flag) = &self.playback {
            flag.store(false, Ordering::SeqCst);
        }
        self.playback = None;
        self.status = "Playback stopped.".into();
    }

    fn save_current(&mut self) {
        match library::save(&self.current) {
            Ok(path) => {
                self.status = format!("Saved to {}", path.display());
                self.library = library::load_all();
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump_capture();

        // Clear finished playback handle.
        if let Some(flag) = &self.playback {
            if !flag.load(Ordering::SeqCst) {
                self.playback = None;
                if self.status == "Playing…" {
                    self.status = "Playback finished.".into();
                }
            }
        }

        self.top_bar(ctx);
        self.library_panel(ctx);
        self.playback_panel(ctx);
        self.step_list(ctx);

        // Keep ticking so capture/playback updates are reflected promptly.
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
}

impl App {
    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("🐭 Mackrey");
                ui.separator();

                let (label, color) = if self.recording {
                    ("⏺ Stop", egui::Color32::from_rgb(220, 60, 60))
                } else {
                    ("⏺ Record", egui::Color32::from_rgb(60, 160, 90))
                };
                if ui
                    .add(egui::Button::new(egui::RichText::new(label).color(egui::Color32::WHITE)).fill(color))
                    .clicked()
                {
                    self.start_recording_button();
                }

                if self.recording {
                    ui.label(egui::RichText::new("● REC").color(egui::Color32::from_rgb(220, 60, 60)));
                }

                ui.separator();
                ui.label("Name:");
                ui.add(egui::TextEdit::singleline(&mut self.current.name).desired_width(140.0));

                ui.separator();
                if ui.checkbox(&mut self.capture_moves, "Record mouse moves").changed() {
                    self.capture.set_capture_moves(self.capture_moves);
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Record hotkey:");
                ui.label(egui::RichText::new(crate::model::vk_name(self.hotkey_vk)).strong());
                ui.label("(F9)");
                ui.separator();
                ui.weak(&self.status);
            });
            ui.add_space(4.0);
        });
    }

    fn library_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("library")
            .resizable(true)
            .default_width(190.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.strong("Library");
                    if ui.small_button("⟳").on_hover_text("Reload from disk").clicked() {
                        self.library = library::load_all();
                    }
                });
                ui.separator();

                if self.library.is_empty() {
                    ui.weak("No saved macros yet.");
                }

                let mut to_load: Option<Macro> = None;
                let mut to_delete: Option<String> = None;

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for mac in &self.library {
                        ui.horizontal(|ui| {
                            if ui
                                .button(&mac.name)
                                .on_hover_text(format!(
                                    "{} steps · {} ms",
                                    mac.steps.len(),
                                    mac.duration_ms()
                                ))
                                .clicked()
                            {
                                to_load = Some(mac.clone());
                            }
                            if ui.small_button("🗑").on_hover_text("Delete").clicked() {
                                to_delete = Some(mac.name.clone());
                            }
                        });
                    }
                });

                if let Some(mac) = to_load {
                    self.current = mac;
                    self.selected_step = None;
                    self.status = format!("Loaded '{}'.", self.current.name);
                }
                if let Some(name) = to_delete {
                    let _ = library::delete(&name);
                    self.library = library::load_all();
                    self.status = format!("Deleted '{name}'.");
                }
            });
    }

    fn playback_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("playback")
            .resizable(true)
            .default_width(250.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.strong("Playback");
                ui.separator();

                ui.label("Mode:");
                ui.radio_value(&mut self.play_mode, PlayMode::Blocking, "Blocking (real input)")
                    .on_hover_text("SendInput — reliable, takes over your keyboard/mouse.");
                ui.radio_value(&mut self.play_mode, PlayMode::Background, "Background (target window)")
                    .on_hover_text("PostMessage — keeps you working; best-effort, may not work with games.");

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Target window:");
                    if ui.small_button("⟳").on_hover_text("Refresh list").clicked() {
                        self.windows = winutil::list_windows();
                        self.target_idx = None;
                    }
                });

                let selected_label = self
                    .target_idx
                    .and_then(|i| self.windows.get(i))
                    .map(|w| ellipsize(&w.title, 34))
                    .unwrap_or_else(|| "(none — foreground)".to_string());

                egui::ComboBox::from_id_salt("target_combo")
                    .selected_text(selected_label)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.target_idx, None, "(none — foreground)");
                        for (i, w) in self.windows.iter().enumerate() {
                            ui.selectable_value(&mut self.target_idx, Some(i), ellipsize(&w.title, 40));
                        }
                    });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Speed:");
                    ui.add(egui::Slider::new(&mut self.speed, 0.25..=5.0).suffix("×"));
                });
                ui.horizontal(|ui| {
                    ui.label("Repeat:");
                    ui.add(egui::DragValue::new(&mut self.repeat).range(1..=9999));
                });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let playing = self.playback.is_some();
                    if ui
                        .add_enabled(!playing, egui::Button::new("▶ Play"))
                        .clicked()
                    {
                        self.play_current();
                    }
                    if ui.add_enabled(playing, egui::Button::new("■ Stop")).clicked() {
                        self.stop_playback();
                    }
                });

                ui.add_space(12.0);
                ui.separator();
                if ui.button("💾 Save to library").clicked() {
                    self.save_current();
                }
            });
    }

    fn step_list(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong(format!("Steps ({})", self.current.steps.len()));
                ui.weak(format!("· total {} ms", self.current.duration_ms()));
                if ui.small_button("Clear all").clicked() {
                    self.current.steps.clear();
                    self.selected_step = None;
                }
            });
            ui.separator();

            let mut to_delete: Option<usize> = None;
            let mut move_up: Option<usize> = None;
            let mut move_down: Option<usize> = None;

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("steps_grid")
                    .num_columns(5)
                    .striped(true)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.strong("#");
                        ui.strong("Delay (ms)");
                        ui.strong("Action");
                        ui.strong("");
                        ui.strong("");
                        ui.end_row();

                        let len = self.current.steps.len();
                        for i in 0..len {
                            let selected = self.selected_step == Some(i);
                            if ui.selectable_label(selected, format!("{}", i + 1)).clicked() {
                                self.selected_step = Some(i);
                            }

                            ui.add(
                                egui::DragValue::new(&mut self.current.steps[i].delay_ms)
                                    .speed(1.0)
                                    .range(0..=600_000),
                            );

                            ui.label(self.current.steps[i].action.describe());

                            ui.horizontal(|ui| {
                                if ui.small_button("↑").clicked() && i > 0 {
                                    move_up = Some(i);
                                }
                                if ui.small_button("↓").clicked() && i + 1 < len {
                                    move_down = Some(i);
                                }
                            });

                            if ui.small_button("🗑").clicked() {
                                to_delete = Some(i);
                            }
                            ui.end_row();
                        }
                    });

                if self.current.steps.is_empty() {
                    ui.add_space(8.0);
                    ui.weak("No steps. Press F9 to record, or load a macro from the library.");
                }
            });

            if let Some(i) = move_up {
                self.current.steps.swap(i - 1, i);
            }
            if let Some(i) = move_down {
                self.current.steps.swap(i, i + 1);
            }
            if let Some(i) = to_delete {
                self.current.steps.remove(i);
                self.selected_step = None;
            }

            // Inline editor for the selected step's action.
            if let Some(i) = self.selected_step {
                if i < self.current.steps.len() {
                    ui.separator();
                    ui.label(egui::RichText::new(format!("Edit step {}", i + 1)).strong());
                    edit_action(ui, &mut self.current.steps[i].action);
                }
            }
        });
    }
}

/// Inline widgets to tweak a single action's fields.
fn edit_action(ui: &mut egui::Ui, action: &mut Action) {
    match action {
        Action::KeyDown { vk, scan } | Action::KeyUp { vk, scan } => {
            ui.horizontal(|ui| {
                ui.label("VK:");
                ui.add(egui::DragValue::new(vk).range(0..=255));
                ui.label("Scan:");
                ui.add(egui::DragValue::new(scan).range(0..=65535));
                ui.weak(crate::model::vk_name(*vk));
            });
        }
        Action::MouseMove { x, y } => {
            ui.horizontal(|ui| {
                ui.label("X:");
                ui.add(egui::DragValue::new(x));
                ui.label("Y:");
                ui.add(egui::DragValue::new(y));
            });
        }
        Action::MouseDown { button, x, y } | Action::MouseUp { button, x, y } => {
            ui.horizontal(|ui| {
                ui.label("Button:");
                egui::ComboBox::from_id_salt("btn")
                    .selected_text(button.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(button, MouseButton::Left, "Left");
                        ui.selectable_value(button, MouseButton::Right, "Right");
                        ui.selectable_value(button, MouseButton::Middle, "Middle");
                        ui.selectable_value(button, MouseButton::X1, "X1");
                        ui.selectable_value(button, MouseButton::X2, "X2");
                    });
                ui.label("X:");
                ui.add(egui::DragValue::new(x));
                ui.label("Y:");
                ui.add(egui::DragValue::new(y));
            });
        }
        Action::Wheel { delta, x, y } => {
            ui.horizontal(|ui| {
                ui.label("Delta:");
                ui.add(egui::DragValue::new(delta));
                ui.label("X:");
                ui.add(egui::DragValue::new(x));
                ui.label("Y:");
                ui.add(egui::DragValue::new(y));
            });
        }
    }
}

fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{head}…")
    }
}
