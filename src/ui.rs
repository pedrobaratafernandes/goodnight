//! The window (egui).

use crate::config::{Action, Config, Mode, Options, TimerSettings, Unit};
use crate::curve::Profile;
use crate::engine::{self, Engine, Phase};
use crate::system;
use chrono::Local;
use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke, Vec2, pos2, vec2};
use std::time::Duration;

pub struct GoodnightApp {
    cfg: Config,
    engine: Option<Engine>,
    current_volume: Option<f32>,
    backend: String,
    message: Option<(String, bool)>, // (text, is_error)

    show_options: bool,
    options_draft: Options,
    show_save: bool,
    preset_name: String,
    show_delete: bool,
    delete_index: usize,
    show_about: bool,
    confirm_close: bool,
    allow_close: bool,
}

impl GoodnightApp {
    pub fn new() -> Self {
        system::init_thread();
        let cfg = Config::load();
        let current_volume = system::get_volume().ok();
        Self {
            options_draft: cfg.options.clone(),
            cfg,
            engine: None,
            current_volume,
            backend: system::volume_backend(),
            message: None,
            show_options: false,
            show_save: false,
            preset_name: String::new(),
            show_delete: false,
            delete_index: 0,
            show_about: false,
            confirm_close: false,
            allow_close: false,
        }
    }

    fn running(&self) -> bool {
        self.engine.as_ref().is_some_and(|e| e.is_active())
    }

    fn save_cfg(&mut self) {
        if let Err(e) = self.cfg.save() {
            self.message = Some((e, true));
        }
    }

    fn start(&mut self) {
        self.save_cfg();
        let plan = engine::make_plan(&self.cfg.last, &self.cfg.options, Local::now());
        self.engine = Some(Engine::start(plan));
        self.message = None;
    }

    fn cancel(&mut self) {
        if let Some(mut e) = self.engine.take() {
            e.cancel();
        }
        self.message = Some(("Temporizador cancelado.".into(), false));
        self.current_volume = system::get_volume().ok();
    }
}

fn fmt_hms(d: chrono::Duration) -> String {
    let s = d.num_seconds().max(0);
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

fn fmt_human(d: Duration) -> String {
    let s = d.as_secs();
    match (s / 3600, (s / 60) % 60, s % 60) {
        (0, 0, s) => format!("{s} s"),
        (0, m, 0) => format!("{m} min"),
        (0, m, s) => format!("{m} min {s} s"),
        (h, 0, _) => format!("{h} h"),
        (h, m, _) => format!("{h} h {m} min"),
    }
}

fn unit_combo(ui: &mut egui::Ui, id: &str, unit: &mut Unit) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(unit.label())
        .show_ui(ui, |ui| {
            for u in Unit::ALL {
                ui.selectable_value(unit, u, u.label());
            }
        });
}

fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).strong().size(15.0));
            ui.add_space(4.0);
            add(ui);
        });
    ui.add_space(6.0);
}

/// Small chart: volume over time for the selected profile.
fn profile_chart(ui: &mut egui::Ui, s: &TimerSettings) {
    let (resp, painter) = ui.allocate_painter(vec2(ui.available_width(), 130.0), Sense::hover());
    let r = resp.rect;
    let visuals = ui.visuals();
    let fg = visuals.text_color();
    let weak = visuals.weak_text_color();
    let accent = Color32::from_rgb(250, 200, 90);
    let plot = egui::Rect::from_min_max(
        pos2(r.left() + 36.0, r.top() + 6.0),
        pos2(r.right() - 8.0, r.bottom() - 20.0),
    );

    painter.rect_filled(plot, 4.0, visuals.extreme_bg_color);
    for pct in [0.0, 0.5, 1.0] {
        let y = plot.bottom() - pct * plot.height();
        painter.line_segment(
            [pos2(plot.left(), y), pos2(plot.right(), y)],
            Stroke::new(0.5, weak),
        );
        painter.text(
            pos2(plot.left() - 4.0, y),
            Align2::RIGHT_CENTER,
            format!("{}%", (pct * 100.0) as u32),
            FontId::proportional(10.0),
            weak,
        );
    }
    let minutes = s.fade_unit.to_duration(s.fade_amount).as_secs_f32() / 60.0;
    for (frac, align) in [
        (0.0, Align2::LEFT_TOP),
        (0.5, Align2::CENTER_TOP),
        (1.0, Align2::RIGHT_TOP),
    ] {
        let x = plot.left() + frac * plot.width();
        let m = frac * minutes;
        let label = if (m - m.round()).abs() < 0.05 {
            format!("{:.0} min", m)
        } else {
            format!("{m:.1} min")
        };
        painter.text(
            pos2(x, plot.bottom() + 3.0),
            align,
            label,
            FontId::proportional(10.0),
            weak,
        );
    }

    let (a, b) = (s.start_volume as f32 / 100.0, s.end_volume as f32 / 100.0);
    let pts: Vec<_> = (0..=80)
        .map(|i| {
            let t = i as f32 / 80.0;
            let v = s.profile.volume(a, b, t);
            pos2(
                plot.left() + t * plot.width(),
                plot.bottom() - v * plot.height(),
            )
        })
        .collect();
    painter.add(egui::Shape::line(pts, Stroke::new(2.0, accent)));
    painter.text(
        plot.right_top() + vec2(-4.0, 4.0),
        Align2::RIGHT_TOP,
        "Volume ao longo do tempo",
        FontId::proportional(10.0),
        fg,
    );
}

