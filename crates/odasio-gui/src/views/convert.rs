//! Étape 5 : conversion et rapport.

use std::sync::Arc;

use eframe::egui::{self, RichText, Ui};
use odasio_core::mc::World;
use odasio_core::{ConvertOptions, Progress};

use crate::app::OdasioApp;
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
    let mapping = app.mapping.clone();

    app.convert = Some(Task::spawn(ctx, "Préparation...", move |r| {
        let world = World::open(&path)?;
        let report = odasio_core::convert(&world, &mapping, &opts, |progress| match progress {
            Progress::GeneratingTemplate => r.progress(
                None,
                "Création du monde modèle par le serveur Vintage Story...",
            ),
            Progress::Converting { done, total } => r.progress(
                Some(done as f32 / total.max(1) as f32),
                format!("Conversion des régions : {done} / {total}"),
            ),
            Progress::Finishing => r.progress(Some(1.0), "Finalisation de la sauvegarde..."),
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
    ui.heading("Conversion");
    ui.add_space(theme::GAP);

    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new("recap")
            .num_columns(2)
            .spacing([24.0, 8.0])
            .show(ui, |ui| {
                summary_row(
                    ui,
                    "Monde Minecraft",
                    format!("{} ({})", loaded.world.level.name, loaded.dimension),
                );
                summary_row(ui, "Monde Vintage Story", app.form.world_name.clone());
                summary_row(ui, "Fichier", app.form.output.display().to_string());
                summary_row(
                    ui,
                    "Réglages",
                    format!(
                        "roche {}, hauteur {}, décalage {:+}",
                        app.mapping.settings.rock, app.form.map_size_y, app.form.y_offset
                    ),
                );
                let choices = app.mapping.policy.overrides.len() + app.mapping.policy.rules.len();
                summary_row(ui, "Vos choix de correspondance", choices.to_string());
            });
    });
    ui.add_space(theme::GAP);

    let running = app.convert.as_ref().is_some_and(|t| t.is_running());
    let blocked = if app.form.world_name.trim().is_empty() {
        Some("Donnez un nom au monde dans les options.")
    } else if !running && app.form.output.exists() && !app.form.overwrite && !produced_here(app) {
        Some(
            "Une sauvegarde porte déjà ce nom : changez-le ou autorisez le remplacement dans les options.",
        )
    } else {
        None
    };

    ui.horizontal(|ui| {
        let label = if app.convert.is_some() && !running {
            "Convertir à nouveau"
        } else {
            "Convertir"
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
                theme::section(ui, "Conversion en cours", message);
                match progress {
                    Some(v) => {
                        ui.add(egui::ProgressBar::new(*v).show_percentage().fill(p.accent));
                    }
                    // Étape sans avancement mesurable : indicateur d'activité.
                    None => {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().size(20.0).color(p.accent));
                            ui.label(RichText::new("Patientez, cela prend quelques secondes.").color(p.muted));
                        });
                    }
                }
            });
        }
        State::Failed(error) => {
            theme::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::section(ui, "La conversion a échoué", "");
                ui.label(RichText::new(error).color(p.danger));
            });
        }
        State::Done(report) => {
            theme::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("✔  Monde converti").size(19.0).strong().color(p.success));
                ui.label(
                    RichText::new(format!(
                        "En {:.1} s. Ouvrez Vintage Story, puis Solo : le monde « {} » est dans la liste.",
                        report.elapsed.as_secs_f32(),
                        app.form.world_name
                    ))
                    .color(p.muted),
                );
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 42.0;
                    theme::stat(ui, &theme::thousands(report.mc_chunks as u64), "chunks Minecraft");
                    theme::stat(ui, &theme::thousands(report.columns as u64), "colonnes Vintage Story");
                    theme::stat(ui, &theme::thousands(report.skipped_chunks as u64), "chunks ignorés");
                    theme::stat(ui, &theme::thousands(report.clipped_blocks), "blocs hors hauteur");
                });
                if !report.missing_codes.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Blocs inconnus du jeu, remplacés par de l'air :").color(p.warning));
                    for code in report.missing_codes.keys() {
                        ui.label(RichText::new(format!("•  {code}")).monospace());
                    }
                }
                ui.add_space(10.0);
                if let Some(dir) = app.form.output.parent()
                    && theme::secondary_button(ui, "Ouvrir le dossier des sauvegardes").clicked()
                {
                    open_in_file_manager(dir);
                }
            });
        }
    });
}
