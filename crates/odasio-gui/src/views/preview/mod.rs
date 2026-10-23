//! Étape 4 : aperçu 3D du monde, avant (Minecraft) et après conversion (Vintage Story),
//! côte à côte avec une caméra partagée.
//!
//! Navigation libre façon globe virtuel : vue d'ensemble de tout le monde, glisser pour se
//! déplacer, molette pour zoomer vers le curseur, clic droit glissé pour tourner et
//! incliner. De loin, le relief vient de la carte du dessus ; en s'approchant, les chunks
//! autour de la caméra sont chargés et dessinés bloc par bloc avec les textures et formes
//! des jeux installés. Un clic sur un bloc permet de le remapper ; la vue Vintage Story suit
//! en direct.

mod camera;
mod gpu;
mod stream;

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2, pos2, vec2};
use glam::DVec3;
use odasio_core::mapping::Source;
use odasio_core::mc::BlockState;
use odasio_render::Shape;
use odasio_render::mc_assets::{McAssets, detect_client_jar};
use odasio_render::mesh::{MeshData, mesh_box};
use odasio_render::topmap::TopMap;
use odasio_render::voxel::ChunkSource;
use odasio_render::vs_assets::{VsAssets, detect_vs_assets};

use crate::app::OdasioApp;
use crate::i18n::{tr, trf};
use crate::task::{State, Task};
use crate::theme::{self, Palette};
use crate::views::mapping::{self, PickTarget, Picker};
use camera::Camera;
use gpu::{Gpu, GpuMesh, Uniforms};
use odasio_core::Selection;
use stream::{LookTable, MC, Streamer, VS};

/// Ciel (couleur gamma), aussi couleur du brouillard.
const SKY: [f64; 3] = [0.56, 0.72, 0.91];
/// Distance de caméra en deçà de laquelle les blocs détaillés sont chargés.
const DETAIL_DISTANCE: f64 = 900.0;

/// Aspects des blocs lus dans les jeux installés (absents si le jeu ne l'est pas).
pub struct Assets {
    pub mc: Option<McAssets>,
    pub vs: Option<VsAssets>,
}

/// Bloc ou colonne désignés à la souris.
#[derive(Clone)]
struct Target {
    x: i32,
    y: i32,
    z: i32,
    state: BlockState,
    /// Point visé (pour zoomer vers le curseur).
    point: DVec3,
}

/// Moteur de rendu d'un monde ouvert.
struct Engine {
    gpu: Gpu,
    streamer: Streamer,
    camera: Camera,
    map: Arc<TopMap>,
    mapping_rev: u64,
    water: [u8; 3],
    water_built: bool,
}

#[derive(Default)]
pub struct ViewState {
    assets: Option<Task<Arc<Assets>>>,
    map: Option<Task<Arc<TopMap>>>,
    /// Monde et dimension en cours, et dossier de ses régions.
    source: Option<(PathBuf, String)>,
    region_dir: Option<PathBuf>,
    engine: Option<Engine>,
    hovered: Option<Target>,
    selected: Option<Target>,
    /// Boutons cliqués à l'image précédente.
    pending: Option<Input>,
    /// Mode sélection de la zone à convertir, et rectangle en cours de tracé.
    select_mode: bool,
    drag: Option<Drag>,
    /// Sélection déjà envoyée au GPU.
    mask_of: Option<Selection>,
}

impl ViewState {
    /// Ouvre l'aperçu directement en mode sélection de zone.
    pub fn open_zone_editor(&mut self) {
        self.select_mode = true;
        self.drag = None;
        self.selected = None;
    }
}

/// Rectangle de chunks en cours de tracé.
#[derive(Clone, Copy)]
struct Drag {
    start: (i32, i32),
    current: (i32, i32),
    remove: bool,
}

/// Lance la lecture des assets et de la carte dès qu'un monde est ouvert, pour que l'aperçu
/// soit prêt en arrivant à l'étape.
pub fn prefetch(app: &mut OdasioApp, ctx: &egui::Context) {
    let Some(loaded) = &app.world else {
        return;
    };
    let source = (loaded.path.clone(), loaded.dimension.clone());
    let dim = loaded.world.dimension(&loaded.dimension).cloned();
    let view = &mut app.preview;
    if view.assets.is_none() {
        view.assets = Some(Task::spawn(ctx, tr("preview.loading_assets"), |_| {
            let mc = detect_client_jar().and_then(|jar| McAssets::open(&jar).ok());
            let vs = detect_vs_assets().and_then(|root| VsAssets::open(&root).ok());
            Ok(Arc::new(Assets { mc, vs }))
        }));
    }
    if view.source.as_ref() != Some(&source) {
        *view = ViewState {
            assets: view.assets.take(),
            source: Some(source),
            region_dir: dim.as_ref().map(|d| d.region_dir.clone()),
            ..Default::default()
        };
        if let Some(dim) = dim {
            view.map = Some(Task::spawn(ctx, tr("preview.loading_map"), move |r| {
                let map = TopMap::build(&dim, |done, total| {
                    r.progress(
                        Some(done as f32 / total.max(1) as f32),
                        trf(
                            "preview.loading_map_n",
                            &[("done", &done), ("total", &total)],
                        ),
                    )
                })?;
                Ok(Arc::new(map))
            }));
        }
    }
}

