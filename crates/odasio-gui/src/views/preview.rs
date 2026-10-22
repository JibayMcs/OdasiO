//! Étape 4 : aperçu isométrique d'une zone du monde, avant (Minecraft) et après conversion
//! (Vintage Story), côte à côte. Textures et formes viennent des jeux installés. Un clic sur
//! un bloc permet de le remapper ; la vue Vintage Story suit en direct.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, Color32, RichText, Sense, Stroke, TextureHandle, Ui, Vec2};
use odasio_core::mapping::Source;
use odasio_render::BlockLook;
use odasio_render::area::Area;
use odasio_render::iso::{self, Image, Scene};
use odasio_render::mc_assets::{McAssets, detect_client_jar};
use odasio_render::vs_assets::{VsAssets, detect_vs_assets};

use crate::app::OdasioApp;
use crate::i18n::{tr, trf};
use crate::task::{State, Task};
use crate::theme::{self, Palette};
use crate::views::mapping::{self, PickTarget, Picker};

const SIZES: [i32; 5] = [32, 48, 64, 96, 128];
/// Blocs affichés sous la surface la plus basse de la zone.
const DEPTH: i32 = 12;

/// Aspects des blocs lus dans les jeux installés (absents si le jeu ne l'est pas).
pub struct Assets {
    mc: Option<McAssets>,
    vs: Option<VsAssets>,
}

struct Side {
    image: Image,
    texture: TextureHandle,
}

struct Rendered {
    /// Zone, rotation et version des correspondances de ce rendu.
    area: usize,
    rotation: u8,
    mapping_rev: u64,
    /// Demi-largeur d'un bloc dans l'image rendue.
    a: f32,
    scene: Scene,
    mc: Option<Side>,
    vs: Option<Side>,
}

pub struct ViewState {
    assets: Option<Task<Arc<Assets>>>,
    area: Option<Task<Arc<Area>>>,
    /// Monde pour lequel le centre a été initialisé (point d'apparition).
    world: Option<PathBuf>,
    pub center: [i32; 2],
    pub size: i32,
    /// Zone (centre, taille) du dernier chargement demandé.
    requested: Option<([i32; 2], i32)>,
    rotation: u8,
    zoom: f32,
    pan: Vec2,
    rendered: Option<Rendered>,
    /// Bloc choisi et bloc survolé (image précédente), en coordonnées de la zone.
    selected: Option<(usize, usize, usize)>,
    hovered: Option<(usize, usize, usize)>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            assets: None,
            area: None,
            world: None,
            center: [0, 0],
            size: 48,
            requested: None,
            rotation: 0,
            zoom: 1.0,
            pan: Vec2::ZERO,
            rendered: None,
            selected: None,
            hovered: None,
        }
    }
}

fn spawn_of(app: &OdasioApp) -> [i32; 2] {
    app.world
        .as_ref()
        .and_then(|w| w.world.level.spawn)
        .map_or([0, 0], |s| [s[0], s[2]])
}

