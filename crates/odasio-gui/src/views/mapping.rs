//! Étape 3 : assistant de correspondances. Seuls les blocs présents dans le monde sont
//! proposés ; les groupes à arbitrer (avec perte, remplaçants, sans équivalent, non mappés)
//! passent en premier.

use eframe::egui::{self, Align, Layout, RichText, Ui};
use egui_extras::Column;
use odasio_core::catalog;
use odasio_core::mapping::{Action, Category, Quality};
use odasio_core::plan::{Group, PlanEntry};

use crate::app::OdasioApp;
use crate::theme::{self, Palette};
use crate::views::analysis;

/// Ce que le sélecteur de bloc VS va modifier.
#[derive(Clone)]
pub enum PickTarget {
    /// Surcharge pour un état précis, ou pour tous les états d'un identifiant.
    Block { state: String, name: String },
    /// Action d'un groupe (qualité, catégorie).
    Group(Quality, Category),
}

#[derive(Clone)]
pub struct Picker {
    pub target: PickTarget,
    pub query: String,
    pub whole_id: bool,
}

#[derive(Default)]
pub struct ViewState {
    pub filter: String,
    pub show_all: bool,
    pub picker: Option<Picker>,
}

fn action_label(action: &Action) -> String {
    match action {
        Action::Keep => "Garder la proposition".into(),
        Action::Air => "Remplacer par de l'air".into(),
        Action::Block(code) => format!("Remplacer par {code}"),
    }
}

fn matches_filter(entry: &PlanEntry, filter: &str) -> bool {
    filter.is_empty()
        || filter.split_whitespace().all(|w| {
            let w = w.to_lowercase();
            entry.state.to_string().to_lowercase().contains(&w) || entry.resolved.block.contains(&w)
        })
}

pub fn show(app: &mut OdasioApp, ui: &mut Ui) {
    let p = Palette::of(ui);
    let (Some(plan), Some(inventory)) = (app.plan.clone(), app.inventory()) else {
        return;
    };

    ui.heading("Correspondances des blocs");
    ui.label(
        RichText::new(
            "Les blocs présents dans votre monde et leur équivalent Vintage Story. \
             Décidez par groupe, ou bloc par bloc : vos choix s'appliquent à la conversion.",
        )
        .color(p.muted),
    );
    ui.add_space(theme::GAP);

    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        analysis::quality_bar(ui, &analysis::solid_distribution(&plan, &inventory));
        ui.add_space(4.0);
        let review = plan.needs_review().count();
        let text = if review == 0 {
            "Tout est tranché : la conversion peut être lancée.".to_string()
        } else if review == 1 {
            "1 état de bloc mérite un coup d'œil.".to_string()
        } else {
            format!("{review} états de bloc méritent un coup d'œil.")
        };
        ui.label(RichText::new(text).color(if review == 0 { p.success } else { p.muted }));
    });
    ui.add_space(theme::GAP);

    let view = &mut app.mapping_view;
    ui.add(
        egui::TextEdit::singleline(&mut view.filter)
            .hint_text("Rechercher un bloc Minecraft ou Vintage Story...")
            .margin(egui::Margin::symmetric(10, 7))
            .desired_width(f32::INFINITY),
    );
    ui.checkbox(
        &mut view.show_all,
        "Afficher aussi les blocs exacts et équivalents",
    );
    ui.add_space(theme::GAP);

    let filter = view.filter.trim().to_lowercase();
    let show_all = view.show_all;
    let mut changed = false;
    let mut open_picker = None;

    for ((quality, category), group) in plan.groups.iter().rev() {
        if *quality <= Quality::Equivalent && !show_all {
            continue;
        }
        let entries: Vec<&PlanEntry> = group
            .entries
            .iter()
            .filter(|e| !e.state.is_air() && matches_filter(e, &filter))
            .collect();
        if entries.is_empty() {
            continue;
        }
        let action = app.mapping.policy.action_for(*quality, *category).clone();
        group_card(
            ui,
            *quality,
            *category,
            group,
            &entries,
            &action,
            &mut |event| match event {
                Event::SetAction(a) => {
                    app.mapping.policy.set_action(*quality, Some(*category), a);
                    changed = true;
                }
                Event::PickGroup => open_picker = Some(PickTarget::Group(*quality, *category)),
                Event::PickBlock(state, name) => {
                    open_picker = Some(PickTarget::Block { state, name })
                }
                Event::ClearOverride(state, name) => {
                    app.mapping.policy.overrides.remove(&state);
                    app.mapping.policy.overrides.remove(&name);
                    changed = true;
                }
            },
        );
        ui.add_space(theme::GAP - 4.0);
    }

    if let Some(target) = open_picker {
        app.mapping_view.picker = Some(Picker {
            target,
            query: String::new(),
            whole_id: true,
        });
    }
    if picker(app, ui.ctx()) {
        changed = true;
    }
    if changed {
        app.plan_dirty = true;
    }
}