/// Message, avancement et échec d'une tâche en cours ou ratée.
fn describe<T>(state: &State<T>) -> Option<(String, Option<f32>, bool)> {
    match state {
        State::Running { message, progress } => Some((message.clone(), *progress, false)),
        State::Failed(e) => Some((e.clone(), None, true)),
        State::Done(_) => None,
    }
}

/// Couleurs de carte des états de la carte du dessus, pour un des deux mondes.
fn far_colors(table: &LookTable, map: &TopMap, fallback: Option<&[[u8; 3]]>) -> Vec<[u8; 3]> {
    map.palette
        .iter()
        .enumerate()
        .map(|(i, state)| match table.look(state) {
            Some(look) => look.map_color(),
            // Bloc retiré à la conversion : la couleur d'origine, assombrie.
            None => fallback
                .and_then(|f| f.get(i))
                .map_or([128, 128, 128], |c| c.map(|v| (v as f32 * 0.6) as u8)),
        })
        .collect()
}

fn start_engine(app: &OdasioApp, assets: &Arc<Assets>, map: &Arc<TopMap>) -> Option<Engine> {
    let rs = app.render_state.as_ref()?;
    let region_dir = app.preview.region_dir.clone()?;
    let mapping = Arc::new(app.mapping.clone());
    let mut streamer = Streamer::new(ChunkSource::new(region_dir), assets.clone(), mapping);
    let mc_colors = far_colors(&streamer.tables[MC], map, None);
    let vs_colors = far_colors(&streamer.tables[VS], map, Some(&mc_colors));
    streamer.far_colors = [Arc::new(mc_colors), Arc::new(vs_colors)];
    streamer.revs = [0, app.mapping_rev];
    let water = assets
        .vs
        .as_ref()
        .and_then(|vs| vs.look("game:water-still-7"))
        .map_or([40, 66, 120], |l| l.map_color());

    // Cadrage : tout le monde visible, vu du dessus.
    let (x0, z0, x1, z1) = map.bounds;
    let center = DVec3::new((x0 + x1) as f64 / 2.0, 64.0, (z0 + z1) as f64 / 2.0);
    let extent = ((x1 - x0).max(z1 - z0).max(64)) as f64;
    let distance = extent / 2.0 / (camera::FOV / 2.0).tan() * 1.05;
    Some(Engine {
        gpu: Gpu::new(rs, stream::ATLAS_SIZE),
        streamer,
        camera: Camera::new(center, distance),
        map: map.clone(),
        mapping_rev: app.mapping_rev,
        water,
        water_built: false,
    })
}

pub fn show(app: &mut OdasioApp, ui: &mut Ui) {
    let p = Palette::of(ui);
    prefetch(app, ui.ctx());
    let view = &app.preview;
    let assets = view.assets.as_ref().and_then(Task::take_done);
    let map = view.map.as_ref().and_then(Task::take_done);
    let (Some(assets), Some(map)) = (assets, map) else {
        let message = view
            .assets
            .as_ref()
            .and_then(|t| t.with(describe))
            .or_else(|| view.map.as_ref().and_then(|t| t.with(describe)));
        if let Some((message, progress, failed)) = message {
            theme::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if failed {
                    ui.label(RichText::new(message).color(p.danger));
                } else {
                    theme::section(ui, tr("preview.title"), &message);
                    ui.add(
                        egui::ProgressBar::new(progress.unwrap_or(0.0))
                            .animate(true)
                            .fill(p.accent),
                    );
                }
            });
        }
        return;
    };
    if app.preview.engine.is_none() {
        app.preview.engine = start_engine(app, &assets, &map);
    }
    if app.preview.engine.is_none() {
        ui.label(RichText::new(tr("preview.no_gpu")).color(p.danger));
        return;
    }

    // Disposition : deux vues côte à côte (empilées si la fenêtre est étroite), barre
    // d'information en bas.
    let available = ui.available_rect_before_wrap();
    let info_height = 64.0;
    let views_rect = Rect::from_min_max(
        available.min,
        pos2(available.max.x, available.max.y - info_height - theme::GAP),
    );
    let stacked = views_rect.width() < 900.0;
    let rects = if stacked {
        let h = (views_rect.height() - theme::GAP) / 2.0;
        [
            Rect::from_min_size(views_rect.min, vec2(views_rect.width(), h)),
            Rect::from_min_size(
                views_rect.min + vec2(0.0, h + theme::GAP),
                vec2(views_rect.width(), h),
            ),
        ]
    } else {
        let w = (views_rect.width() - theme::GAP) / 2.0;
        [
            Rect::from_min_size(views_rect.min, vec2(w, views_rect.height())),
            Rect::from_min_size(
                views_rect.min + vec2(w + theme::GAP, 0.0),
                vec2(w, views_rect.height()),
            ),
        ]
    };

    let mut input = app.preview.pending.take().unwrap_or_default();
    let select_mode = app.preview.select_mode;
    for (side, rect) in [MC, VS].into_iter().zip(rects) {
        gestures(ui, side, rect, select_mode, &mut input);
    }
    frame(app, ui, rects, &input);

    let info_rect = Rect::from_min_max(
        pos2(available.min.x, available.max.y - info_height),
        available.max,
    );
    ui.allocate_rect(available, Sense::hover());
    info_bar(app, ui, info_rect);
    if mapping::picker(app, ui.ctx()) {
        app.plan_dirty = true;
    }
}

