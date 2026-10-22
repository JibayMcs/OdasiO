//! Parcours Vintage Story vers Minecraft : choix de la sauvegarde, analyse, correspondances,
//! options et conversion.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui::{self, Align, Layout, RichText, Ui};
use egui_extras::Column;
use odasio_core::mapping::Quality;
use odasio_core::mc::writer::Generator;
use odasio_core::paths;
use odasio_core::reverse::ReverseMapping;
use odasio_core::to_minecraft::{ToMcOptions, ToMcProgress, ToMcReport, convert_to_minecraft};
use odasio_core::vs::VsDatabase;
use odasio_core::vs_inventory::VsInventory;

use crate::app::OdasioApp;
use crate::i18n::{self, tr, trf};
use crate::task::{State, Task};
use crate::theme::{self, Palette};
use crate::views::analysis;

#[derive(Clone)]
pub struct SaveCard {
    pub path: PathBuf,
    pub name: String,
    pub world_type: String,
    pub height: u32,
    pub bytes: u64,
}

pub struct LoadedSave {
    pub path: PathBuf,
    pub name: String,
    pub world_type: String,
    pub height: u32,
}

pub struct Form {
    pub world_name: String,
    pub output: PathBuf,
    pub output_custom: bool,
    pub overwrite: bool,
    pub generator: Generator,
    pub creative: bool,
}

pub struct ReverseState {
    pub saves: Option<Vec<SaveCard>>,
    pub save: Option<LoadedSave>,
    pub error: Option<String>,
    pub scan: Option<Task<Arc<VsInventory>>>,
    pub mapping: ReverseMapping,
    pub form: Form,
    pub convert: Option<Task<Arc<ToMcReport>>>,
}

impl Default for ReverseState {
    fn default() -> Self {
        Self {
            saves: None,
            save: None,
            error: None,
            scan: None,
            mapping: ReverseMapping::builtin(),
            form: Form {
                world_name: String::new(),
                output: PathBuf::new(),
                output_custom: false,
                overwrite: false,
                generator: Generator::Ocean,
                creative: true,
            },
            convert: None,
        }
    }
}

impl ReverseState {
    pub fn inventory(&self) -> Option<Arc<VsInventory>> {
        self.scan.as_ref().and_then(Task::take_done)
    }
}

fn minecraft_saves() -> PathBuf {
    paths::minecraft_saves_dir().unwrap_or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".minecraft/saves")
    })
}

/// Taille lisible, en mégaoctets.
fn file_size(bytes: u64) -> String {
    trf(
        "reverse.size_mb",
        &[("n", &format!("{:.0}", bytes as f64 / 1_048_576.0))],
    )
}

/// Ligne de détail d'une carte, formatée à l'affichage pour suivre la langue.
fn save_details(save: &SaveCard) -> String {
    trf(
        "reverse.save_details",
        &[
            ("type", &save.world_type),
            ("height", &save.height),
            ("size", &file_size(save.bytes)),
        ],
    )
}

fn discover() -> Vec<SaveCard> {
    let Some(dir) = paths::vs_saves_dir() else {
        return Vec::new();
    };
    let mut saves: Vec<SaveCard> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "vcdbs"))
        .filter_map(|path| {
            let db = VsDatabase::open(&path).ok()?;
            Some(SaveCard {
                name: db.save.world_name.clone(),
                world_type: db.save.world_type.clone(),
                height: db.save.map_size[1],
                bytes: std::fs::metadata(&path).map_or(0, |m| m.len()),
                path,
            })
        })
        .collect();
    saves.sort_by(|a, b| a.name.cmp(&b.name));
    saves
}

pub fn open_save(app: &mut OdasioApp, ctx: &egui::Context, path: &Path) {
    let state = &mut app.reverse;
    match VsDatabase::open(path) {
        Ok(db) => {
            state.error = None;
            state.form.world_name = theme::converted_name(&db.save.world_name);
            state.form.output_custom = false;
            state.save = Some(LoadedSave {
                path: path.to_path_buf(),
                name: db.save.world_name.clone(),
                world_type: db.save.world_type.clone(),
                height: db.save.map_size[1],
            });
            state.convert = None;
            let path = path.to_path_buf();
            state.scan = Some(Task::spawn(ctx, tr("reverse.scan_start"), move |r| {
                let inventory = VsInventory::scan(&path, |done, total| {
                    r.progress(
                        Some(done as f32 / total.max(1) as f32),
                        trf("reverse.scan", &[("done", &done), ("total", &total)]),
                    )
                })?;
                Ok(Arc::new(inventory))
            }));
            app.step = crate::app::Step::Analysis;
        }
        Err(e) => state.error = Some(format!("{}\n{e}", path.display())),
    }
}

