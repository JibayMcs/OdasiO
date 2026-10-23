use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui::{self, Align, Layout, RichText, Stroke, Ui};
use odasio_core::mc::World;
use odasio_core::{ConversionPlan, ConvertReport, GameMode, Inventory, Mapping, Season, WorldType};

use crate::i18n::{self, Lang, tr, trf};
use crate::recent::Recent;
use crate::task::Task;
use crate::theme::{self, Palette};
use crate::update;
use crate::views;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    World,
    Analysis,
    Mapping,
    Preview,
    Options,
    Convert,
}

impl Step {
    pub const ALL: [Step; 6] = [
        Step::World,
        Step::Analysis,
        Step::Mapping,
        Step::Preview,
        Step::Options,
        Step::Convert,
    ];

    pub fn label(self) -> &'static str {
        tr(match self {
            Step::World => "step.world",
            Step::Analysis => "step.analysis",
            Step::Mapping => "step.mapping",
            Step::Preview => "step.preview",
            Step::Options => "step.options",
            Step::Convert => "step.convert",
        })
    }
}

/// Sens de la conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    ToVintageStory,
    ToMinecraft,
}

pub struct LoadedWorld {
    pub path: PathBuf,
    pub world: World,
    pub dimension: String,
}

/// Réglages de la conversion, édités à l'étape Options.
pub struct ConvertForm {
    pub world_name: String,
    pub output: PathBuf,
    /// L'utilisateur a choisi un fichier : ne plus le dériver du nom.
    pub output_custom: bool,
    pub overwrite: bool,
    pub map_size_y: u32,
    pub y_offset: i32,
    pub world_type: WorldType,
    /// Monde standard ou océan : hauteur et décalage calculés pour aligner les mers.
    pub auto_layout: bool,
    pub season: Season,
    pub game_mode: GameMode,
}

pub struct OdasioApp {
    pub step: Step,
    pub direction: Direction,
    pub reverse: views::reverse::ReverseState,
    pub recent: Recent,
    pub world_cards: Option<Vec<views::world::Card>>,
    pub world: Option<LoadedWorld>,
    pub world_error: Option<String>,
    pub scan: Option<Task<Arc<Inventory>>>,
    pub mapping: Mapping,
    pub plan: Option<ConversionPlan>,
    pub plan_dirty: bool,
    /// Incrémenté à chaque reconstruction du plan : les vues qui dépendent des
    /// correspondances (aperçu) se mettent à jour.
    pub mapping_rev: u64,
    pub mapping_view: views::mapping::ViewState,
    pub preview: views::preview::ViewState,
    /// Zone à convertir (vide : tout le monde), choisie dans l'aperçu.
    pub selection: odasio_core::Selection,
    pub updater: update::Updater,
    /// Accès au GPU (wgpu, partagé avec egui) pour l'aperçu 3D.
    pub render_state: Option<eframe::egui_wgpu::RenderState>,
    window_fitted: bool,
    pub form: ConvertForm,
    pub convert: Option<Task<Arc<ConvertReport>>>,
}

