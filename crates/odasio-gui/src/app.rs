use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui::{self, Align, Layout, RichText, Stroke, Ui};
use odasio_core::mc::World;
use odasio_core::{ConversionPlan, ConvertReport, Inventory, Mapping, WorldType};

use crate::recent::Recent;
use crate::task::Task;
use crate::theme::{self, Palette};
use crate::views;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    World,
    Analysis,
    Mapping,
    Options,
    Convert,
}

impl Step {
    pub const ALL: [Step; 5] = [
        Step::World,
        Step::Analysis,
        Step::Mapping,
        Step::Options,
        Step::Convert,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Step::World => "Monde",
            Step::Analysis => "Analyse",
            Step::Mapping => "Correspondances",
            Step::Options => "Options",
            Step::Convert => "Conversion",
        }
    }
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
    /// Monde standard : hauteur et décalage calculés pour aligner les mers.
    pub auto_layout: bool,
}

pub struct OdasioApp {
    pub step: Step,
    pub recent: Recent,
    pub world_cards: Option<Vec<views::world::Card>>,
    pub world: Option<LoadedWorld>,
    pub world_error: Option<String>,
    pub scan: Option<Task<Arc<Inventory>>>,
    pub mapping: Mapping,
    pub plan: Option<ConversionPlan>,
    pub plan_dirty: bool,
    pub mapping_view: views::mapping::ViewState,
    pub form: ConvertForm,
    pub convert: Option<Task<Arc<ConvertReport>>>,
}

impl OdasioApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::install(&cc.egui_ctx);
        Self {
            step: Step::World,
            recent: Recent::load(),
            world_cards: None,
            world: None,
            world_error: None,
            scan: None,
            mapping: Mapping::builtin(),
            plan: None,
            plan_dirty: false,
            mapping_view: Default::default(),
            form: ConvertForm {
                world_name: String::new(),
                output: PathBuf::new(),
                output_custom: false,
                overwrite: false,
                map_size_y: 384,
                y_offset: 64,
                world_type: WorldType::Standard,
                auto_layout: true,
            },
            convert: None,
        }
    }

    /// Hauteur et décalage effectifs de la conversion.
    pub fn layout(&self) -> (u32, i32) {
        if self.form.world_type == WorldType::Standard && self.form.auto_layout {
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
                self.form.world_name = format!("{} (Odasio)", world.level.name);
                self.form.output_custom = false;
                self.world_error = None;
                self.world = Some(LoadedWorld {
                    path: path.to_path_buf(),
                    world,
                    dimension,
                });
                self.convert = None;
                self.start_scan(ctx);
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
        self.scan = Some(Task::spawn(ctx, "Lecture des régions...", move |r| {
            let total = files.len().max(1);
            let inventory = Inventory::scan(&files, |done| {
                r.progress(
                    Some(done as f32 / total as f32),
                    format!("Lecture des régions : {done} / {total}"),
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
        }
    }

    fn step_available(&self, step: Step) -> bool {
        match step {
            Step::World => true,
            Step::Analysis => self.world.is_some(),
            Step::Mapping | Step::Options | Step::Convert => self.inventory().is_some(),
        }
    }

    fn header(&mut self, ui: &mut Ui) {
        let p = Palette::of(ui);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Odasio").size(22.0).strong().color(p.accent));
            ui.label(RichText::new("Minecraft vers Vintage Story").color(p.muted));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                egui::widgets::global_theme_preference_switch(ui);
            });
        });
        ui.add_space(10.0);

        // Étapes : numérotées, cliquables quand elles sont accessibles.
        ui.horizontal_wrapped(|ui| {
            for (i, step) in Step::ALL.into_iter().enumerate() {
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
                        Stroke::new(1.0, p.accent.gamma_multiply(0.6))
                    } else {
                        Stroke::NONE
                    });
                if ui.add_enabled(available, button).clicked() {
                    self.step = step;
                }
                if i + 1 < Step::ALL.len() {
                    ui.label(RichText::new("›").color(p.muted));
                }
            }
        });
    }

    fn footer(&mut self, ui: &mut Ui) {
        let index = Step::ALL.iter().position(|s| *s == self.step).unwrap_or(0);
        ui.horizontal(|ui| {
            if index > 0 && theme::secondary_button(ui, "‹  Retour").clicked() {
                self.step = Step::ALL[index - 1];
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(&next) = Step::ALL.get(index + 1) {
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
        self.refresh_plan();
        let p = Palette::of_ctx(ctx);

        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(24, 14)),
            )
            .show(ctx, |ui| self.header(ui));

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
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(20.0);
                        // Contenu centré, largeur bornée : lisible sur grand écran,
                        // pleine largeur sur petite fenêtre.
                        let width = ui.available_width().min(theme::CONTENT_MAX);
                        let margin = ((ui.available_width() - width) / 2.0).max(0.0);
                        ui.horizontal(|ui| {
                            ui.add_space(margin);
                            ui.vertical(|ui| {
                                ui.set_width(width);
                                match self.step {
                                    Step::World => views::world::show(self, ui),
                                    Step::Analysis => views::analysis::show(self, ui),
                                    Step::Mapping => views::mapping::show(self, ui),
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