pub fn show_world(app: &mut OdasioApp, ui: &mut Ui) {
    let p = Palette::of(ui);
    if app.reverse.saves.is_none() {
        app.reverse.saves = Some(discover());
    }
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.heading(tr("reverse.title"));
            ui.label(RichText::new(tr("reverse.subtitle")).color(p.muted));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if theme::primary_button(ui, tr("common.browse"), true).clicked()
                && let Some(file) = rfd::FileDialog::new()
                    .add_filter(tr("common.vs_save"), &["vcdbs"])
                    .pick_file()
            {
                open_save(app, ui.ctx(), &file);
            }
        });
    });
    ui.add_space(theme::GAP);

    if let Some(error) = &app.reverse.error {
        ui.label(RichText::new(error).color(p.danger));
        ui.add_space(theme::GAP);
    }

    let saves = app.reverse.saves.clone().unwrap_or_default();
    if saves.is_empty() {
        theme::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label(RichText::new(tr("reverse.empty_title")).size(18.0).strong());
                ui.label(RichText::new(tr("reverse.empty_hint")).color(p.muted));
                ui.add_space(24.0);
            });
        });
        return;
    }

    let width = ui.available_width();
    let columns = ((width + theme::GAP) / (360.0 + theme::GAP))
        .floor()
        .max(1.0) as usize;
    let card_width = (width - theme::GAP * (columns - 1) as f32) / columns as f32;
    let mut chosen = None;
    for row in saves.chunks(columns) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme::GAP;
            for save in row {
                let response = theme::card(ui)
                    .show(ui, |ui| {
                        ui.set_width(card_width - 38.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&save.name).size(16.5).strong());
                            ui.label(RichText::new(save_details(save)).color(p.muted));
                            ui.add(
                                egui::Label::new(
                                    RichText::new(save.path.display().to_string())
                                        .size(12.0)
                                        .color(p.muted),
                                )
                                .truncate(),
                            );
                        });
                    })
                    .response
                    .interact(egui::Sense::click());
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    ui.painter().rect_stroke(
                        response.rect,
                        theme::RADIUS,
                        egui::Stroke::new(1.5_f32, p.accent),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.clicked() {
                    chosen = Some(save.path.clone());
                }
            }
        });
        ui.add_space(theme::GAP - 8.0);
    }
    if let Some(path) = chosen {
        open_save(app, ui.ctx(), &path);
    }
}

fn distribution(
    plan: &std::collections::BTreeMap<
        Quality,
        Vec<(String, u64, odasio_core::reverse::ReverseResolved)>,
    >,
) -> Vec<(Quality, u64)> {
    plan.iter()
        .map(|(q, entries)| (*q, entries.iter().map(|e| e.1).sum()))
        .collect()
}

fn scan_status(app: &OdasioApp, ui: &mut Ui) -> Option<Arc<VsInventory>> {
    let p = Palette::of(ui);
    let scan = app.reverse.scan.clone()?;
    scan.with(|state| match state {
        State::Running { progress, message } => {
            theme::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::section(ui, tr("common.analysis_running"), message);
                ui.add(
                    egui::ProgressBar::new(progress.unwrap_or(0.0))
                        .show_percentage()
                        .fill(p.accent),
                );
            });
            None
        }
        State::Failed(error) => {
            ui.label(RichText::new(error).color(p.danger));
            None
        }
        State::Done(inventory) => Some(inventory.clone()),
    })
}

pub fn show_analysis(app: &mut OdasioApp, ui: &mut Ui) {
    let p = Palette::of(ui);
    let Some(save) = &app.reverse.save else {
        return;
    };
    ui.heading(&save.name);
    ui.label(
        RichText::new(trf(
            "reverse.details",
            &[
                ("type", &save.world_type),
                ("height", &save.height),
                ("path", &save.path.display()),
            ],
        ))
        .color(p.muted),
    );
    ui.add_space(theme::GAP);
    let Some(inventory) = scan_status(app, ui) else {
        return;
    };
    let plan = inventory.plan(&app.reverse.mapping);
    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 42.0;
            theme::stat(
                ui,
                &theme::thousands(inventory.columns as u64),
                tr("reverse.columns"),
            );
            theme::stat(
                ui,
                &theme::thousands(inventory.counts.len() as u64),
                tr("reverse.types"),
            );
            theme::stat(
                ui,
                &theme::thousands(inventory.total_blocks()),
                tr("common.solid_blocks"),
            );
        });
    });
    ui.add_space(theme::GAP);
    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        theme::section(ui, tr("common.fidelity"), tr("reverse.fidelity_hint"));
        analysis::quality_bar(ui, &distribution(&plan));
    });
}