impl OdasioApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::install(&cc.egui_ctx);
        Self {
            step: Step::World,
            direction: Direction::ToVintageStory,
            reverse: Default::default(),
            recent: Recent::load(),
            world_cards: None,
            world: None,
            world_error: None,
            scan: None,
            mapping: Mapping::builtin(),
            plan: None,
            plan_dirty: false,
            mapping_rev: 0,
            mapping_view: Default::default(),
            preview: Default::default(),
            selection: Default::default(),
            updater: {
                let mut updater = update::Updater::default();
                updater.start_auto(&cc.egui_ctx);
                updater
            },
            render_state: cc.wgpu_render_state.clone(),
            window_fitted: false,
            form: ConvertForm {
                world_name: String::new(),
                output: PathBuf::new(),
                output_custom: false,
                overwrite: false,
                map_size_y: 384,
                y_offset: 64,
                world_type: WorldType::Standard,
                auto_layout: true,
                season: Season::Cycle,
                game_mode: GameMode::Creative,
            },
            convert: None,
        }
    }

    /// Au premier affichage : fenêtre à 80 % de l'écran (bornée), centrée.
    fn fit_window(&mut self, ctx: &egui::Context) {
        if self.window_fitted {
            return;
        }
        let Some(monitor) = ctx.input(|i| i.viewport().monitor_size) else {
            return;
        };
        self.window_fitted = true;
        let size = egui::vec2(
            (monitor.x * 0.8).clamp(960.0, 1920.0).min(monitor.x),
            (monitor.y * 0.85).clamp(680.0, 1200.0).min(monitor.y),
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(
            ((monitor - size) / 2.0).to_pos2(),
        ));
    }

    /// Hauteur et décalage effectifs de la conversion.
    pub fn layout(&self) -> (u32, i32) {
        if self.form.world_type.aligns_sea() && self.form.auto_layout {
            odasio_core::standard_layout(self.inventory().and_then(|i| i.y_range))
        } else {
            (self.form.map_size_y, self.form.y_offset)
        }
    }

    pub fn inventory(&self) -> Option<Arc<Inventory>> {
        self.scan.as_ref().and_then(Task::take_done)
    }

    /// Ouvre un monde Minecraft et lance son analyse.
    pub fn open_world(&mut self, ctx: &egui::Context, path: &Path) {
        match World::open(path) {
            Ok(world) => {
                self.recent.push(path);
                let dimension = world
                    .overworld()
                    .or(world.dimensions.first())
                    .map(|d| d.id.clone())
                    .unwrap_or_default();
                self.form.world_name = theme::converted_name(&world.level.name);
                self.form.output_custom = false;
                self.world_error = None;
                self.world = Some(LoadedWorld {
                    path: path.to_path_buf(),
                    world,
                    dimension,
                });
                self.convert = None;
                self.selection = Default::default();
                self.start_scan(ctx);
                views::preview::prefetch(self, ctx);
                self.step = Step::Analysis;
            }
            Err(e) => self.world_error = Some(format!("{}\n{e}", path.display())),
        }
    }

    pub fn start_scan(&mut self, ctx: &egui::Context) {
        let Some(loaded) = &self.world else {
            return;
        };
        let Some(dim) = loaded.world.dimension(&loaded.dimension) else {
            return;
        };
        let files = dim.region_files().unwrap_or_default();
        self.plan = None;
        self.scan = Some(Task::spawn(ctx, tr("analysis.scan_start"), move |r| {
            let total = files.len().max(1);
            let inventory = Inventory::scan(&files, |done| {
                r.progress(
                    Some(done as f32 / total as f32),
                    trf("analysis.scan", &[("done", &done), ("total", &total)]),
                )
            });
            Ok(Arc::new(inventory))
        }));
    }

    /// Recalcule le plan quand l'inventaire ou les choix de l'utilisateur ont changé.
    fn refresh_plan(&mut self) {
        if self.plan.is_some() && !self.plan_dirty {
            return;
        }
        if let Some(inventory) = self.inventory() {
            self.plan = Some(ConversionPlan::build(&self.mapping, &inventory));
            self.plan_dirty = false;
            self.mapping_rev += 1;
        }
    }

    /// Étapes du sens de conversion choisi (l'aperçu n'existe que vers Vintage Story).
    fn steps(&self) -> Vec<Step> {
        Step::ALL
            .into_iter()
            .filter(|s| *s != Step::Preview || self.direction == Direction::ToVintageStory)
            .collect()
    }

    fn step_available(&self, step: Step) -> bool {
        if self.direction == Direction::ToMinecraft {
            return match step {
                Step::World => true,
                Step::Analysis => self.reverse.save.is_some(),
                _ => self.reverse.inventory().is_some(),
            };
        }
        match step {
            Step::World => true,
            Step::Analysis => self.world.is_some(),
            Step::Mapping | Step::Preview | Step::Options | Step::Convert => {
                self.inventory().is_some()
            }
        }
    }

    fn header(&mut self, ui: &mut Ui) {
        let p = Palette::of(ui);
        ui.horizontal(|ui| {
            ui.add(
                egui::Image::from_bytes(
                    "bytes://odasio-icon.png",
                    include_bytes!("../../../packaging/icons/hicolor/64x64/apps/odasio.png"),
                )
                .fit_to_exact_size(egui::vec2(30.0, 30.0)),
            );
            ui.label(RichText::new("OdasiO").size(22.0).strong().color(p.accent));
            let subtitle = match self.direction {
                Direction::ToVintageStory => tr("app.subtitle_to_vs"),
                Direction::ToMinecraft => tr("app.subtitle_to_mc"),
            };
            ui.label(RichText::new(subtitle).color(p.muted));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                egui::widgets::global_theme_preference_switch(ui);
                language_switch(ui);
                ui.add_space(6.0);
                self.updater.version_button(ui);
            });
        });
        ui.add_space(10.0);

        // Étapes : numérotées, cliquables quand elles sont accessibles.
        let steps = self.steps();
        ui.horizontal_wrapped(|ui| {
            for (i, &step) in steps.iter().enumerate() {
                let current = step == self.step;
                let done = step < self.step;
                let available = self.step_available(step);
                let color = if current {
                    p.accent
                } else if done {
                    p.text
                } else {
                    p.muted
                };
                let bullet = if done {
                    "✔".to_string()
                } else {
                    (i + 1).to_string()
                };
                let text = RichText::new(format!("{bullet}  {}", step.label())).color(color);
                let button = egui::Button::new(if current { text.strong() } else { text })
                    .fill(if current {
                        p.accent.gamma_multiply(0.14)
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .stroke(if current {
                        Stroke::new(1.0_f32, p.accent.gamma_multiply(0.6))
                    } else {
                        Stroke::NONE
                    });
                if ui.add_enabled(available, button).clicked() {
                    self.step = step;
                }
                if i + 1 < steps.len() {
                    ui.label(RichText::new("›").color(p.muted));
                }
            }
        });
    }

    fn footer(&mut self, ui: &mut Ui) {
        let steps = self.steps();
        let index = steps.iter().position(|s| *s == self.step).unwrap_or(0);
        ui.horizontal(|ui| {
            if index > 0 && theme::secondary_button(ui, tr("app.back")).clicked() {
                self.step = steps[index - 1];
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(&next) = steps.get(index + 1) {
                    let enabled = self.step_available(next);
                    if theme::primary_button(ui, &format!("{}  ›", next.label()), enabled).clicked()
                    {
                        self.step = next;
                    }
                }
            });
        });
    }
}