pub fn show(app: &mut OdasioApp, ui: &mut Ui) {
    let p = Palette::of(ui);
    let Some(loaded) = &app.world else {
        return;
    };
    let path = loaded.path.clone();
    let dim = loaded.world.dimension(&loaded.dimension).cloned();

    if app.preview.world.as_ref() != Some(&path) {
        let center = spawn_of(app);
        app.preview = ViewState {
            assets: app.preview.assets.take(),
            world: Some(path),
            center,
            ..Default::default()
        };
    }
    let ctx = ui.ctx().clone();
    let view = &mut app.preview;
    if view.assets.is_none() {
        view.assets = Some(Task::spawn(&ctx, tr("preview.loading_assets"), |_| {
            let mc = detect_client_jar().and_then(|jar| McAssets::open(&jar).ok());
            let vs = detect_vs_assets().and_then(|root| VsAssets::open(&root).ok());
            Ok(Arc::new(Assets { mc, vs }))
        }));
    }
    if view.requested.is_none() {
        view.requested = Some((view.center, view.size));
    }
    if let (Some(dim), Some((center, size))) = (&dim, view.requested)
        && view.area.is_none()
    {
        let dim = dim.clone();
        view.selected = None;
        view.area = Some(Task::spawn(&ctx, tr("preview.loading_area"), move |_| {
            let area = Area::load(&dim, center[0], center[1], size, DEPTH)?
                .ok_or_else(|| anyhow::anyhow!(tr("preview.empty_area")))?;
            Ok(Arc::new(area))
        }));
    }

    ui.heading(tr("preview.title"));
    ui.label(RichText::new(tr("preview.subtitle")).color(p.muted));
    ui.add_space(theme::GAP);
    controls(app, ui);
    ui.add_space(theme::GAP);

    let view = &app.preview;
    let assets = view.assets.as_ref().and_then(Task::take_done);
    let area = view.area.as_ref().and_then(Task::take_done);
    let (Some(assets), Some(area)) = (assets, area) else {
        // Chargement en cours ou échec : on affiche l'état de la tâche qui bloque.
        let task_state = |ui: &mut Ui, message: Option<(String, bool)>| {
            if let Some((message, failed)) = message {
                theme::card(ui).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if failed {
                        ui.label(RichText::new(message).color(p.danger));
                    } else {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new(message).color(p.muted));
                        });
                    }
                });
            }
        };
        let message = view
            .assets
            .as_ref()
            .and_then(|t| t.with(describe))
            .or_else(|| view.area.as_ref().and_then(|t| t.with(describe)));
        task_state(ui, message);
        return;
    };

    refresh_render(app, ui.ctx(), &assets, &area);
    viewports(app, ui, &area);
    ui.add_space(theme::GAP);
    selection(app, ui, &area);

    if mapping::picker(app, ui.ctx()) {
        app.plan_dirty = true;
    }
}

fn controls(app: &mut OdasioApp, ui: &mut Ui) {
    let p = Palette::of(ui);
    let spawn = spawn_of(app);
    let view = &mut app.preview;
    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.label("X");
            ui.add(egui::DragValue::new(&mut view.center[0]).speed(4));
            ui.label("Z");
            ui.add(egui::DragValue::new(&mut view.center[1]).speed(4));
            if ui
                .small_button(tr("preview.spawn"))
                .on_hover_text(tr("preview.spawn_hint"))
                .clicked()
            {
                view.center = spawn;
            }
            ui.add_space(10.0);
            ui.label(tr("preview.size"));
            egui::ComboBox::from_id_salt("preview_size")
                .selected_text(format!("{0} × {0}", view.size))
                .width(110.0)
                .show_ui(ui, |ui| {
                    for size in SIZES {
                        ui.selectable_value(&mut view.size, size, format!("{size} × {size}"));
                    }
                });
            let pending = view.requested != Some((view.center, view.size));
            let loading = view.area.as_ref().is_some_and(Task::is_running);
            if theme::primary_button(ui, tr("preview.show"), pending && !loading).clicked() {
                view.requested = Some((view.center, view.size));
                view.area = None;
                view.pan = Vec2::ZERO;
            }
            ui.add_space(10.0);
            if ui.button(tr("preview.rotate_left")).clicked() {
                view.rotation = (view.rotation + 3) % 4;
            }
            if ui.button(tr("preview.rotate_right")).clicked() {
                view.rotation = (view.rotation + 1) % 4;
            }
            ui.add_space(10.0);
            ui.label(tr("preview.zoom"));
            ui.add(
                egui::Slider::new(&mut view.zoom, 0.5..=6.0)
                    .logarithmic(true)
                    .show_value(false),
            );
            if ui.small_button(tr("preview.reset_view")).clicked() {
                view.zoom = 1.0;
                view.pan = Vec2::ZERO;
            }
        });
        ui.add_space(4.0);
        ui.label(RichText::new(tr("preview.hint")).size(12.5).color(p.muted));
    });
}

