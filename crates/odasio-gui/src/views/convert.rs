//! Étape 5 : conversion et rapport.

use std::sync::Arc;

use eframe::egui::{self, RichText, Ui};
use odasio_core::mc::World;
use odasio_core::{ConvertOptions, Progress};

use crate::app::OdasioApp;
use crate::i18n::{tr, trf};
use crate::task::{State, Task};
use crate::theme::{self, Palette};

fn open_in_file_manager(path: &std::path::Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program).arg(path).spawn();
}

/// La sauvegarde à l'emplacement de sortie vient d'être produite par cette session :
/// la reconvertir la remplace sans autre confirmation.
fn produced_here(app: &OdasioApp) -> bool {
    app.convert
        .as_ref()
        .is_some_and(|t| t.with(|s| matches!(s, State::Done(_))))
}

fn start(app: &mut OdasioApp, ctx: &egui::Context) {
    let overwrite = app.form.overwrite || produced_here(app);
    let Some(loaded) = &app.world else {
        return;
    };
    let path = loaded.path.clone();
    let mut opts = ConvertOptions::new(app.form.output.clone());
    opts.dimension = loaded.dimension.clone();
    opts.world_name = app.form.world_name.trim().to_string();
    opts.overwrite = overwrite;
    (opts.map_size_y, opts.y_offset) = app.layout();
    opts.world_type = app.form.world_type;
    opts.season = app.form.season;
    opts.game_mode = app.form.game_mode;
    opts.selection = (!app.selection.is_empty()).then(|| app.selection.clone());
    let mapping = app.mapping.clone();

    app.convert = Some(Task::spawn(ctx, tr("common.preparing"), move |r| {
        let world = World::open(&path)?;
        let report = odasio_core::convert(&world, &mapping, &opts, |progress| match progress {
            Progress::GeneratingTemplate => r.progress(None, tr("convert.template")),
            Progress::Converting { done, total } => r.progress(
                Some(done as f32 / total.max(1) as f32),
                trf(
                    "common.regions_progress",
                    &[("done", &done), ("total", &total)],
                ),
            ),
            Progress::Finishing => r.progress(Some(1.0), tr("convert.finishing")),
        })?;
        Ok(Arc::new(report))
    }));
}

fn summary_row(ui: &mut Ui, label: &str, value: String) {
    let p = Palette::of(ui);
    ui.label(RichText::new(label).color(p.muted));
    ui.add(egui::Label::new(RichText::new(value).strong()).truncate());
    ui.end_row();
}

pub fn show(app: &mut OdasioApp, ui: &mut Ui) {
    let p = Palette::of(ui);
    let Some(loaded) = &app.world else {
        return;
    };
    ui.heading(tr("common.convert_title"));
    ui.add_space(theme::GAP);

    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new("recap")
            .num_columns(2)
            .spacing([24.0, 8.0])
            .show(ui, |ui| {
                summary_row(
                    ui,
                    tr("common.mc_world"),
                    format!("{} ({})", loaded.world.level.name, loaded.dimension),
                );
                summary_row(ui, tr("convert.vs_world"), app.form.world_name.clone());
                summary_row(
                    ui,
                    tr("preview.zone"),
                    crate::views::preview::zone_text(&app.selection),
                );
                summary_row(
                    ui,
                    tr("convert.file"),
                    app.form.output.display().to_string(),
                );
                summary_row(
                    ui,
                    tr("convert.settings"),
                    trf(
                        "convert.settings_value",
                        &[
                            ("rock", &app.mapping.settings.rock),
                            ("height", &app.form.map_size_y),
                            ("offset", &format!("{:+}", app.form.y_offset)),
                        ],
                    ),
                );
                let choices = app.mapping.policy.overrides.len() + app.mapping.policy.rules.len();
                summary_row(ui, tr("convert.choices"), choices.to_string());
            });
    });
    ui.add_space(theme::GAP);

    let running = app.convert.as_ref().is_some_and(|t| t.is_running());
    let blocked = if app.form.world_name.trim().is_empty() {
        Some(tr("convert.no_name"))
    } else if !running && app.form.output.exists() && !app.form.overwrite && !produced_here(app) {
        Some(tr("convert.exists"))
    } else {
        None
    };

    ui.horizontal(|ui| {
        let label = if app.convert.is_some() && !running {
            tr("common.convert_again")
        } else {
            tr("common.convert_start")
        };
        if theme::primary_button(ui, label, !running && blocked.is_none()).clicked() {
            start(app, ui.ctx());
        }
        if let Some(reason) = blocked {
            ui.label(RichText::new(reason).color(p.warning));
        }
    });
    ui.add_space(theme::GAP);

    let Some(task) = app.convert.clone() else {
        return;
    };
    task.with(|state| match state {
        State::Running { progress, message } => {
            theme::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::section(ui, tr("common.convert_running"), message);
                match progress {
                    Some(v) => {
                        ui.add(egui::ProgressBar::new(*v).show_percentage().fill(p.accent));
                    }
                    // Étape sans avancement mesurable : indicateur d'activité.
                    None => {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().size(20.0).color(p.accent));
                            ui.label(RichText::new(tr("convert.wait")).color(p.muted));
                        });
                    }
                }
            });
        }
        State::Failed(error) => {
            theme::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::section(ui, tr("convert.failed"), "");
                ui.label(RichText::new(error).color(p.danger));
            });
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
                        "convert.done",
                        &[
                            ("secs", &theme::decimal(report.elapsed.as_secs_f64(), 1)),
                            ("name", &app.form.world_name),
                        ],
                    ))
                    .color(p.muted),
                );
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 42.0;
                    theme::stat(
                        ui,
                        &theme::thousands(report.mc_chunks as u64),
                        tr("common.mc_chunks"),
                    );
                    theme::stat(
                        ui,
                        &theme::thousands(report.columns as u64),
                        tr("convert.columns"),
                    );
                    theme::stat(
                        ui,
                        &theme::thousands(report.skipped_chunks as u64),
                        tr("convert.skipped"),
                    );
                    theme::stat(
                        ui,
                        &theme::thousands(report.clipped_blocks),
                        tr("convert.clipped"),
                    );
                });
                if !report.missing_codes.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new(tr("convert.missing")).color(p.warning));
                    for code in report.missing_codes.keys() {
                        ui.label(RichText::new(format!("•  {code}")).monospace());
                    }
                }
                ui.add_space(10.0);
                if let Some(dir) = app.form.output.parent()
                    && theme::secondary_button(ui, tr("convert.open_folder")).clicked()
                {
                    open_in_file_manager(dir);
                }
            });
        }
    });
}