impl eframe::App for OdasioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.fit_window(ctx);
        self.refresh_plan();
        let p = Palette::of_ctx(ctx);

        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(24, 14)),
            )
            .show(ctx, |ui| self.header(ui));
        self.updater.banner(ctx);

        egui::TopBottomPanel::bottom("footer")
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(24, 12)),
            )
            .show(ctx, |ui| self.footer(ui));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::symmetric(16, 0)),
            )
            .show(ctx, |ui| {
                // L'aperçu occupe toute la fenêtre et gère lui-même la souris (pas de
                // défilement de page).
                if self.step == Step::Preview && self.direction == Direction::ToVintageStory {
                    ui.add_space(14.0);
                    views::preview::show(self, ui);
                    return;
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(20.0);
                        // Contenu centré, largeur bornée mais qui grandit avec la fenêtre :
                        // lisible sur grand écran, pleine largeur sur petite fenêtre.
                        let full = ui.available_width();
                        let width = full.min(theme::CONTENT_MAX.max(full * 0.72).min(1500.0));
                        let margin = ((full - width) / 2.0).max(0.0);
                        ui.horizontal(|ui| {
                            ui.add_space(margin);
                            ui.vertical(|ui| {
                                ui.set_width(width);
                                if self.step == Step::World {
                                    direction_switch(self, ui);
                                }
                                if self.direction == Direction::ToMinecraft {
                                    match self.step {
                                        Step::World => views::reverse::show_world(self, ui),
                                        Step::Analysis => views::reverse::show_analysis(self, ui),
                                        Step::Mapping => views::reverse::show_mapping(self, ui),
                                        Step::Preview => {}
                                        Step::Options => views::reverse::show_options(self, ui),
                                        Step::Convert => views::reverse::show_convert(self, ui),
                                    }
                                    return;
                                }
                                match self.step {
                                    Step::World => views::world::show(self, ui),
                                    Step::Analysis => views::analysis::show(self, ui),
                                    Step::Mapping => views::mapping::show(self, ui),
                                    Step::Preview => {}
                                    Step::Options => views::options::show(self, ui),
                                    Step::Convert => views::convert::show(self, ui),
                                }
                            });
                        });
                        ui.add_space(24.0);
                    });
            });
    }
}

/// Choix du sens de conversion, en tête de la première étape.
fn direction_switch(app: &mut OdasioApp, ui: &mut Ui) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        for (direction, label) in [
            (Direction::ToVintageStory, "Minecraft  ›  Vintage Story"),
            (Direction::ToMinecraft, "Vintage Story  ›  Minecraft"),
        ] {
            let selected = app.direction == direction;
            let text = RichText::new(label).size(15.5);
            let button = egui::Button::new(if selected {
                text.strong().color(p.accent_text)
            } else {
                text
            })
            .fill(if selected { p.accent } else { p.card })
            .min_size(egui::vec2(240.0, 38.0));
            if ui.add(button).clicked() && !selected {
                app.direction = direction;
                app.step = Step::World;
            }
        }
    });
    ui.add_space(theme::GAP + 4.0);
}

/// Choix compact de la langue, dans l'en-tête à côté du thème.
fn language_switch(ui: &mut Ui) {
    let p = Palette::of(ui);
    ui.add_space(4.0);
    // Disposition de droite à gauche : EN puis FR pour lire « FR EN ».
    ui.spacing_mut().item_spacing.x = 2.0;
    ui.spacing_mut().button_padding = egui::vec2(8.0, 3.0);
    for lang in Lang::ALL.into_iter().rev() {
        let selected = i18n::current() == lang;
        let text = RichText::new(lang.code().to_uppercase()).size(12.5);
        let button = egui::Button::new(if selected {
            text.strong().color(p.accent_text)
        } else {
            text.color(p.muted)
        })
        .fill(if selected {
            p.accent
        } else {
            egui::Color32::TRANSPARENT
        })
        .stroke(Stroke::NONE)
        .min_size(egui::vec2(30.0, 24.0));
        if ui.add(button).on_hover_text(tr("app.language")).clicked() && !selected {
            i18n::set(lang);
        }
    }
}