/// Gestes de la souris sur les vues et boutons, appliqués à la caméra partagée.
#[derive(Default)]
struct Input {
    pan: Vec2,
    orbit: Vec2,
    /// Facteur de zoom (0 = aucun) et position de la souris (vue, point dans la vue 0..1).
    zoom: f64,
    zoom_at: Option<(usize, [f64; 2])>,
    /// Vue et position (0..1) de la souris.
    hover: Option<(usize, [f64; 2])>,
    clicked: bool,
    rotate: f64,
    north: bool,
    top_view: bool,
    overview: bool,
    /// Mode sélection : début (point d'appui) et position courante d'un glisser, fin du
    /// glisser, retrait (Alt), clic sur un chunk.
    select_press: Option<[f64; 2]>,
    select_at: Option<[f64; 2]>,
    select_release: bool,
    select_remove: bool,
    select_click: Option<[f64; 2]>,
}

fn gestures(ui: &mut Ui, side: usize, rect: Rect, select_mode: bool, input: &mut Input) {
    let response = ui.interact(
        rect,
        ui.id().with(("preview3d", side)),
        Sense::click_and_drag(),
    );
    let relative = |pos: Pos2| {
        [
            ((pos.x - rect.left()) / rect.width()) as f64,
            ((pos.y - rect.top()) / rect.height()) as f64,
        ]
    };
    if let Some(pos) = response.hover_pos() {
        input.hover = Some((side, relative(pos)));
    }
    let (shift, alt) = ui.input(|i| (i.modifiers.shift, i.modifiers.alt));
    let primary = response.dragged_by(egui::PointerButton::Primary);
    if select_mode {
        // Glisser trace un rectangle de chunks ; le clic molette déplace la vue.
        if primary && !shift {
            if response.drag_started()
                && let Some(origin) = ui.input(|i| i.pointer.press_origin())
            {
                input.select_press = Some(relative(origin));
                input.select_remove = alt;
            }
            input.select_at = response.interact_pointer_pos().map(relative);
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            input.select_release = true;
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            input.pan += response.drag_delta();
        }
    } else {
        if primary && !shift {
            input.pan += response.drag_delta();
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            input.orbit += response.drag_delta();
        }
    }
    if response.dragged_by(egui::PointerButton::Secondary) || (primary && shift) {
        input.orbit += response.drag_delta();
    }
    if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    if response.hovered() {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
        let factor = (-scroll as f64 * 0.0018).exp() / pinch as f64;
        if (factor - 1.0).abs() > 1e-6 {
            input.zoom = factor;
            input.zoom_at = input.hover;
        }
    }
    if response.double_clicked() && !select_mode {
        input.zoom = 0.35;
        input.zoom_at = input.hover;
    } else if response.clicked() {
        if select_mode {
            input.select_click = input.hover.map(|(_, at)| at);
        } else {
            input.clicked = true;
        }
    }
}