/// Refait les images quand la zone, la rotation ou les correspondances ont changé.
fn refresh_render(app: &mut OdasioApp, ctx: &egui::Context, assets: &Assets, area: &Arc<Area>) {
    let key = (Arc::as_ptr(area) as usize, app.preview.rotation);
    let rev = app.mapping_rev;
    let current = app.preview.rendered.as_ref();
    let same_view = current.is_some_and(|r| (r.area, r.rotation) == key);
    if same_view && current.is_some_and(|r| r.mapping_rev == rev) {
        return;
    }
    // Échelle : les grandes zones sont rendues plus petites pour rester rapides.
    let a = match area.scene.size_x {
        0..=64 => 8.0,
        65..=96 => 6.0,
        _ => 4.0,
    };
    let scene = area.scene.rotated(key.1);
    let upload = |name: &str, image: Image| -> Side {
        let color =
            egui::ColorImage::from_rgba_unmultiplied([image.width, image.height], &image.rgba);
        let texture = ctx.load_texture(name, color, egui::TextureOptions::NEAREST);
        Side { image, texture }
    };

    let mc = if same_view {
        app.preview.rendered.take().and_then(|r| r.mc)
    } else {
        assets.mc.as_ref().map(|mc| {
            let looks: Vec<Option<BlockLook>> = area.palette.iter().map(|s| mc.look(s)).collect();
            upload("preview_mc", iso::render(&scene, &looks, a))
        })
    };
    let vs = assets.vs.as_ref().map(|vs| {
        let looks: Vec<Option<BlockLook>> = area
            .palette
            .iter()
            .map(|state| {
                if state.is_air() {
                    return None;
                }
                let resolved = app.mapping.resolve(state);
                match &resolved.fluid {
                    Some(fluid) if resolved.block == "game:air" => vs.look(fluid),
                    _ => vs.look(&resolved.block),
                }
            })
            .collect();
        upload("preview_vs", iso::render(&scene, &looks, a))
    });
    app.preview.rendered = Some(Rendered {
        area: key.0,
        rotation: key.1,
        mapping_rev: rev,
        a,
        scene,
        mc,
        vs,
    });
}

/// Message d'une tâche en cours ou en échec (`true` si échec).
fn describe<T>(state: &State<T>) -> Option<(String, bool)> {
    match state {
        State::Running { message, .. } => Some((message.clone(), false)),
        State::Failed(e) => Some((e.clone(), true)),
        State::Done(_) => None,
    }
}

/// Les deux vues, côte à côte (empilées sur une fenêtre étroite), avec déplacement et zoom
/// partagés.
fn viewports(app: &mut OdasioApp, ui: &mut Ui, area: &Area) {
    let p = Palette::of(ui);
    let Some(rendered) = &app.preview.rendered else {
        return;
    };
    let view = &app.preview;
    let stacked = ui.available_width() < 760.0;
    let gap = theme::GAP;
    let width = if stacked {
        ui.available_width()
    } else {
        (ui.available_width() - gap) / 2.0
    };

    // Les vues tiennent dans la fenêtre, sous les réglages.
    let view_height = (ui.ctx().screen_rect().height() - 400.0).clamp(320.0, 760.0);
    let mut hovered: Option<(usize, usize, usize)> = None;
    let mut clicked = false;
    let mut drag = Vec2::ZERO;
    let mut zoom = 1.0;
    let mut viewport = |ui: &mut Ui, title: &str, side: Option<&Side>, missing: &str| {
        ui.vertical(|ui| {
            ui.set_width(width);
            ui.label(RichText::new(title).strong());
            ui.add_space(4.0);
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(width, view_height), Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, theme::RADIUS, p.panel);
            let Some(side) = side else {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    missing,
                    egui::FontId::proportional(14.0),
                    p.muted,
                );
                return;
            };
            let size = Vec2::new(side.image.width as f32, side.image.height as f32);
            let fit = (rect.width() / size.x).min(rect.height() / size.y) * 0.95;
            let scale = fit * view.zoom;
            let image_rect = egui::Rect::from_center_size(rect.center() + view.pan, size * scale);
            painter.image(
                side.texture.id(),
                image_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            if response.dragged() {
                drag += response.drag_delta();
            }
            if response.hovered() {
                let z = ui.input(|i| i.zoom_delta());
                if z != 1.0 {
                    zoom = z;
                }
            }
            if let Some(pos) = response.hover_pos() {
                let local = (pos - image_rect.min) / scale;
                if local.x >= 0.0
                    && local.y >= 0.0
                    && let Some(cell) = side.image.picked(local.x as usize, local.y as usize)
                {
                    let (x, y, z) = rendered.scene.position(cell);
                    let (ox, oz) = area.scene.unrotate(view.rotation, x, z);
                    hovered = Some((ox, y, oz));
                    if response.clicked() {
                        clicked = true;
                    }
                }
            }
            // Surlignage du bloc survolé et du bloc choisi, sur les deux vues.
            let outline = |cell: (usize, usize, usize), stroke: Stroke| {
                let (rx, rz) = area.scene.rotate_xz(view.rotation, cell.0, cell.2);
                let points = iso::block_outline(&rendered.scene, rendered.a, rx, cell.1, rz)
                    .map(|[x, y]| image_rect.min + Vec2::new(x, y) * scale);
                painter.add(egui::Shape::closed_line(points.to_vec(), stroke));
            };
            if let Some(cell) = view.selected {
                outline(cell, Stroke::new(2.0, p.accent));
            }
            if let Some(cell) = hovered.or(view.hovered) {
                outline(cell, Stroke::new(1.5, Color32::from_white_alpha(200)));
            }
        });
    };

    let mc_title = "Minecraft";
    let vs_title = "Vintage Story";
    if stacked {
        viewport(ui, mc_title, rendered.mc.as_ref(), tr("preview.no_mc"));
        ui.add_space(gap);
        viewport(ui, vs_title, rendered.vs.as_ref(), tr("preview.no_vs"));
    } else {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            viewport(ui, mc_title, rendered.mc.as_ref(), tr("preview.no_mc"));
            viewport(ui, vs_title, rendered.vs.as_ref(), tr("preview.no_vs"));
        });
    }

    // Le survol se calcule pendant le dessin : on le retient pour la prochaine image afin
    // que les deux vues le surlignent.
    let state = &mut app.preview;
    state.pan += drag;
    if zoom != 1.0 {
        state.zoom = (state.zoom * zoom).clamp(0.5, 6.0);
    }
    if clicked {
        state.selected = hovered;
    }
    state.hovered = hovered;
    if let Some(cell) = hovered {
        if let Some(state) = area
            .palette
            .get(area.scene.get(cell.0, cell.1, cell.2) as usize)
        {
            let resolved = app.mapping.resolve(state);
            egui::Tooltip::always_open(
                ui.ctx().clone(),
                ui.layer_id(),
                egui::Id::new("preview_tip"),
                egui::PopupAnchor::Pointer,
            )
            .show(|ui| {
                let line = |ui: &mut Ui, text: RichText| {
                    ui.add(egui::Label::new(text).wrap_mode(egui::TextWrapMode::Extend));
                };
                line(ui, RichText::new(state.to_string()).monospace().size(12.5));
                line(
                    ui,
                    RichText::new(format!("› {resolved}"))
                        .monospace()
                        .size(12.5)
                        .color(p.accent),
                );
            });
        }
    }
}

