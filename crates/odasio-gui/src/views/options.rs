//! Étape 4 : réglages du monde Vintage Story produit.

use eframe::egui::{self, RichText, Ui};
use odasio_core::{GameMode, Season, WorldType, paths};

use crate::app::OdasioApp;
use crate::i18n::{self, tr, trf};
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
    ui.heading(tr("options.title"));
    ui.label(RichText::new(tr("options.subtitle")).color(p.muted));
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
                row(
                    ui,
                    tr("common.world_name"),
                    tr("options.world_name_hint"),
                    |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut app.form.world_name)
                                .desired_width(380.0),
                        );
                    },
                );

                row(ui, tr("options.save_file"), "", |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(app.form.output.display().to_string())
                                    .monospace()
                                    .size(12.5),
                            )
                            .truncate(),
                        );
                    });
                    ui.horizontal(|ui| {
                        if ui.button(tr("common.choose")).clicked()
                            && let Some(file) = rfd::FileDialog::new()
                                .add_filter(tr("common.vs_save"), &["vcdbs"])
                                .set_file_name(
                                    app.form
                                        .output
                                        .file_name()
                                        .map(|f| f.to_string_lossy().into_owned())
                                        .unwrap_or_default(),
                                )
                                .save_file()
                        {
                            app.form.output = file.with_extension("vcdbs");
                            app.form.output_custom = true;
                        }
                        if app.form.output_custom && ui.button(tr("options.game_folder")).clicked()
                        {
                            app.form.output_custom = false;
                        }
                    });
                    if app.form.output.exists() {
                        ui.checkbox(&mut app.form.overwrite, tr("options.overwrite"));
                    }
                });

                let rocks = app.mapping.settings.safe_rocks.clone();
                let mut rock = app.mapping.settings.rock.clone();
                row(ui, tr("options.rock"), tr("options.rock_hint"), |ui| {
                    egui::ComboBox::from_id_salt("rock")
                        .selected_text(&rock)
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for r in &rocks {
                                ui.selectable_value(&mut rock, r.clone(), r);
                            }
                        });
                });
                if rock != app.mapping.settings.rock && app.mapping.set_rock(&rock).is_ok() {
                    app.plan_dirty = true;
                }

                row(ui, tr("common.terrain"), tr("options.terrain_hint"), |ui| {
                    ui.radio_value(
                        &mut app.form.world_type,
                        WorldType::Standard,
                        tr("options.standard"),
                    );
                    ui.radio_value(
                        &mut app.form.world_type,
                        WorldType::Ocean,
                        tr("options.ocean"),
                    );
                    ui.radio_value(
                        &mut app.form.world_type,
                        WorldType::Superflat,
                        tr("options.superflat"),
                    );
                });

                row(ui, tr("options.season"), tr("options.season_hint"), |ui| {
                    egui::ComboBox::from_id_salt("season")
                        .selected_text(i18n::season(app.form.season))
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for season in Season::ALL {
                                ui.selectable_value(
                                    &mut app.form.season,
                                    season,
                                    i18n::season(season),
                                );
                            }
                        });
                });

                row(ui, tr("common.game_mode"), "", |ui| {
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut app.form.game_mode,
                            GameMode::Creative,
                            tr("common.creative"),
                        );
                        ui.radio_value(
                            &mut app.form.game_mode,
                            GameMode::Survival,
                            tr("common.survival"),
                        );
                    });
                });

                let zone = crate::views::preview::zone_text(&app.selection);
                let mut edit_zone = false;
                let mut clear_zone = false;
                row(ui, tr("preview.zone"), tr("options.zone_hint"), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&zone).color(p.accent));
                        ui.add_space(8.0);
                        if ui.button(tr("options.zone_edit")).clicked() {
                            edit_zone = true;
                        }
                        if !app.selection.is_empty() && ui.button(tr("options.zone_all")).clicked()
                        {
                            clear_zone = true;
                        }
                    });
                });
                if edit_zone {
                    app.step = crate::app::Step::Preview;
                    app.preview.open_zone_editor();
                }
                if clear_zone {
                    app.selection = Default::default();
                }

                let standard = app.form.world_type.aligns_sea();
                if standard {
                    let (height, offset) =
                        odasio_core::standard_layout(app.inventory().and_then(|i| i.y_range));
                    row(ui, tr("options.layout"), tr("options.layout_hint"), |ui| {
                        ui.checkbox(&mut app.form.auto_layout, tr("options.auto"));
                        if app.form.auto_layout {
                            ui.label(
                                RichText::new(trf(
                                    "options.auto_detail",
                                    &[
                                        ("height", &height),
                                        ("offset", &format!("{offset:+}")),
                                        ("sea", &(63 + offset)),
                                    ],
                                ))
                                .color(p.muted),
                            );
                        } else if app.form.map_size_y == 384 && app.form.y_offset == 64 {
                            // Point de départ des réglages manuels : la proposition automatique.
                            app.form.map_size_y = height;
                            app.form.y_offset = offset;
                        }
                    });
                }

                if !standard || !app.form.auto_layout {
                    row(ui, tr("options.height"), tr("options.height_hint"), |ui| {
                        egui::ComboBox::from_id_salt("height")
                            .selected_text(app.form.map_size_y.to_string())
                            .width(120.0)
                            .show_ui(ui, |ui| {
                                for h in [256u32, 320, 384, 448, 512] {
                                    ui.selectable_value(&mut app.form.map_size_y, h, h.to_string());
                                }
                            });
                    });
                    row(ui, tr("options.offset"), tr("options.offset_hint"), |ui| {
                        ui.add(
                            egui::DragValue::new(&mut app.form.y_offset)
                                .range(-256..=256)
                                .speed(1),
                        );
                    });
                }
            });
    });
}