impl eframe::App for GoodnightApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let running = self.running();

        // Closing the window would kill the timer: ask first.
        if ctx.input(|i| i.viewport().close_requested()) && running && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        if running {
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        // ---------- menu ----------
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("Ficheiro", |ui| {
                    if ui.button("Opções…").clicked() {
                        self.options_draft = self.cfg.options.clone();
                        self.show_options = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Sair").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        ui.close();
                    }
                });
                ui.add_enabled_ui(!running, |ui| {
                    ui.menu_button("Presets", |ui| {
                        if self.cfg.presets.is_empty() {
                            ui.label(RichText::new("(sem presets)").weak());
                        }
                        let mut load = None;
                        for p in &self.cfg.presets {
                            if ui.button(&p.name).clicked() {
                                load = Some(p.clone());
                                ui.close();
                            }
                        }
                        if let Some(p) = load {
                            self.cfg.last = p.settings;
                            self.message = Some((format!("Preset «{}» carregado.", p.name), false));
                        }
                        ui.separator();
                        if ui.button("Guardar preset…").clicked() {
                            self.preset_name = "O meu preset".into();
                            self.show_save = true;
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                !self.cfg.presets.is_empty(),
                                egui::Button::new("Apagar preset…"),
                            )
                            .clicked()
                        {
                            self.delete_index = 0;
                            self.show_delete = true;
                            ui.close();
                        }
                    });
                });
                ui.menu_button("Ajuda", |ui| {
                    if ui.button("Sobre").clicked() {
                        self.show_about = true;
                        ui.close();
                    }
                });
            });
        });

        // ---------- status / start ----------
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.add_space(8.0);
            if let Some(engine) = &self.engine {
                let st = engine.status();
                let now = Local::now();
                match &st.phase {
                    Phase::Waiting | Phase::Fading | Phase::Executing => {
                        let remaining = st.plan.deadline - now;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(st.plan.action.countdown_verb()).size(15.0));
                            ui.label(
                                RichText::new(fmt_hms(remaining))
                                    .monospace()
                                    .size(22.0)
                                    .strong(),
                            );
                        });
                        ui.label(format!(
                            "Hora da ação: {}",
                            st.plan.deadline.format("%H:%M:%S (%d/%m)")
                        ));
                        match (&st.plan.fade, st.fade_start) {
                            (Some(_), Some(fs)) if now >= fs => {
                                let total =
                                    (st.plan.deadline - fs).num_milliseconds().max(1) as f32;
                                let p =
                                    ((now - fs).num_milliseconds() as f32 / total).clamp(0.0, 1.0);
                                ui.add(egui::ProgressBar::new(p).text("A baixar o volume…"));
                            }
                            (Some(_), Some(fs)) => {
                                ui.label(format!(
                                    "A redução de volume começa às {}",
                                    fs.format("%H:%M:%S")
                                ));
                            }
                            _ => {
                                ui.label("Sem redução de volume.");
                            }
                        }
                        if let Some(v) = st.volume {
                            ui.label(format!("Volume atual: {:.0}%", v * 100.0));
                        }
                        if let Some(w) = &st.warning {
                            ui.colored_label(
                                Color32::from_rgb(220, 140, 40),
                                format!("Aviso: {w}"),
                            );
                        }
                        ui.add_space(4.0);
                        if ui
                            .add_sized(
                                [ui.available_width(), 34.0],
                                egui::Button::new(RichText::new("Cancelar").size(16.0)),
                            )
                            .clicked()
                        {
                            self.cancel();
                        }
                    }
                    Phase::Done => {
                        ui.label("Ação executada.");
                    }
                    Phase::Cancelled => {}
                    Phase::Failed(e) => {
                        ui.colored_label(
                            Color32::from_rgb(220, 70, 70),
                            format!("A ação falhou:\n{e}"),
                        );
                    }
                }
            }
            if !self.running() {
                if let Some((m, err)) = &self.message {
                    let c = if *err {
                        Color32::from_rgb(220, 70, 70)
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    ui.colored_label(c, m);
                }
                let label = RichText::new("Iniciar")
                    .size(17.0)
                    .strong()
                    .color(Color32::from_rgb(30, 24, 10));
                let button = egui::Button::new(label).fill(Color32::from_rgb(250, 200, 90));
                if ui.add_sized([ui.available_width(), 38.0], button).clicked() {
                    self.engine = None;
                    self.start();
                }
            }
            ui.add_space(8.0);
        });

        // ---------- settings ----------
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_enabled_ui(!running, |ui| {
                    let s = &mut self.cfg.last;

                    section(ui, "Quando", |ui| {
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut s.mode, Mode::Countdown, "Daqui a");
                            ui.add(egui::DragValue::new(&mut s.amount).range(1..=9999));
                            unit_combo(ui, "unit", &mut s.unit);
                        });
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut s.mode, Mode::AtTime, "Às");
                            ui.add(egui::DragValue::new(&mut s.at_hour).range(0..=23).custom_formatter(|v, _| format!("{v:02}")));
                            ui.label(":");
                            ui.add(egui::DragValue::new(&mut s.at_minute).range(0..=59).custom_formatter(|v, _| format!("{v:02}")));
                            if s.mode == Mode::AtTime {
                                let now = Local::now();
                                let when = engine::next_occurrence(now, s.at_hour, s.at_minute);
                                let day = if when.date_naive() == now.date_naive() { "hoje" } else { "amanhã" };
                                let left = (when - now).to_std().unwrap_or_default();
                                ui.label(RichText::new(format!("({day}, daqui a {})", fmt_human(left))).weak());
                            }
                        });
                    });

                    section(ui, "Ação", |ui| {
                        egui::ComboBox::from_id_salt("action").width(ui.available_width() - 10.0).selected_text(s.action.label()).show_ui(ui, |ui| {
                            for a in Action::ALL {
                                ui.selectable_value(&mut s.action, a, a.label());
                            }
                        });
                    });

                    section(ui, "Redução de volume", |ui| {
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut s.fade_enabled, true, "Ligada");
                            ui.radio_value(&mut s.fade_enabled, false, "Desligada");
                        });
                        ui.add_enabled_ui(s.fade_enabled, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Começar a baixar");
                                ui.add(egui::DragValue::new(&mut s.fade_amount).range(1..=9999));
                                unit_combo(ui, "fade_unit", &mut s.fade_unit);
                                ui.label("antes da ação");
                            });
                            if s.mode == Mode::Countdown && s.fade_unit.to_duration(s.fade_amount) > s.unit.to_duration(s.amount) {
                                ui.label(RichText::new("A redução vai começar logo ao iniciar (é mais longa do que o tempo total).").weak().small());
                            }
                            ui.horizontal(|ui| {
                                ui.label("Perfil:");
                                for p in Profile::ALL {
                                    ui.radio_value(&mut s.profile, p, p.label());
                                }
                            });
                            egui::Grid::new("vols").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
                                ui.label("Volume inicial:");
                                ui.add(egui::Slider::new(&mut s.start_volume, 0..=100).suffix("%"));
                                if ui.button("Atual").on_hover_text("Usar o volume atual do sistema").clicked() {
                                    self.current_volume = system::get_volume().ok();
                                    match self.current_volume {
                                        Some(v) => s.start_volume = (v * 100.0).round() as u8,
                                        None => self.message = Some((format!("Não consegui ler o volume ({})", self.backend), true)),
                                    }
                                }
                                ui.end_row();
                                ui.label("Volume final:");
                                ui.add(egui::Slider::new(&mut s.end_volume, 0..=100).suffix("%"));
                                ui.end_row();
                            });
                            ui.add_space(4.0);
                            profile_chart(ui, s);
                        });
                    });
                });
            });
        });

        // ---------- dialogs ----------
        if self.show_options {
            let mut open = true;
            let mut close = false;
            egui::Window::new("Opções").open(&mut open).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(&ctx, |ui| {
                let o = &mut self.options_draft;
                ui.checkbox(&mut o.force_quit, "Ao desligar, forçar o fecho de todas as aplicações")
                    .on_hover_text("Alterações não guardadas em documentos serão perdidas.");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut o.restore_enabled, "Repor o volume em");
                    ui.add_enabled(o.restore_enabled, egui::Slider::new(&mut o.restore_volume, 0..=100).suffix("%"));
                });
                ui.label(RichText::new("…logo após desligar/suspender/hibernar, para que o computador não fique em silêncio da próxima vez.").weak().small());
                ui.label(RichText::new(format!("Controlo de volume: {}", self.backend)).weak().small());
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        self.cfg.options = o.clone();
                        close = true;
                    }
                    if ui.button("Cancelar").clicked() {
                        close = true;
                    }
                });
            });
            if close {
                self.save_cfg();
            }
            self.show_options = open && !close;
        }

        if self.show_save {
            let mut open = true;
            let mut close = false;
            egui::Window::new("Guardar preset")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(&ctx, |ui| {
                    ui.label("Nome do preset:");
                    let r = ui.text_edit_singleline(&mut self.preset_name);
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.horizontal(|ui| {
                        let name = self.preset_name.trim().to_string();
                        if (ui
                            .add_enabled(!name.is_empty(), egui::Button::new("Guardar"))
                            .clicked()
                            || enter)
                            && !name.is_empty()
                        {
                            self.cfg.upsert_preset(&name, self.cfg.last.clone());
                            self.message = Some((format!("Preset «{name}» guardado."), false));
                            close = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.save_cfg();
            }
            self.show_save = open && !close;
        }

        if self.show_delete {
            let mut open = true;
            let mut close = false;
            egui::Window::new("Apagar preset")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(&ctx, |ui| {
                    ui.label("Escolha o preset a apagar:");
                    let names: Vec<String> =
                        self.cfg.presets.iter().map(|p| p.name.clone()).collect();
                    self.delete_index = self.delete_index.min(names.len().saturating_sub(1));
                    egui::ComboBox::from_id_salt("del")
                        .selected_text(names.get(self.delete_index).cloned().unwrap_or_default())
                        .show_ui(ui, |ui| {
                            for (i, n) in names.iter().enumerate() {
                                ui.selectable_value(&mut self.delete_index, i, n);
                            }
                        });
                    ui.horizontal(|ui| {
                        if ui.button("Apagar").clicked()
                            && self.delete_index < self.cfg.presets.len()
                        {
                            let p = self.cfg.presets.remove(self.delete_index);
                            self.message = Some((format!("Preset «{}» apagado.", p.name), false));
                            close = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.save_cfg();
            }
            self.show_delete = open && !close;
        }

        if self.show_about {
            egui::Window::new("Sobre").open(&mut self.show_about).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(&ctx, |ui| {
                ui.heading("Goodnight");
                ui.label(format!("Versão {}", env!("CARGO_PKG_VERSION")));
                ui.label("Temporizador para adormecer: baixa o volume aos poucos e depois desliga, suspende ou hiberna o computador.");
                ui.label(RichText::new("Reimplementação em Rust inspirada no Goodnight Timer 1.0 (2006).").weak().small());
                if let Some(p) = Config::path() {
                    ui.label(RichText::new(format!("Configuração: {}", p.display())).weak().small());
                }
            });
        }

        if self.confirm_close {
            let modal =
                egui::Modal::new(egui::Id::unique("goodnight_confirm_close")).show(&ctx, |ui| {
                    ui.set_width(320.0);
                    ui.label(RichText::new("O temporizador está a correr").strong());
                    ui.label("Se fechar a janela, o temporizador é cancelado. Prefere minimizar?");
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Minimizar").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                            self.confirm_close = false;
                        }
                        if ui.button("Cancelar temporizador e sair").clicked() {
                            self.cancel();
                            self.allow_close = true;
                            self.confirm_close = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Voltar").clicked() {
                            self.confirm_close = false;
                        }
                    });
                });
            if modal.should_close() {
                self.confirm_close = false;
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.cfg.save();
    }
}
