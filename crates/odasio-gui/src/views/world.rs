//! Étape 1 : choix du monde Minecraft.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Align, Layout, RichText, Sense, Ui, Vec2};
use odasio_core::mc::LevelInfo;
use odasio_core::paths;

use crate::app::OdasioApp;
use crate::theme::{self, Palette};

#[derive(Clone)]
pub struct Card {
    pub path: PathBuf,
    pub name: String,
    pub version: String,
    pub modded: bool,
    pub icon: Option<PathBuf>,
    pub recent: bool,
}

fn card_for(path: &Path, recent: bool) -> Option<Card> {
    let level = LevelInfo::read(&path.join("level.dat")).ok()?;
    let icon = path.join("icon.png");
    Some(Card {
        path: path.to_path_buf(),
        name: if level.name.is_empty() {
            path.file_name()?.to_string_lossy().into_owned()
        } else {
            level.name.clone()
        },
        version: match &level.version_name {
            Some(v) => format!("{v} · DataVersion {}", level.data_version),
            None => format!("DataVersion {}", level.data_version),
        },
        modded: level.is_modded(),
        icon: icon.is_file().then_some(icon),
        recent,
    })
}

/// Mondes récents puis mondes du lanceur Minecraft, sans doublons.
pub fn discover(recent: &[PathBuf]) -> Vec<Card> {
    let mut cards: Vec<Card> = recent.iter().filter_map(|p| card_for(p, true)).collect();
    if let Some(dir) = paths::minecraft_saves_dir() {
        for path in paths::list_worlds(&dir) {
            if !cards.iter().any(|c| c.path == path) {
                cards.extend(card_for(&path, false));
            }
        }
    }
    cards
}

fn world_card(ui: &mut Ui, card: &Card, width: f32) -> bool {
    let p = Palette::of(ui);
    let response = theme::card(ui)
        .show(ui, |ui| {
            ui.set_width(width - 38.0);
            ui.horizontal(|ui| {
                let size = Vec2::splat(56.0);
                match &card.icon {
                    Some(icon) => {
                        ui.add(
                            egui::Image::new(format!("file://{}", icon.display()))
                                .fit_to_exact_size(size)
                                .corner_radius(8),
                        );
                    }
                    None => {
                        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                        ui.painter()
                            .rect_filled(rect, 8.0, p.accent.gamma_multiply(0.25));
                        let initial = card.name.chars().next().unwrap_or('?').to_uppercase();
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            initial.to_string(),
                            egui::FontId::proportional(24.0),
                            p.accent,
                        );
                    }
                }
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&card.name).size(16.5).strong());
                        // Deux mondes peuvent porter le même nom : le dossier les distingue.
                        let folder = card
                            .path
                            .file_name()
                            .map(|f| f.to_string_lossy().into_owned());
                        if let Some(folder) = folder.filter(|f| *f != card.name) {
                            ui.label(RichText::new(format!("({folder})")).color(p.muted));
                        }
                        if card.recent {
                            theme::badge(ui, "récent", p.accent);
                        }
                        if card.modded {
                            theme::badge(ui, "moddé", p.quality(odasio_core::Quality::Unmapped));
                        }
                    });
                    ui.label(RichText::new(&card.version).color(p.muted));
                    ui.add(
                        egui::Label::new(
                            RichText::new(card.path.display().to_string())
                                .size(12.0)
                                .color(p.muted),
                        )
                        .truncate(),
                    );
                });
            });
        })
        .response
        .interact(Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        ui.painter().rect_stroke(
            response.rect,
            theme::RADIUS,
            egui::Stroke::new(1.5, p.accent),
            egui::StrokeKind::Inside,
        );
    }
    response.clicked()
}

pub fn show(app: &mut OdasioApp, ui: &mut Ui) {
    let p = Palette::of(ui);
    if app.world_cards.is_none() {
        app.world_cards = Some(discover(&app.recent.paths));
    }

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.heading("Choisissez un monde Minecraft");
            ui.label(
                RichText::new(
                    "Java Edition, de la bêta aux dernières versions. Le monde n'est jamais modifié.",
                )
                .color(p.muted),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if theme::primary_button(ui, "Parcourir...", true).clicked()
                && let Some(dir) = rfd::FileDialog::new()
                    .set_title("Dossier du monde Minecraft (contient level.dat)")
                    .pick_folder()
            {
                app.open_world(ui.ctx(), &dir);
                app.world_cards = None;
            }
        });
    });
    ui.add_space(theme::GAP);

    if let Some(error) = &app.world_error {
        egui::Frame::new()
            .fill(p.danger.gamma_multiply(0.15))
            .stroke(egui::Stroke::new(1.0, p.danger))
            .corner_radius(theme::RADIUS)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Ce dossier ne peut pas être ouvert").strong());
                ui.label(error);
            });
        ui.add_space(theme::GAP);
    }

    let cards = app.world_cards.clone().unwrap_or_default();
    if cards.is_empty() {
        theme::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label(RichText::new("Aucun monde détecté").size(18.0).strong());
                ui.label(
                    RichText::new(
                        "Aucun dossier .minecraft/saves trouvé. Utilisez « Parcourir... » pour \
                         sélectionner le dossier d'un monde.",
                    )
                    .color(p.muted),
                );
                ui.add_space(24.0);
            });
        });
        return;
    }

    // Grille responsive : autant de colonnes que la largeur le permet.
    let width = ui.available_width();
    let columns = ((width + theme::GAP) / (360.0 + theme::GAP))
        .floor()
        .max(1.0) as usize;
    let card_width = (width - theme::GAP * (columns - 1) as f32) / columns as f32;
    let mut chosen = None;
    for row in cards.chunks(columns) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme::GAP;
            for card in row {
                if world_card(ui, card, card_width) {
                    chosen = Some(card.path.clone());
                }
            }
        });
        ui.add_space(theme::GAP - 8.0);
    }
    if let Some(path) = chosen {
        app.open_world(ui.ctx(), &path);
        app.world_cards = None;
    }
}