/// Fiche du bloc choisi, avec remappage.
fn selection(app: &mut OdasioApp, ui: &mut Ui, area: &Area) {
    let p = Palette::of(ui);
    let Some((x, y, z)) = app.preview.selected else {
        return;
    };
    let Some(state) = area.palette.get(area.scene.get(x, y, z) as usize).cloned() else {
        return;
    };
    let resolved = app.mapping.resolve(&state);
    let world = (
        area.x0 + x as i32,
        area.bottom + y as i32,
        area.z0 + z as i32,
    );
    let mut open = None;
    let mut clear = false;
    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        theme::section(
            ui,
            tr("preview.selected"),
            &trf(
                "preview.position",
                &[("x", &world.0), ("y", &world.1), ("z", &world.2)],
            ),
        );
        egui::Grid::new("preview_selected")
            .num_columns(2)
            .spacing([18.0, 6.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Minecraft").color(p.muted));
                ui.label(RichText::new(state.to_string()).monospace());
                ui.end_row();
                ui.label(RichText::new("Vintage Story").color(p.muted));
                ui.horizontal(|ui| {
                    theme::badge(
                        ui,
                        crate::i18n::quality(resolved.quality),
                        p.quality(resolved.quality),
                    );
                    ui.label(RichText::new(resolved.to_string()).monospace());
                });
                ui.end_row();
            });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if theme::primary_button(ui, tr("preview.change"), !state.is_air()).clicked() {
                open = Some(PickTarget::Block {
                    state: state.to_string(),
                    name: state.name.clone(),
                });
            }
            if resolved.source == Source::Override
                && theme::secondary_button(ui, tr("mapping.clear_override")).clicked()
            {
                clear = true;
            }
        });
    });
    if let Some(target) = open {
        app.mapping_view.picker = Some(Picker {
            target,
            query: String::new(),
            whole_id: true,
        });
    }
    if clear {
        app.mapping.policy.overrides.remove(&state.to_string());
        app.mapping.policy.overrides.remove(&state.name);
        app.plan_dirty = true;
    }
}