/// Une image : caméra, chargement, rendu, superpositions.
fn frame(app: &mut OdasioApp, ui: &mut Ui, rects: [Rect; 2], input: &Input) {
    let p = Palette::of(ui);
    let mapping_rev = app.mapping_rev;
    let mapping = app.mapping.clone();
    let Some(rs) = app.render_state.clone() else {
        return;
    };
    let view = &mut app.preview;
    let Some(engine) = view.engine.as_mut() else {
        return;
    };
    let aspect = (rects[0].width() / rects[0].height()).max(0.1) as f64;
    let dt = ui.input(|i| i.stable_dt).min(0.1) as f64;

    // Zone à convertir : chunk sous un point de la vue.
    let chunk_at = |engine: &Engine, at: [f64; 2]| {
        pick(engine, aspect, at).map(|t| (t.x.div_euclid(16), t.z.div_euclid(16)))
    };
    if let Some(at) = input.select_press
        && let Some(start) = chunk_at(engine, at)
    {
        view.drag = Some(Drag {
            start,
            current: start,
            remove: input.select_remove,
        });
    }
    if let (Some(drag), Some(at)) = (view.drag.as_mut(), input.select_at)
        && let Some(current) = chunk_at(engine, at)
    {
        drag.current = current;
    }
    if input.select_release
        && let Some(drag) = view.drag.take()
    {
        if drag.remove {
            app.selection.remove_rect(drag.start, drag.current);
        } else {
            app.selection.add_rect(drag.start, drag.current);
        }
    }
    if let Some(at) = input.select_click
        && let Some(chunk) = chunk_at(engine, at)
    {
        app.selection.toggle(chunk);
    }
    // Masque de sélection : un texel par chunk sur l'emprise du monde.
    let (bx0, bz0, bx1, bz1) = engine.map.bounds;
    let mask_origin = (bx0.div_euclid(16), bz0.div_euclid(16));
    let mask_size = [
        ((bx1 - bx0) / 16).max(1) as u32,
        ((bz1 - bz0) / 16).max(1) as u32,
    ];
    if view.mask_of.as_ref() != Some(&app.selection) {
        let mut mask = vec![0u8; (mask_size[0] * mask_size[1]) as usize];
        for &(cx, cz) in &app.selection.chunks {
            let (x, z) = (cx - mask_origin.0, cz - mask_origin.1);
            if (0..mask_size[0] as i32).contains(&x) && (0..mask_size[1] as i32).contains(&z) {
                mask[z as usize * mask_size[0] as usize + x as usize] = 255;
            }
        }
        engine.gpu.set_selection(&rs, mask_size, &mask);
        view.mask_of = Some(app.selection.clone());
    }

    // Correspondances modifiées : le côté Vintage Story est refait.
    if engine.mapping_rev != mapping_rev {
        engine.mapping_rev = mapping_rev;
        let table = LookTable::new(
            engine.streamer.tables[VS].assets(),
            Some(Arc::new(mapping.clone())),
        );
        let mc_colors = engine.streamer.far_colors[MC].clone();
        let colors = far_colors(&table, &engine.map, Some(&mc_colors));
        engine
            .streamer
            .set_mapping(Arc::new(mapping), mapping_rev, colors);
    }

    // Ce qui est sous la souris (avant de bouger la caméra : la vue affichée).
    let hovered = input.hover.and_then(|(_, at)| pick(engine, aspect, at));
    view.hovered = hovered.clone();
    if input.clicked {
        view.selected = hovered.clone();
    }

    // Caméra.
    let camera = &mut engine.camera;
    if input.overview {
        let fresh = Camera::new(camera.goal.target, camera.max_distance / 1.5);
        let (x0, z0, x1, z1) = engine.map.bounds;
        camera.goal = fresh.goal;
        camera.goal.target = DVec3::new((x0 + x1) as f64 / 2.0, 64.0, (z0 + z1) as f64 / 2.0);
        camera.auto_pitch = true;
    }
    if input.pan != Vec2::ZERO {
        camera.pan(
            [input.pan.x as f64, input.pan.y as f64],
            rects[0].height() as f64,
        );
    }
    if input.orbit != Vec2::ZERO {
        camera.orbit([input.orbit.x as f64, input.orbit.y as f64]);
    }
    if input.zoom != 0.0 {
        let anchor = input
            .zoom_at
            .and_then(|(_, at)| pick(engine, aspect, at))
            .map(|t| t.point);
        engine.camera.zoom(input.zoom, anchor);
    }
    let camera = &mut engine.camera;
    camera.goal.yaw += input.rotate;
    if input.north {
        camera.goal.yaw = 0.0;
    }
    if input.top_view {
        // Bascule vue du dessus / vue inclinée.
        if camera.goal.pitch > 1.4 {
            camera.auto_pitch = false;
            camera.goal.pitch = 0.75;
        } else {
            camera.auto_pitch = false;
            camera.goal.pitch = camera::TOP_PITCH;
        }
    }
    // Le point visé suit le relief.
    let t = camera.goal.target;
    if let Some(h) = engine.map.height(t.x.floor() as i32, t.z.floor() as i32) {
        camera.goal.target.y = h as f64 + 1.0;
    }
    let moving = camera.update(dt);

    // Chargement autour de la caméra.
    let pose = engine.camera.pose;
    let radius = if pose.distance < DETAIL_DISTANCE {
        ((pose.distance * 1.4 / 16.0) as i32).clamp(4, 16)
    } else {
        0
    };
    let center = (
        (pose.target.x / 16.0).floor() as i32,
        (pose.target.z / 16.0).floor() as i32,
    );
    let eye = pose.eye();
    let mut far: Vec<((i32, i32), usize)> = engine
        .map
        .regions
        .keys()
        .map(|&(rx, rz)| {
            let (cx, cz) = (rx as f64 * 512.0 + 256.0, rz as f64 * 512.0 + 256.0);
            let flat = ((cx - pose.target.x).hypot(cz - pose.target.z) - 362.0).max(0.0);
            let d = flat.hypot(eye.y - 64.0);
            let step = if d < 500.0 {
                2
            } else if d < 1400.0 {
                4
            } else if d < 4000.0 {
                8
            } else {
                16
            };
            ((rx, rz), step)
        })
        .collect();
    far.sort_by_key(|(_, step)| *step);
    engine
        .streamer
        .update(&rs, &mut engine.gpu, &engine.map, center, radius, &far);
    for side in [MC, VS] {
        let atlas = engine.streamer.atlases[side].lock().unwrap();
        engine.gpu.sync_atlas(&rs, side, &atlas);
    }
    if !engine.water_built {
        engine.water_built = true;
        let (x0, z0, x1, z1) = engine.map.bounds;
        let m = 16384.0;
        let w = engine.water;
        let mut water = mesh_box(
            [x0 as f32 - m, 62.7, z0 as f32 - m],
            [x1 as f32 + m, 62.8, z1 as f32 + m],
            [w[0], w[1], w[2], 215],
        );
        // Seulement le dessus (le reste de la boîte est sous l'eau).
        water.indices.truncate(6);
        mark_plain(&mut water);
        engine.gpu.sides[VS].water = GpuMesh::new(&rs.device, &water);
    }

    // Surlignage : bloc choisi et bloc survolé.
    let mut highlight = MeshData::default();
    for (target, color) in [
        (view.selected.as_ref(), [240, 170, 60, 110]),
        (view.hovered.as_ref(), [255, 255, 255, 60]),
    ] {
        if let Some(t) = target {
            let e = 0.02;
            let mut b = mesh_box(
                [t.x as f32 - e, t.y as f32 - e, t.z as f32 - e],
                [
                    t.x as f32 + 1.0 + e,
                    t.y as f32 + 1.0 + e,
                    t.z as f32 + 1.0 + e,
                ],
                color,
            );
            mark_plain(&mut b);
            let base = highlight.vertices.len() as u32;
            highlight.vertices.extend(b.vertices);
            highlight.indices.extend(b.indices.iter().map(|i| i + base));
        }
    }
    engine.gpu.highlight = GpuMesh::new(&rs.device, &highlight);

    // Rendu.
    let fog = engine.camera.fog_distance() as f32;
    let uniforms = Uniforms {
        view_proj: engine.camera.view_proj(aspect).to_cols_array_2d(),
        camera: [eye.x as f32, eye.y as f32, eye.z as f32, 1.0],
        fog: [
            SKY[0].powf(2.2) as f32,
            SKY[1].powf(2.2) as f32,
            SKY[2].powf(2.2) as f32,
            fog,
        ],
        near_rect: engine.streamer.near_rect,
        selection: [
            mask_origin.0 as f32,
            mask_origin.1 as f32,
            mask_size[0] as f32,
            mask_size[1] as f32,
        ],
        selection_mode: [
            if app.selection.is_empty() { 0.0 } else { 1.0 },
            if view.select_mode { 1.0 } else { 0.0 },
            match view.drag {
                Some(d) if d.remove => -1.0,
                Some(_) => 1.0,
                None => 0.0,
            },
            0.0,
        ],
        drag_rect: view.drag.map_or([0.0; 4], |d| {
            [
                d.start.0.min(d.current.0) as f32,
                d.start.1.min(d.current.1) as f32,
                d.start.0.max(d.current.0) as f32,
                d.start.1.max(d.current.1) as f32,
            ]
        }),
    };
    let ppp = ui.ctx().pixels_per_point();
    let size = |r: Rect| {
        [
            ((r.width() * ppp) as u32).max(1),
            ((r.height() * ppp) as u32).max(1),
        ]
    };
    let ids = engine.gpu.render(
        &rs,
        &uniforms,
        [(MC, size(rects[0])), (VS, size(rects[1]))],
        SKY,
    );

    // Images et superpositions.
    let busy = engine.streamer.busy();
    let yaw = engine.camera.pose.yaw;
    let pitch = engine.camera.pose.pitch;
    let mut actions = Input::default();
    for (i, rect) in rects.into_iter().enumerate() {
        let painter = ui.painter_at(rect);
        painter.image(
            ids[i],
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.rect_stroke(
            rect,
            theme::RADIUS,
            Stroke::new(1.0_f32, p.border),
            egui::StrokeKind::Inside,
        );
        let title = if i == MC {
            "Minecraft"
        } else {
            "Vintage Story"
        };
        let label = painter.layout_no_wrap(
            title.to_string(),
            egui::FontId::proportional(13.5),
            Color32::WHITE,
        );
        let label_pos = rect.min + vec2(14.0, 12.0);
        let label_rect = Rect::from_min_size(label_pos, label.size()).expand2(vec2(10.0, 5.0));
        painter.rect_filled(label_rect, 8.0, Color32::from_black_alpha(150));
        painter.galley(label_pos, label, Color32::WHITE);
        if busy {
            egui::Spinner::new().paint_at(
                ui,
                Rect::from_center_size(
                    pos2(label_rect.right() + 18.0, label_rect.center().y),
                    vec2(16.0, 16.0),
                ),
            );
        }
        controls(ui, i, rect, yaw, pitch, &mut actions);
    }
    // Les boutons agissent à l'image suivante (comme un geste).
    view.pending = Some(actions);
    if moving || busy || view.drag.is_some() || view.pending.as_ref().is_some_and(Input::any) {
        ui.ctx().request_repaint();
    }
}

impl Input {
    fn any(&self) -> bool {
        self.zoom != 0.0 || self.rotate != 0.0 || self.north || self.top_view || self.overview
    }
}

/// Sommets sans texture et jamais masqués (surlignage, plan d'eau).
fn mark_plain(mesh: &mut MeshData) {
    for v in &mut mesh.vertices {
        v.uv = [-2.0, -2.0];
    }
}

/// Bloc visé par le rayon passant par un point de la vue : blocs détaillés chargés, sinon
/// relief de la carte.
fn pick(engine: &Engine, aspect: f64, at: [f64; 2]) -> Option<Target> {
    let (origin, dir) = engine.camera.ray(aspect, at);
    let streamer = &engine.streamer;
    let looks = streamer.tables[MC].snapshot(&streamer.source.registry);
    let registry = streamer.source.registry.read().unwrap();
    // Le rayon commence au plus haut point utile.
    let mut origin = origin;
    if origin.y > 400.0 && dir.y < 0.0 {
        origin += dir * ((origin.y - 400.0) / -dir.y);
    }
    let mut cell = origin.floor().as_ivec3();
    let step = dir.signum().as_ivec3();
    let inv = DVec3::new(1.0 / dir.x.abs(), 1.0 / dir.y.abs(), 1.0 / dir.z.abs());
    let next = |c: f64, s: i32, o: f64| if s > 0 { c + 1.0 - o } else { o - c };
    let mut t_max = DVec3::new(
        next(cell.x as f64, step.x, origin.x) * inv.x,
        next(cell.y as f64, step.y, origin.y) * inv.y,
        next(cell.z as f64, step.z, origin.z) * inv.z,
    );
    for _ in 0..30000 {
        if cell.y < -80 && dir.y < 0.0 {
            return None;
        }
        let pos = (cell.x.div_euclid(16), cell.z.div_euclid(16));
        let point = || origin + dir * t_max.min_element().max(0.0);
        match streamer.chunks.get(&pos) {
            Some(Some(chunk)) => {
                let id = chunk.get(
                    cell.x.rem_euclid(16),
                    cell.y - chunk.min_y,
                    cell.z.rem_euclid(16),
                );
                if id != 0
                    && let Some(look) = looks.get(id as usize).and_then(Option::as_ref)
                    && look.shape != Shape::Liquid
                    && let Some(state) = registry.get(id)
                {
                    return Some(Target {
                        x: cell.x,
                        y: cell.y,
                        z: cell.z,
                        state: state.clone(),
                        point: point(),
                    });
                }
            }
            Some(None) => {}
            None => {
                if let Some(h) = engine.map.height(cell.x, cell.z)
                    && cell.y <= h
                    && let Some((id, _)) = engine.map.column(cell.x, cell.z)
                {
                    return Some(Target {
                        x: cell.x,
                        y: h,
                        z: cell.z,
                        state: engine.map.palette[id as usize].clone(),
                        point: DVec3::new(cell.x as f64 + 0.5, h as f64 + 1.0, cell.z as f64 + 0.5),
                    });
                }
            }
        }
        if t_max.x < t_max.y && t_max.x < t_max.z {
            cell.x += step.x;
            t_max.x += inv.x;
        } else if t_max.y < t_max.z {
            cell.y += step.y;
            t_max.y += inv.y;
        } else {
            cell.z += step.z;
            t_max.z += inv.z;
        }
    }
    None
}

/// Boussole (nord en rouge, clic : nord en haut) et boutons, dans le coin de la vue.
fn controls(ui: &mut Ui, side: usize, rect: Rect, yaw: f64, pitch: f64, input: &mut Input) {
    let painter = ui.painter_at(rect);
    let center = pos2(rect.right() - 46.0, rect.top() + 46.0);
    let radius = 26.0;
    let compass = ui.interact(
        Rect::from_center_size(center, vec2(radius * 2.0 + 20.0, radius * 2.0 + 20.0)),
        ui.id().with(("compass", side)),
        Sense::click(),
    );
    painter.circle(
        center,
        radius + 14.0,
        Color32::from_black_alpha(if compass.hovered() { 190 } else { 140 }),
        Stroke::new(1.0_f32, Color32::from_white_alpha(50)),
    );
    // Direction à l'écran d'un vecteur horizontal du monde (x, z).
    let (s, c) = (yaw.sin() as f32, yaw.cos() as f32);
    let screen = |x: f32, z: f32| -> Vec2 {
        // Composantes sur les axes « droite » (cos, -sin) et « haut » (-sin, -cos).
        let right = x * c - z * s;
        let up = -x * s - z * c;
        vec2(right, -up).normalized()
    };
    let north = screen(0.0, -1.0);
    let across = vec2(-north.y, north.x) * 5.0;
    painter.add(egui::Shape::convex_polygon(
        vec![
            center + north * (radius - 6.0),
            center + across,
            center - across,
        ],
        Color32::from_rgb(225, 72, 60),
        Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            center - north * (radius - 6.0),
            center - across,
            center + across,
        ],
        Color32::from_gray(215),
        Stroke::NONE,
    ));
    for (text, (x, z), strong) in [
        (tr("preview.north"), (0.0, -1.0), true),
        (tr("preview.south"), (0.0, 1.0), false),
        (tr("preview.east"), (1.0, 0.0), false),
        (tr("preview.west"), (-1.0, 0.0), false),
    ] {
        painter.text(
            center + screen(x, z) * (radius + 5.0),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(if strong { 12.5 } else { 11.0 }),
            if strong {
                Color32::from_rgb(255, 120, 105)
            } else {
                Color32::from_gray(230)
            },
        );
    }
    if compass.on_hover_text(tr("preview.compass_hint")).clicked() {
        input.north = true;
    }

    // Colonne de boutons sous la boussole.
    let top = pitch > 1.4;
    let mut y = center.y + radius + 26.0;
    for (text, hint, action) in [
        ("+", tr("preview.zoom_in"), 0u8),
        ("-", tr("preview.zoom_out"), 1),
        ("‹", tr("preview.rotate_left"), 2),
        ("›", tr("preview.rotate_right"), 3),
        (if top { "3D" } else { "2D" }, tr("preview.top_view"), 4),
        ("", tr("preview.overview"), 5),
    ] {
        let button = Rect::from_center_size(pos2(center.x, y + 15.0), vec2(32.0, 30.0));
        let response = ui
            .interact(
                button,
                ui.id().with(("preview_button", side, action)),
                Sense::click(),
            )
            .on_hover_text(hint);
        painter.rect_filled(
            button,
            8.0,
            Color32::from_black_alpha(if response.hovered() { 200 } else { 140 }),
        );
        if action == 5 {
            // Vue d'ensemble : coins d'un cadre (pas de glyphe fiable dans la police).
            let r = Rect::from_center_size(button.center(), vec2(14.0, 14.0));
            let stroke = Stroke::new(1.6_f32, Color32::WHITE);
            for (corner, dx, dy) in [
                (r.left_top(), 1.0, 1.0),
                (r.right_top(), -1.0, 1.0),
                (r.left_bottom(), 1.0, -1.0),
                (r.right_bottom(), -1.0, -1.0),
            ] {
                painter.line_segment([corner, corner + vec2(5.0 * dx, 0.0)], stroke);
                painter.line_segment([corner, corner + vec2(0.0, 5.0 * dy)], stroke);
            }
        } else {
            painter.text(
                button.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(if text.len() > 1 { 12.5 } else { 17.0 }),
                Color32::WHITE,
            );
        }
        if response.clicked() {
            match action {
                0 => input.zoom = 0.6,
                1 => input.zoom = 1.0 / 0.6,
                2 => input.rotate = std::f64::consts::FRAC_PI_4,
                3 => input.rotate = -std::f64::consts::FRAC_PI_4,
                4 => input.top_view = true,
                _ => input.overview = true,
            }
        }
        y += 34.0;
    }
}