enum Event {
    SetAction(Action),
    PickGroup,
    PickBlock(String, String),
    ClearOverride(String, String),
}

fn group_card(
    ui: &mut Ui,
    quality: Quality,
    category: Category,
    group: &Group,
    entries: &[&PlanEntry],
    action: &Action,
    on: &mut dyn FnMut(Event),
) {
    let p = Palette::of(ui);
    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            theme::badge(ui, quality.label(), p.quality(quality));
            ui.label(RichText::new(category.label()).size(16.5).strong());
            ui.label(
                RichText::new(format!(
                    "{} états · {} blocs",
                    entries.len(),
                    theme::thousands(group.blocks)
                ))
                .color(p.muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Les groupes fidèles n'ont pas besoin d'action de groupe.
                if quality <= Quality::Equivalent {
                    return;
                }
                egui::ComboBox::from_id_salt(("action", quality, category))
                    .selected_text(action_label(action))
                    .width(250.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(*action == Action::Keep, action_label(&Action::Keep))
                            .clicked()
                        {
                            on(Event::SetAction(Action::Keep));
                        }
                        if ui
                            .selectable_label(*action == Action::Air, action_label(&Action::Air))
                            .clicked()
                        {
                            on(Event::SetAction(Action::Air));
                        }
                        if ui
                            .selectable_label(
                                matches!(action, Action::Block(_)),
                                "Remplacer par un bloc...",
                            )
                            .clicked()
                        {
                            on(Event::PickGroup);
                        }
                    });
            });
        });

        let id = ui.make_persistent_id(("group", quality, category));
        egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            id,
            quality > Quality::Equivalent,
        )
        .show_header(ui, |ui| {
            ui.label(RichText::new("Détail des blocs").color(p.muted));
        })
        .body(|ui| {
            let row_height = 30.0;
            egui_extras::TableBuilder::new(ui)
                .id_salt(("entries", quality, category))
                .striped(true)
                .vscroll(false)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::remainder().at_least(160.0).clip(true))
                .column(Column::auto().at_least(70.0))
                .column(Column::remainder().at_least(160.0).clip(true))
                .column(Column::auto().at_least(150.0))
                .header(22.0, |mut header| {
                    for title in ["Minecraft", "Blocs", "Vintage Story", ""] {
                        header.col(|ui| {
                            ui.label(RichText::new(title).size(12.5).color(p.muted));
                        });
                    }
                })
                .body(|mut body| {
                    for entry in entries.iter().take(200) {
                        body.row(row_height, |mut row| {
                            let state = entry.state.to_string();
                            row.col(|ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(&state).monospace().size(12.5))
                                        .truncate(),
                                )
                                .on_hover_text(&state);
                            });
                            row.col(|ui| {
                                ui.label(
                                    RichText::new(theme::thousands(entry.count)).color(p.muted),
                                );
                            });
                            row.col(|ui| {
                                let result = RichText::new(entry.resolved.to_string())
                                    .monospace()
                                    .size(12.5);
                                ui.add(
                                    egui::Label::new(if entry.resolved.policy_applied {
                                        result.color(p.accent)
                                    } else {
                                        result
                                    })
                                    .truncate(),
                                );
                            });
                            row.col(|ui| {
                                let name = entry.state.name.clone();
                                if ui.small_button("Changer").clicked() {
                                    on(Event::PickBlock(state.clone(), name.clone()));
                                }
                                if entry.resolved.source == odasio_core::mapping::Source::Override
                                    && ui.small_button("Annuler").clicked()
                                {
                                    on(Event::ClearOverride(state, name));
                                }
                            });
                        });
                    }
                });
            if entries.len() > 200 {
                ui.label(
                    RichText::new(format!(
                        "... et {} autres, affinez la recherche.",
                        entries.len() - 200
                    ))
                    .color(p.muted),
                );
            }
        });
    });
}