pub fn show_mapping(app: &mut OdasioApp, ui: &mut Ui) {
    let p = Palette::of(ui);
    let Some(inventory) = app.reverse.inventory() else {
        return;
    };
    let plan = inventory.plan(&app.reverse.mapping);
    ui.heading(tr("common.mapping_title"));
    ui.label(RichText::new(tr("reverse.mapping_subtitle")).color(p.muted));
    ui.add_space(theme::GAP);
    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        analysis::quality_bar(ui, &distribution(&plan));
    });
    ui.add_space(theme::GAP);

    for (quality, entries) in plan.iter().rev() {
        theme::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                theme::badge(ui, i18n::quality(*quality), p.quality(*quality));
                let blocks: u64 = entries.iter().map(|e| e.1).sum();
                ui.label(
                    RichText::new(format!(
                        "{} · {}",
                        i18n::plural(entries.len() as u64, "unit.type.one", "unit.type.other"),
                        i18n::plural(blocks, "unit.block.one", "unit.block.other")
                    ))
                    .color(p.muted),
                );
            });
            let id = ui.make_persistent_id(("reverse", *quality));
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                *quality > Quality::Equivalent,
            )
            .show_header(ui, |ui| {
                ui.label(RichText::new(tr("common.block_details")).color(p.muted));
            })
            .body(|ui| {
                egui_extras::TableBuilder::new(ui)
                    .id_salt(("reverse-table", *quality))
                    .striped(true)
                    .vscroll(false)
                    .column(Column::remainder().at_least(160.0).clip(true))
                    .column(Column::auto().at_least(80.0))
                    .column(Column::remainder().at_least(160.0).clip(true))
                    .header(22.0, |mut header| {
                        for title in ["Vintage Story", tr("common.col_blocks"), "Minecraft"] {
                            header.col(|ui| {
                                ui.label(RichText::new(title).size(12.5).color(p.muted));
                            });
                        }
                    })
                    .body(|mut body| {
                        for (code, count, resolved) in entries.iter().take(200) {
                            body.row(26.0, |mut row| {
                                row.col(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(code).monospace().size(12.5),
                                        )
                                        .truncate(),
                                    );
                                });
                                row.col(|ui| {
                                    ui.label(
                                        RichText::new(theme::thousands(*count)).color(p.muted),
                                    );
                                });
                                row.col(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(resolved.state.to_string())
                                                .monospace()
                                                .size(12.5),
                                        )
                                        .truncate(),
                                    );
                                });
                            });
                        }
                    });
            });
        });
        ui.add_space(theme::GAP - 4.0);
    }
}

fn default_output(name: &str) -> PathBuf {
    let folder: String = name
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    minecraft_saves().join(folder.trim())
}

pub fn show_options(app: &mut OdasioApp, ui: &mut Ui) {
    let p = Palette::of(ui);
    let form = &mut app.reverse.form;
    if !form.output_custom {
        form.output = default_output(&form.world_name);
    }
    ui.heading(tr("reverse.options_title"));
    ui.label(RichText::new(tr("reverse.options_subtitle")).color(p.muted));
    ui.add_space(theme::GAP);
    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new("mc-options")
            .num_columns(2)
            .spacing([28.0, 18.0])
            .min_col_width(170.0)
            .show(ui, |ui| {
                ui.label(RichText::new(tr("common.world_name")).strong());
                ui.add(egui::TextEdit::singleline(&mut form.world_name).desired_width(380.0));
                ui.end_row();

                ui.label(RichText::new(tr("reverse.folder")).strong());
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(form.output.display().to_string())
                                .monospace()
                                .size(12.5),
                        )
                        .truncate(),
                    );
                    ui.horizontal(|ui| {
                        if ui.button(tr("common.choose")).clicked()
                            && let Some(dir) = rfd::FileDialog::new().pick_folder()
                        {
                            form.output = dir.join(form.world_name.trim());
                            form.output_custom = true;
                        }
                        if form.output_custom && ui.button(tr("reverse.mc_folder")).clicked() {
                            form.output_custom = false;
                        }
                    });
                    if form.output.exists() {
                        ui.checkbox(&mut form.overwrite, tr("reverse.overwrite"));
                    }
                });
                ui.end_row();

                ui.label(RichText::new(tr("common.terrain")).strong());
                ui.vertical(|ui| {
                    ui.radio_value(&mut form.generator, Generator::Ocean, tr("reverse.ocean"));
                    ui.radio_value(&mut form.generator, Generator::Normal, tr("reverse.normal"));
                    ui.radio_value(&mut form.generator, Generator::Void, tr("reverse.void"));
                });
                ui.end_row();

                ui.label(RichText::new(tr("common.game_mode")).strong());
                ui.horizontal(|ui| {
                    ui.radio_value(&mut form.creative, true, tr("common.creative"));
                    ui.radio_value(&mut form.creative, false, tr("common.survival"));
                });
                ui.end_row();
            });
    });
}