/// Barre du bas : position, bloc survolé ou choisi, et remappage ; ou, en mode sélection,
/// la zone à convertir.
fn info_bar(app: &mut OdasioApp, ui: &mut Ui, rect: Rect) {
    let p = Palette::of(ui);
    let target = app
        .preview
        .selected
        .clone()
        .or_else(|| app.preview.hovered.clone());
    let selected = app.preview.selected.is_some();
    let select_mode = app.preview.select_mode;
    let zone = zone_text(&app.selection);
    let mut open = None;
    let mut clear = None;
    let mut deselect = false;
    let mut toggle_mode = false;
    let mut clear_zone = false;
    let frame = egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(theme::RADIUS)
        .inner_margin(egui::Margin::symmetric(16, 0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        frame.show(ui, |ui| {
            ui.set_min_size(vec2(rect.width() - 34.0, rect.height() - 2.0));
            // Ligne centrée verticalement, sans étirer les pastilles.
            ui.add_space((rect.height() - 30.0) / 2.0);
            ui.horizontal(|ui| {
                // À droite : le mode sélection, toujours accessible.
                let right = |ui: &mut Ui, toggle_mode: &mut bool, clear_zone: &mut bool| {
                    let label = if select_mode {
                        tr("preview.zone_done")
                    } else {
                        tr("preview.zone_edit")
                    };
                    if theme::primary_button(ui, label, true)
                        .on_hover_text(tr("preview.zone_hint"))
                        .clicked()
                    {
                        *toggle_mode = true;
                    }
                    if select_mode
                        && !app.selection.is_empty()
                        && theme::secondary_button(ui, tr("preview.zone_clear")).clicked()
                    {
                        *clear_zone = true;
                    }
                };
                if select_mode {
                    ui.label(RichText::new(&zone).strong().color(p.accent));
                    ui.add_space(10.0);
                    ui.label(RichText::new(tr("preview.zone_hint")).color(p.muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        right(ui, &mut toggle_mode, &mut clear_zone);
                    });
                    return;
                }
                let Some(target) = &target else {
                    if !app.selection.is_empty() {
                        ui.label(RichText::new(&zone).color(p.accent));
                        ui.add_space(10.0);
                    }
                    ui.label(RichText::new(tr("preview.hint")).color(p.muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        right(ui, &mut toggle_mode, &mut clear_zone);
                    });
                    return;
                };
                let resolved = app.mapping.resolve(&target.state);
                ui.label(
                    RichText::new(trf(
                        "preview.position",
                        &[("x", &target.x), ("y", &target.y), ("z", &target.z)],
                    ))
                    .color(p.muted),
                );
                ui.add_space(6.0);
                ui.label(RichText::new(target.state.to_string()).monospace());
                ui.label(RichText::new("›").color(p.muted));
                theme::badge(
                    ui,
                    crate::i18n::quality(resolved.quality),
                    p.quality(resolved.quality),
                );
                ui.label(
                    RichText::new(resolved.to_string())
                        .monospace()
                        .color(p.accent),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    right(ui, &mut toggle_mode, &mut clear_zone);
                    if !selected {
                        return;
                    }
                    if theme::secondary_button(ui, tr("preview.deselect")).clicked() {
                        deselect = true;
                    }
                    if resolved.source == Source::Override
                        && theme::secondary_button(ui, tr("mapping.clear_override")).clicked()
                    {
                        clear = Some(target.state.clone());
                    }
                    if theme::primary_button(ui, tr("preview.change"), true).clicked() {
                        open = Some(PickTarget::Block {
                            state: target.state.to_string(),
                            name: target.state.name.clone(),
                        });
                    }
                });
            });
        });
    });
    if toggle_mode {
        app.preview.select_mode = !select_mode;
        app.preview.drag = None;
        app.preview.selected = None;
    }
    if clear_zone {
        app.selection = Selection::default();
    }
    if let Some(target) = open {
        app.mapping_view.picker = Some(Picker {
            target,
            query: String::new(),
            whole_id: true,
        });
    }
    if let Some(state) = clear {
        app.mapping.policy.overrides.remove(&state.to_string());
        app.mapping.policy.overrides.remove(&state.name);
        app.plan_dirty = true;
    }
    if deselect {
        app.preview.selected = None;
    }
}

/// Résumé de la zone à convertir : tout le monde, ou nombre de chunks et étendue.
pub fn zone_text(selection: &Selection) -> String {
    match selection.bounds() {
        None => tr("preview.zone_all").to_string(),
        Some((x0, z0, x1, z1)) => trf(
            "preview.zone_some",
            &[
                ("n", &crate::theme::thousands(selection.len() as u64)),
                ("w", &((x1 - x0 + 1) * 16)),
                ("h", &((z1 - z0 + 1) * 16)),
            ],
        ),
    }
}