/// Fenêtre de choix d'un bloc VS. Renvoie vrai si un choix a été appliqué.
fn picker(app: &mut OdasioApp, ctx: &egui::Context) -> bool {
    let Some(mut state) = app.mapping_view.picker.clone() else {
        return false;
    };
    let mut applied = None;
    let mut close = false;

    let p = Palette::of_ctx(ctx);
    let frame = egui::Frame::new()
        .fill(p.card)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(theme::RADIUS)
        .inner_margin(22)
        .shadow(ctx.style().visuals.popup_shadow);
    let modal = egui::Modal::new(egui::Id::new("vs_picker"))
        .frame(frame)
        .show(ctx, |ui| {
            let p = Palette::of(ui);
            ui.set_width(560.0f32.min(ctx.screen_rect().width() - 80.0));
            let title = match &state.target {
                PickTarget::Block { state: s, .. } => format!("Bloc Vintage Story pour {s}"),
                PickTarget::Group(q, c) => format!("Remplacer « {} / {} »", q.label(), c.label()),
            };
            ui.label(RichText::new(title).size(17.0).strong());
            ui.add_space(6.0);
            if let PickTarget::Block { name, .. } = &state.target {
                ui.checkbox(
                    &mut state.whole_id,
                    format!("Appliquer à tous les états de {name}"),
                );
            }
            let search = ui.add(
                egui::TextEdit::singleline(&mut state.query)
                    .hint_text("Rechercher : rock granite, planks oak, glass...")
                    .desired_width(f32::INFINITY),
            );
            search.request_focus();
            ui.add_space(4.0);
            let hits = catalog::search(&state.query, 300);
            ui.label(
                RichText::new(format!("{} résultats", hits.len()))
                    .size(12.0)
                    .color(p.muted),
            );
            egui::ScrollArea::vertical()
                .max_height(340.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if ui
                        .selectable_label(
                            false,
                            RichText::new("game:air  (supprimer le bloc)").monospace(),
                        )
                        .clicked()
                    {
                        applied = Some("game:air".to_string());
                    }
                    for code in hits {
                        if ui
                            .selectable_label(false, RichText::new(code).monospace())
                            .clicked()
                        {
                            applied = Some(code.to_string());
                        }
                    }
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if theme::secondary_button(ui, "Annuler").clicked() {
                    close = true;
                }
            });
        });
    if modal.should_close() {
        close = true;
    }

    let mut changed = false;
    if let Some(code) = applied {
        match &state.target {
            PickTarget::Block { state: s, name } => {
                let key = if state.whole_id {
                    name.clone()
                } else {
                    s.clone()
                };
                app.mapping.policy.overrides.insert(key, code);
            }
            PickTarget::Group(q, c) => {
                let action = if code == "game:air" {
                    Action::Air
                } else {
                    Action::Block(code)
                };
                app.mapping.policy.set_action(*q, Some(*c), action);
            }
        }
        changed = true;
        close = true;
    }
    app.mapping_view.picker = if close { None } else { Some(state) };
    changed
}