pub fn show_convert(app: &mut OdasioApp, ui: &mut Ui) {
    let p = Palette::of(ui);
    let Some((save_name, save_path)) = app
        .reverse
        .save
        .as_ref()
        .map(|s| (s.name.clone(), s.path.clone()))
    else {
        return;
    };
    let state = &mut app.reverse;
    ui.heading(tr("common.convert_title"));
    ui.add_space(theme::GAP);
    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new("mc-recap")
            .num_columns(2)
            .spacing([24.0, 8.0])
            .show(ui, |ui| {
                ui.label(RichText::new(tr("common.vs_save")).color(p.muted));
                ui.label(RichText::new(&save_name).strong());
                ui.end_row();
                ui.label(RichText::new(tr("common.mc_world")).color(p.muted));
                ui.label(RichText::new(&state.form.world_name).strong());
                ui.end_row();
                ui.label(RichText::new(tr("reverse.folder_short")).color(p.muted));
                ui.add(
                    egui::Label::new(
                        RichText::new(state.form.output.display().to_string()).strong(),
                    )
                    .truncate(),
                );
                ui.end_row();
            });
    });
    ui.add_space(theme::GAP);

    let running = state.convert.as_ref().is_some_and(|t| t.is_running());
    let produced = state
        .convert
        .as_ref()
        .is_some_and(|t| t.with(|s| matches!(s, State::Done(_))));
    let blocked = (!running && state.form.output.exists() && !state.form.overwrite && !produced)
        .then_some(tr("reverse.exists"));
    ui.horizontal(|ui| {
        let label = if state.convert.is_some() && !running {
            tr("common.convert_again")
        } else {
            tr("common.convert_start")
        };
        if theme::primary_button(ui, label, !running && blocked.is_none()).clicked() {
            let save_path = save_path.clone();
            let mapping = state.mapping.clone();
            let opts = ToMcOptions {
                output: state.form.output.clone(),
                overwrite: state.form.overwrite || produced,
                world_name: state.form.world_name.trim().to_string(),
                generator: state.form.generator,
                creative: state.form.creative,
            };
            state.convert = Some(Task::spawn(ui.ctx(), tr("common.preparing"), move |r| {
                let report = convert_to_minecraft(&save_path, &mapping, &opts, |p| match p {
                    ToMcProgress::Converting { done, total } => r.progress(
                        Some(done as f32 / total.max(1) as f32),
                        trf(
                            "common.regions_progress",
                            &[("done", &done), ("total", &total)],
                        ),
                    ),
                    ToMcProgress::Finishing => r.progress(Some(1.0), tr("reverse.finishing")),
                })?;
                Ok(Arc::new(report))
            }));
        }
        if let Some(reason) = blocked {
            ui.label(RichText::new(reason).color(p.warning));
        }
    });
    ui.add_space(theme::GAP);

    let Some(task) = state.convert.clone() else {
        return;
    };
    task.with(|s| match s {
        State::Running { progress, message } => {
            theme::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::section(ui, tr("common.convert_running"), message);
                ui.add(
                    egui::ProgressBar::new(progress.unwrap_or(0.0))
                        .show_percentage()
                        .fill(p.accent),
                );
            });
        }
        State::Failed(error) => {
            ui.label(RichText::new(error).color(p.danger));
        }
        State::Done(report) => {
            theme::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new(tr("common.converted"))
                        .size(19.0)
                        .strong()
                        .color(p.success),
                );
                ui.label(
                    RichText::new(trf(
                        "reverse.done",
                        &[
                            ("secs", &theme::decimal(report.elapsed.as_secs_f64(), 1)),
                            ("name", &state.form.world_name),
                        ],
                    ))
                    .color(p.muted),
                );
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 42.0;
                    theme::stat(
                        ui,
                        &theme::thousands(report.chunks as u64),
                        tr("common.mc_chunks"),
                    );
                    theme::stat(
                        ui,
                        &theme::thousands(report.block_entities as u64),
                        tr("reverse.block_entities"),
                    );
                    theme::stat(
                        ui,
                        &theme::thousands(report.signs as u64),
                        tr("reverse.signs"),
                    );
                });
            });
        }
    });
}
