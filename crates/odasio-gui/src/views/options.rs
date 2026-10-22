//! Étape 4 : réglages du monde Vintage Story produit.

use eframe::egui::{self, RichText, Ui};
use odasio_core::paths;

use crate::app::OdasioApp;
use crate::theme::{self, Palette};

/// Sortie par défaut : dossier des sauvegardes du jeu, nommée d'après le monde.
pub fn default_output(name: &str) -> std::path::PathBuf {
    let file: String = name
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    paths::vs_saves_dir()
        .unwrap_or_default()
        .join(format!("{}.vcdbs", file.trim()))
}

fn row(ui: &mut Ui, label: &str, help: &str, add: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui);
    ui.label(RichText::new(label).strong());
    ui.vertical(|ui| {
        add(ui);
        if !help.is_empty() {
            ui.label(RichText::new(help).size(12.5).color(p.muted));
        }
    });
    ui.end_row();
}

pub fn show(app: &mut OdasioApp, ui: &mut Ui) {
    let p = Palette::of(ui);
    ui.heading("Options du monde Vintage Story");
    ui.label(
        RichText::new("Les valeurs proposées conviennent à la plupart des mondes.").color(p.muted),
    );
    ui.add_space(theme::GAP);

    if !app.form.output_custom {
        app.form.output = default_output(&app.form.world_name);
    }

    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new("options")
            .num_columns(2)
            .spacing([28.0, 18.0])
            .min_col_width(170.0)
            .show(ui, |ui| {
                row(ui, "Nom du monde", "Tel qu'il apparaîtra dans la liste des mondes du jeu.", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut app.form.world_name).desired_width(380.0));
                });

                row(ui, "Fichier de sauvegarde", "", |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(RichText::new(app.form.output.display().to_string()).monospace().size(12.5))
                                .truncate(),
                        );
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Choisir...").clicked()
                            && let Some(file) = rfd::FileDialog::new()
                                .add_filter("Sauvegarde Vintage Story", &["vcdbs"])
                                .set_file_name(
                                    app.form.output.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(),
                                )
                                .save_file()
                        {
                            app.form.output = file.with_extension("vcdbs");
                            app.form.output_custom = true;
                        }
                        if app.form.output_custom && ui.button("Dossier du jeu").clicked() {
                            app.form.output_custom = false;
                        }
                    });
                    if app.form.output.exists() {
                        ui.checkbox(&mut app.form.overwrite, "Remplacer la sauvegarde existante");
                    }
                });

                let rocks = app.mapping.settings.safe_rocks.clone();
                let mut rock = app.mapping.settings.rock.clone();
                row(
                    ui,
                    "Roche principale",
                    "Remplace la pierre Minecraft. Les minerais s'y adaptent quand c'est possible.",
                    |ui| {
                        egui::ComboBox::from_id_salt("rock").selected_text(&rock).width(220.0).show_ui(ui, |ui| {
                            for r in &rocks {
                                ui.selectable_value(&mut rock, r.clone(), r);
                            }
                        });
                    },
                );
                if rock != app.mapping.settings.rock && app.mapping.set_rock(&rock).is_ok() {
                    app.plan_dirty = true;
                }

                row(
                    ui,
                    "Hauteur du monde",
                    "384 couvre toute la hauteur d'un monde Minecraft 1.18+ (de -64 à 319).",
                    |ui| {
                        egui::ComboBox::from_id_salt("height")
                            .selected_text(app.form.map_size_y.to_string())
                            .width(120.0)
                            .show_ui(ui, |ui| {
                                for h in [256u32, 320, 384, 448, 512] {
                                    ui.selectable_value(&mut app.form.map_size_y, h, h.to_string());
                                }
                            });
                    },
                );

                row(
                    ui,
                    "Décalage vertical",
                    "Ajouté au Y Minecraft. 64 place le fond d'un monde 1.18+ (Y = -64) au fond du monde VS.",
                    |ui| {
                        ui.add(egui::DragValue::new(&mut app.form.y_offset).range(-256..=256).speed(1));
                    },
                );
            });
    });
}
