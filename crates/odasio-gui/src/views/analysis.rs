//! Étape 2 : analyse du monde (inventaire des blocs, répartition par qualité).

use eframe::egui::{self, RichText, Sense, Ui, Vec2};
use odasio_core::{ConversionPlan, Inventory, Quality};

use crate::app::OdasioApp;
use crate::task::State;
use crate::theme::{self, Palette};

/// Blocs non-air par qualité, dans l'ordre d'affichage.
pub fn solid_distribution(plan: &ConversionPlan, inventory: &Inventory) -> Vec<(Quality, u64)> {
    let air: u64 = inventory
        .counts
        .iter()
        .filter(|(s, _)| s.is_air())
        .map(|(_, n)| n)
        .sum();
    plan.blocks_by_quality()
        .into_iter()
        .map(|(q, n)| (q, if q == Quality::Exact { n - air } else { n }))
        .collect()
}

/// Barre empilée de la répartition, avec légende.
pub fn quality_bar(ui: &mut Ui, distribution: &[(Quality, u64)]) {
    let p = Palette::of(ui);
    let total: u64 = distribution.iter().map(|(_, n)| n).sum();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 14.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 7.0, p.panel);
    let mut x = rect.left();
    for &(quality, n) in distribution {
        if n == 0 || total == 0 {
            continue;
        }
        // Les parts infimes restent visibles (2 px).
        let w = (rect.width() * n as f32 / total as f32).max(2.0);
        let part =
            egui::Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(w, rect.height()));
        painter.rect_filled(part, 0.0, p.quality(quality));
        x += w;
    }
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        for &(quality, n) in distribution {
            if n == 0 {
                continue;
            }
            // Une seule étiquette insécable : le retour à la ligne se fait entre les
            // éléments, jamais entre une pastille et sa valeur.
            let font = egui::TextStyle::Body.resolve(ui.style());
            let mut job = egui::text::LayoutJob::default();
            let format = |font: egui::FontId, color| egui::TextFormat {
                valign: egui::Align::Center,
                ..egui::TextFormat::simple(font, color)
            };
            job.append(
                "• ",
                0.0,
                format(
                    egui::FontId::proportional(font.size * 1.8),
                    p.quality(quality),
                ),
            );
            job.append(quality.label(), 0.0, format(font.clone(), p.text));
            job.append(
                &format!("  {}", theme::percent(n, total)),
                0.0,
                format(font, p.muted),
            );
            ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
            ui.add_space(10.0);
        }
    });
}

pub fn show(app: &mut OdasioApp, ui: &mut Ui) {
    let p = Palette::of(ui);
    let Some(loaded) = &app.world else {
        return;
    };
    let level = &loaded.world.level;

    ui.heading(&level.name);
    ui.label(
        RichText::new(format!(
            "{} · {}",
            level.version_name.as_deref().unwrap_or("version inconnue"),
            loaded.path.display()
        ))
        .color(p.muted),
    );
    ui.add_space(theme::GAP);

    // Dimension : changer relance l'analyse.
    let mut dimension = loaded.dimension.clone();
    let ids: Vec<String> = loaded
        .world
        .dimensions
        .iter()
        .map(|d| d.id.clone())
        .collect();
    let running = app.scan.as_ref().is_some_and(|t| t.is_running());
    ui.horizontal(|ui| {
        ui.label("Dimension");
        ui.add_enabled_ui(!running, |ui| {
            egui::ComboBox::from_id_salt("dimension")
                .selected_text(&dimension)
                .width(280.0)
                .show_ui(ui, |ui| {
                    for id in &ids {
                        ui.selectable_value(&mut dimension, id.clone(), id);
                    }
                });
        });
    });
    if dimension != loaded.dimension {
        if let Some(world) = &mut app.world {
            world.dimension = dimension;
        }
        app.start_scan(ui.ctx());
        return;
    }
    ui.add_space(theme::GAP);

    let Some(scan) = app.scan.clone() else {
        return;
    };
    scan.with(|state| match state {
        State::Running { progress, message } => {
            theme::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::section(ui, "Analyse en cours", message);
                let bar = egui::ProgressBar::new(progress.unwrap_or(0.0))
                    .show_percentage()
                    .animate(true)
                    .fill(p.accent);
                ui.add(bar);
            });
        }
        State::Failed(error) => {
            theme::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::section(ui, "L'analyse a échoué", "");
                ui.label(RichText::new(error).color(p.danger));
            });
        }
        State::Done(inventory) => {
            summary(ui, inventory);
            ui.add_space(theme::GAP);
            if let Some(plan) = &app.plan {
                theme::card(ui).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    theme::section(
                        ui,
                        "Fidélité de la conversion",
                        "Part des blocs (hors air) selon la qualité de leur équivalent Vintage Story.",
                    );
                    quality_bar(ui, &solid_distribution(plan, inventory));
                });
                ui.add_space(theme::GAP);
            }
            top_blocks(ui, inventory);
        }
    });
}

fn summary(ui: &mut Ui, inventory: &Inventory) {
    let p = Palette::of(ui);
    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 42.0;
            let solid: u64 = inventory
                .counts
                .iter()
                .filter(|(s, _)| !s.is_air())
                .map(|(_, n)| n)
                .sum();
            theme::stat(ui, &theme::thousands(inventory.regions as u64), "régions");
            theme::stat(ui, &theme::thousands(inventory.chunks as u64), "chunks");
            theme::stat(
                ui,
                &theme::thousands(inventory.counts.len() as u64),
                "états de bloc",
            );
            theme::stat(ui, &theme::thousands(solid), "blocs (hors air)");
        });
        let mut notes = Vec::new();
        if inventory.incomplete_chunks > 0 {
            notes.push(format!(
                "{} chunks en bordure de zone, pas entièrement générés, seront ignorés.",
                theme::thousands(inventory.incomplete_chunks as u64)
            ));
        }
        for (version, n) in &inventory.unsupported {
            notes.push(format!(
                "{n} chunks dans un format non pris en charge (DataVersion {version})."
            ));
        }
        if inventory.errors > 0 {
            notes.push(format!("{} chunks illisibles.", inventory.errors));
        }
        if !notes.is_empty() {
            ui.add_space(8.0);
            for note in notes {
                ui.label(RichText::new(format!("•  {note}")).color(p.muted));
            }
        }
    });
}

fn top_blocks(ui: &mut Ui, inventory: &Inventory) {
    let p = Palette::of(ui);
    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        theme::section(ui, "Blocs les plus présents", "");
        let sorted = inventory.sorted();
        let total: u64 = sorted
            .iter()
            .filter(|(s, _)| !s.is_air())
            .map(|(_, n)| n)
            .sum();
        egui::Grid::new("top_blocks")
            .num_columns(3)
            .min_col_width(ui.available_width() / 4.0)
            .spacing([18.0, 6.0])
            .striped(true)
            .show(ui, |ui| {
                for (state, n) in sorted.iter().filter(|(s, _)| !s.is_air()).take(12) {
                    ui.label(RichText::new(state.to_string()).monospace());
                    ui.label(theme::thousands(*n));
                    ui.label(RichText::new(theme::percent(*n, total)).color(p.muted));
                    ui.end_row();
                }
            });
    });
}
