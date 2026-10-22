//! Étape 4 : aperçu du monde avant (Minecraft) et après conversion (Vintage Story), côte à
//! côte, avec une caméra partagée.
//!
//! Navigation façon carte : vue d'ensemble du monde vue de dessus, déplacement à la souris,
//! zoom à la molette centré sur le curseur. En zoomant assez, la zone sous la caméra passe en
//! vue isométrique (textures et formes des jeux installés). La rotation (clic droit glissé,
//! boutons ou boussole) tourne la vue par quarts de tour. Un clic sur un bloc permet de le
//! remapper ; la vue Vintage Story suit en direct.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{
    self, Color32, ColorImage, Pos2, Rect, RichText, Sense, Stroke, TextureHandle, Ui, Vec2, pos2,
    vec2,
};
use odasio_core::mapping::Source;
use odasio_core::mc::BlockState;
use odasio_render::BlockLook;
use odasio_render::area::Area;
use odasio_render::iso::{self, Image, Scene};
use odasio_render::mc_assets::{McAssets, detect_client_jar};
use odasio_render::topmap::{INCOMPLETE, REGION, TopMap};
use odasio_render::vs_assets::{VsAssets, detect_vs_assets};

use crate::app::OdasioApp;
use crate::i18n::{tr, trf};
use crate::task::{State, Task};
use crate::theme::{self, Palette};
use crate::views::mapping::{self, PickTarget, Picker};

/// Pixels par bloc à partir desquels la vue passe en isométrique.
const ISO_ZOOM: f32 = 6.0;
const MAX_ZOOM: f32 = 48.0;
/// Blocs affichés sous la surface la plus basse de la zone isométrique.
const DEPTH: i32 = 12;
/// Textures de région pleine résolution créées au plus par image (fluidité).
const FULL_RES_PER_FRAME: usize = 4;
const VOID: Color32 = Color32::from_rgb(18, 18, 21);

/// Aspects des blocs lus dans les jeux installés (absents si le jeu ne l'est pas).
pub struct Assets {
    mc: Option<McAssets>,
    vs: Option<VsAssets>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Side {
    Mc,
    Vs,
}

/// Vue isométrique rendue d'une zone.
struct IsoRender {
    area: Arc<Area>,
    rotation: u8,
    mapping_rev: u64,
    a: f32,
    scene: Scene,
    /// Altitude de référence (surface médiane) pour placer la zone sous la caméra.
    level: f32,
    mc: Option<(Image, TextureHandle)>,
    vs: Option<(Image, TextureHandle)>,
}

/// Bloc ou colonne désignés à la souris.
#[derive(Clone)]
struct Target {
    x: i32,
    /// Altitude, connue seulement en vue isométrique.
    y: Option<i32>,
    z: i32,
    state: BlockState,
}

pub struct ViewState {
    assets: Option<Task<Arc<Assets>>>,
    map: Option<Task<Arc<TopMap>>>,
    /// Monde et dimension de la carte en cours.
    source: Option<(PathBuf, String)>,
    area: Option<Task<Arc<Area>>>,
    /// Caméra : centre en blocs, pixels par bloc, quarts de tour.
    center: [f64; 2],
    zoom: f32,
    rotation: u8,
    /// La caméra a été cadrée sur le monde.
    framed: bool,
    /// Couleurs de carte par index de palette, et version des correspondances côté VS.
    colors: Option<(Vec<Color32>, Vec<Color32>, u64)>,
    water: Color32,
    tiles: HashMap<(Side, i32, i32, u8), TextureHandle>,
    iso: Option<IsoRender>,
    orbit_drag: f32,
    hovered: Option<Target>,
    selected: Option<Target>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            assets: None,
            map: None,
            source: None,
            area: None,
            center: [0.0, 0.0],
            zoom: 0.25,
            rotation: 0,
            framed: false,
            colors: None,
            water: Color32::from_rgb(40, 66, 120),
            tiles: HashMap::new(),
            iso: None,
            orbit_drag: 0.0,
            hovered: None,
            selected: None,
        }
    }
}

/// Rotation d'un vecteur du monde (x, z) vers l'écran, pour `quarter` quarts de tour
/// (même convention que [`Scene::rotated`]).
fn rot(q: u8, v: [f64; 2]) -> [f64; 2] {
    match q % 4 {
        0 => v,
        1 => [-v[1], v[0]],
        2 => [-v[0], -v[1]],
        _ => [v[1], -v[0]],
    }
}

fn unrot(q: u8, v: [f64; 2]) -> [f64; 2] {
    rot((4 - q % 4) % 4, v)
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

    refresh_colors(app, &assets, &map);
    let available = ui.available_rect_before_wrap();
    // Barre d'information en bas, vues au-dessus.
    let info_height = 64.0;
    let views_rect = Rect::from_min_max(
        available.min,
        pos2(available.max.x, available.max.y - info_height - theme::GAP),
    );
    let stacked = views_rect.width() < 900.0;
    let (mc_rect, vs_rect) = if stacked {
        let h = (views_rect.height() - theme::GAP) / 2.0;
        (
            Rect::from_min_size(views_rect.min, vec2(views_rect.width(), h)),
            Rect::from_min_size(
                views_rect.min + vec2(0.0, h + theme::GAP),
                vec2(views_rect.width(), h),
            ),
        )
    } else {
        let w = (views_rect.width() - theme::GAP) / 2.0;
        (
            Rect::from_min_size(views_rect.min, vec2(w, views_rect.height())),
            Rect::from_min_size(
                views_rect.min + vec2(w + theme::GAP, 0.0),
                vec2(w, views_rect.height()),
            ),
        )
    };
    if !app.preview.framed {
        frame_world(&mut app.preview, &map, mc_rect.size());
    }
    if app.preview.zoom >= ISO_ZOOM {
        refresh_iso(app, ui.ctx(), &assets, mc_rect);
    }

    let mut hovered = None;
    let mut input = Input::default();
    for (side, rect) in [(Side::Mc, mc_rect), (Side::Vs, vs_rect)] {
        viewport(
            &mut app.preview,
            ui,
            side,
            rect,
            &map,
            &assets,
            &mut hovered,
            &mut input,
        );
    }
    apply_input(&mut app.preview, &input, &map, mc_rect.size());
    app.preview.hovered = hovered;

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

/// Message, avancement et échec d'une tâche en cours ou ratée.
fn describe<T>(state: &State<T>) -> Option<(String, Option<f32>, bool)> {
    match state {
        State::Running { message, progress } => Some((message.clone(), *progress, false)),
        State::Failed(e) => Some((e.clone(), None, true)),
        State::Done(_) => None,
    }
}

/// Cadre la caméra sur l'ensemble des régions, nord en haut.
fn frame_world(view: &mut ViewState, map: &TopMap, size: Vec2) {
    let (x0, z0, x1, z1) = map.bounds;
    view.center = [(x0 + x1) as f64 / 2.0, (z0 + z1) as f64 / 2.0];
    let (w, h) = ((x1 - x0).max(16) as f32, (z1 - z0).max(16) as f32);
    view.zoom = (size.x / w).min(size.y / h) * 0.95;
    view.rotation = 0;
    view.framed = true;
}

/// Couleurs de carte de chaque état : texture du dessus, côté Minecraft et côté Vintage
/// Story (selon les correspondances courantes).
fn refresh_colors(app: &mut OdasioApp, assets: &Assets, map: &TopMap) {
    let rev = app.mapping_rev;
    if app.preview.colors.as_ref().is_some_and(|c| c.2 == rev) {
        return;
    }
    let to_color = |c: [u8; 3]| Color32::from_rgb(c[0], c[1], c[2]);
    let mc: Vec<Color32> = match (&app.preview.colors, &assets.mc) {
        (Some((mc, _, _)), _) => mc.clone(),
        (None, Some(assets)) => map
            .palette
            .iter()
            .map(|s| {
                assets
                    .look(s)
                    .map_or(Color32::from_rgb(255, 0, 255), |l| to_color(l.map_color()))
            })
            .collect(),
        (None, None) => vec![Color32::GRAY; map.palette.len()],
    };
    if let Some(vs) = &assets.vs
        && let Some(water) = vs.look("game:water-still-7")
    {
        app.preview.water = to_color(water.map_color());
    }
    let vs: Vec<Color32> = map
        .palette
        .iter()
        .zip(&mc)
        .map(|(state, &fallback)| {
            let Some(vs) = &assets.vs else {
                return fallback;
            };
            let resolved = app.mapping.resolve(state);
            let code = match &resolved.fluid {
                Some(fluid) if resolved.block == "game:air" => fluid.clone(),
                _ => resolved.block.clone(),
            };
            // Bloc retiré : on montre le sol de l'autre jeu, assombri.
            vs.look(&code)
                .map_or(fallback.gamma_multiply(0.6), |l| to_color(l.map_color()))
        })
        .collect();
    app.preview.tiles.retain(|k, _| k.0 == Side::Mc);
    app.preview.colors = Some((mc, vs, rev));
}

/// Image d'une région à un niveau de détail (`lod` : 0 = 1 px par bloc, 1 = 1/4, 2 = 1/16).
fn region_image(
    map: &TopMap,
    key: (i32, i32),
    colors: &[Color32],
    side: Side,
    water: Color32,
    lod: u8,
) -> Option<ColorImage> {
    let region = map.regions.get(&key)?;
    let step = 1usize << (2 * lod);
    let size = REGION / step;
    let mut pixels = Vec::with_capacity(size * size);
    for py in 0..size {
        for px in 0..size {
            let i = (py * step + step / 2) * REGION + px * step + step / 2;
            let top = region.top[i];
            let id = top & !INCOMPLETE;
            // Côté VS, les chunks incomplets sont ignorés par la conversion : océan.
            let color = if id == 0 || (side == Side::Vs && top & INCOMPLETE != 0) {
                match side {
                    Side::Mc => VOID,
                    Side::Vs => water,
                }
            } else {
                let shade = [0.82, 1.0, 1.1][region.shade[i] as usize];
                let c = colors[id as usize];
                Color32::from_rgb(
                    (c.r() as f32 * shade).min(255.0) as u8,
                    (c.g() as f32 * shade).min(255.0) as u8,
                    (c.b() as f32 * shade).min(255.0) as u8,
                )
            };
            pixels.push(color);
        }
    }
    Some(ColorImage {
        size: [size, size],
        pixels,
        source_size: vec2(size as f32, size as f32),
    })
}

/// Charge et rend la zone isométrique sous la caméra quand elle a changé.
fn refresh_iso(app: &mut OdasioApp, ctx: &egui::Context, assets: &Assets, rect: Rect) {
    let view = &mut app.preview;
    // Taille de zone : ce que la vue montre, arrondie au chunk, bornée pour rester fluide.
    let visible = rect.width().max(rect.height()) / (view.zoom * 1.414);
    let size = ((visible / 16.0).ceil() as i32 * 16).clamp(32, 128);
    let center = [view.center[0].floor() as i32, view.center[1].floor() as i32];

    let current = view.area.as_ref().and_then(Task::take_done);
    let stale = current.as_ref().is_none_or(|area| {
        let s = area.scene.size_x as i32;
        let (ax, az) = (area.x0 + s / 2, area.z0 + s / 2);
        (center[0] - ax).abs() > s / 4
            || (center[1] - az).abs() > s / 4
            || size > s * 3 / 2
            || size < s / 2
    });
    let running = view.area.as_ref().is_some_and(Task::is_running);
    if stale
        && !running
        && let Some((path, dimension)) = &view.source
        && let Some(dim) = app
            .world
            .as_ref()
            .filter(|w| &w.path == path)
            .and_then(|w| w.world.dimension(dimension))
            .cloned()
    {
        // Hors du monde, la zone est vide mais existe : la vue isométrique reste cohérente.
        view.area = Some(Task::spawn(ctx, tr("preview.loading_area"), move |_| {
            let area = Area::load(&dim, center[0], center[1], size, DEPTH)?;
            Ok(Arc::new(area.unwrap_or_else(|| Area {
                x0: center[0] - size / 2,
                z0: center[1] - size / 2,
                bottom: 60,
                scene: Scene::new(size as usize, 1, size as usize),
                palette: vec![BlockState::new("minecraft:air")],
            })))
        }));
    }
    // Pendant le chargement de la zone suivante, la précédente reste affichée.
    let Some(area) = current.or_else(|| app.preview.iso.as_ref().map(|r| r.area.clone())) else {
        return;
    };

    let rev = app.mapping_rev;
    let rotation = app.preview.rotation;
    let same_area = app
        .preview
        .iso
        .as_ref()
        .is_some_and(|r| Arc::ptr_eq(&r.area, &area) && r.rotation == rotation);
    if same_area
        && app
            .preview
            .iso
            .as_ref()
            .is_some_and(|r| r.mapping_rev == rev)
    {
        return;
    }
    let a = if area.scene.size_x <= 64 { 8.0 } else { 5.0 };
    let scene = area.scene.rotated(rotation);
    let upload = |name: &str, image: Image| {
        let color = ColorImage::from_rgba_unmultiplied([image.width, image.height], &image.rgba);
        let texture = ctx.load_texture(name, color, egui::TextureOptions::NEAREST);
        (image, texture)
    };
    let mc = if same_area {
        app.preview.iso.take().and_then(|r| r.mc)
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
    // Surface médiane : référence verticale stable pendant le déplacement.
    let mut surface: Vec<usize> = Vec::new();
    for z in 0..area.scene.size_z {
        for x in 0..area.scene.size_x {
            if let Some(y) = (0..area.scene.size_y)
                .rev()
                .find(|&y| area.scene.get(x, y, z) != 0)
            {
                surface.push(y);
            }
        }
    }
    surface.sort_unstable();
    let level = surface
        .get(surface.len() / 2)
        .map_or(0.0, |&y| y as f32 + 1.0);
    app.preview.iso = Some(IsoRender {
        area,
        rotation,
        mapping_rev: rev,
        a,
        scene,
        level,
        mc,
        vs,
    });
}

/// Gestes de la souris sur l'une des vues, appliqués à la caméra partagée.
#[derive(Default)]
struct Input {
    drag: Vec2,
    /// Facteur de zoom (0 = aucun).
    zoom: f32,
    /// Point (relatif au centre de la vue) qui reste fixe pendant le zoom.
    zoom_anchor: Vec2,
    orbit: f32,
    rotate: i8,
    clicked: bool,
    reset_north: bool,
    overview: bool,
}

/// Correspondance écran ↔ monde pour la vue isométrique courante.
struct IsoFrame<'a> {
    render: &'a IsoRender,
    /// Échelle écran / image et coin de l'image à l'écran.
    scale: f32,
    origin: Pos2,
}

impl IsoFrame<'_> {
    fn new<'a>(center: [f64; 2], zoom: f32, render: &'a IsoRender, rect: Rect) -> IsoFrame<'a> {
        let area = &render.area;
        // Coordonnées continues du centre de la caméra dans la scène tournée.
        let (lx, lz) = (
            (center[0] - area.x0 as f64) as f32,
            (center[1] - area.z0 as f64) as f32,
        );
        let (sx, sz) = (area.scene.size_x as f32, area.scene.size_z as f32);
        let (rx, rz) = match render.rotation % 4 {
            0 => (lx, lz),
            1 => (sz - lz, lx),
            2 => (sx - lx, sz - lz),
            _ => (lz, sx - lx),
        };
        let at = iso::project(&render.scene, render.a, rx, render.level, rz);
        // Même échelle au sol qu'en carte : la diagonale d'un bloc vaut 1,41 bloc.
        let scale = zoom * 1.414 / (2.0 * render.a);
        IsoFrame {
            render,
            scale,
            origin: rect.center() - vec2(at[0], at[1]) * scale,
        }
    }

    fn image_rect(&self, image: &Image) -> Rect {
        Rect::from_min_size(
            self.origin,
            vec2(image.width as f32, image.height as f32) * self.scale,
        )
    }

    /// Bloc visible au point de l'écran.
    fn pick(&self, image: &Image, pos: Pos2) -> Option<Target> {
        let local = (pos - self.origin) / self.scale;
        if local.x < 0.0 || local.y < 0.0 {
            return None;
        }
        let cell = image.picked(local.x as usize, local.y as usize)?;
        let area = &self.render.area;
        let (x, y, z) = self.render.scene.position(cell);
        let (ox, oz) = area.scene.unrotate(self.render.rotation, x, z);
        let state = area
            .palette
            .get(area.scene.get(ox, y, oz) as usize)?
            .clone();
        Some(Target {
            x: area.x0 + ox as i32,
            y: Some(area.bottom + y as i32),
            z: area.z0 + oz as i32,
            state,
        })
    }

    /// Contour à l'écran d'un bloc de la zone.
    fn outline(&self, target: &Target) -> Option<Vec<Pos2>> {
        let area = &self.render.area;
        let (x, z) = (target.x - area.x0, target.z - area.z0);
        let y = target.y? - area.bottom;
        let (sx, sy, sz) = (
            area.scene.size_x as i32,
            area.scene.size_y as i32,
            area.scene.size_z as i32,
        );
        if !(0..sx).contains(&x) || !(0..sy).contains(&y) || !(0..sz).contains(&z) {
            return None;
        }
        let (rx, rz) = area
            .scene
            .rotate_xz(self.render.rotation, x as usize, z as usize);
        let points = iso::block_outline(&self.render.scene, self.render.a, rx, y as usize, rz);
        Some(
            points
                .iter()
                .map(|[px, py]| self.origin + vec2(*px, *py) * self.scale)
                .collect(),
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn viewport(
    view: &mut ViewState,
    ui: &mut Ui,
    side: Side,
    rect: Rect,
    map: &TopMap,
    assets: &Assets,
    hovered: &mut Option<Target>,
    input: &mut Input,
) {
    let p = Palette::of(ui);
    let (center, zoom, rotation) = (view.center, view.zoom, view.rotation);
    let iso_mode = zoom >= ISO_ZOOM;
    let response = ui.interact(
        rect,
        ui.id().with(("preview_view", side as u8)),
        Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    let background = match side {
        Side::Mc => VOID,
        Side::Vs => view.water,
    };
    painter.rect_filled(rect, theme::RADIUS, background);

    let world_to_screen = |x: f64, z: f64| -> Pos2 {
        let v = rot(rotation, [x - center[0], z - center[1]]);
        rect.center() + vec2(v[0] as f32, v[1] as f32) * zoom
    };
    let screen_to_world = |pos: Pos2| -> [f64; 2] {
        let d = (pos - rect.center()) / zoom;
        let v = unrot(rotation, [d.x as f64, d.y as f64]);
        [center[0] + v[0], center[1] + v[1]]
    };
    let previous_hover = view.hovered.clone();
    let highlights = |hovered: &Option<Target>| {
        [
            (view.selected.clone(), Stroke::new(2.0, p.accent)),
            (
                hovered.clone().or(previous_hover.clone()),
                Stroke::new(1.5, Color32::from_white_alpha(210)),
            ),
        ]
    };

    let iso_render = view.iso.as_ref().filter(|_| iso_mode).and_then(|r| {
        let image = match side {
            Side::Mc => r.mc.as_ref(),
            Side::Vs => r.vs.as_ref(),
        }?;
        Some((r, image))
    });
    if let Some((render, (image, texture))) = iso_render {
        let frame = IsoFrame::new(center, zoom, render, rect);
        painter.image(
            texture.id(),
            frame.image_rect(image),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if let Some(pos) = response.hover_pos()
            && let Some(target) = frame.pick(image, pos)
        {
            *hovered = Some(target);
        }
        for (target, stroke) in highlights(hovered) {
            if let Some(points) = target.and_then(|t| frame.outline(&t)) {
                painter.add(egui::Shape::closed_line(points, stroke));
            }
        }
    } else {
        // Carte : régions visibles, au niveau de détail adapté au zoom.
        let lod = if zoom >= 0.5 {
            0
        } else if zoom >= 0.125 {
            1
        } else {
            2
        };
        let corners = [
            rect.left_top(),
            rect.right_top(),
            rect.left_bottom(),
            rect.right_bottom(),
        ]
        .map(screen_to_world);
        let min_x = corners.iter().map(|c| c[0]).fold(f64::MAX, f64::min);
        let max_x = corners.iter().map(|c| c[0]).fold(f64::MIN, f64::max);
        let min_z = corners.iter().map(|c| c[1]).fold(f64::MAX, f64::min);
        let max_z = corners.iter().map(|c| c[1]).fold(f64::MIN, f64::max);
        let r = REGION as f64;
        let mut created = 0;
        if let Some((mc_colors, vs_colors, _)) = &view.colors {
            let colors = match side {
                Side::Mc => mc_colors,
                Side::Vs => vs_colors,
            };
            for rz in (min_z / r).floor() as i32..=(max_z / r).floor() as i32 {
                for rx in (min_x / r).floor() as i32..=(max_x / r).floor() as i32 {
                    if !map.regions.contains_key(&(rx, rz)) {
                        continue;
                    }
                    // Texture au bon niveau ; en attendant, un niveau plus grossier.
                    let mut texture = None;
                    for level in lod..=2 {
                        let key = (side, rx, rz, level);
                        if let Some(t) = view.tiles.get(&key) {
                            texture = Some(t.clone());
                            break;
                        }
                        if level == 0 && created >= FULL_RES_PER_FRAME {
                            continue;
                        }
                        created += usize::from(level == 0);
                        if let Some(image) =
                            region_image(map, (rx, rz), colors, side, view.water, level)
                        {
                            let filter = if level == 0 {
                                egui::TextureOptions::NEAREST
                            } else {
                                egui::TextureOptions::LINEAR
                            };
                            let t = ui.ctx().load_texture(
                                format!("region_{}_{rx}_{rz}_{level}", side as u8),
                                image,
                                filter,
                            );
                            view.tiles.insert(key, t.clone());
                            texture = Some(t);
                            break;
                        }
                    }
                    let Some(texture) = texture else {
                        continue;
                    };
                    let (x0, z0) = (rx as f64 * r, rz as f64 * r);
                    let mut mesh = egui::Mesh::with_texture(texture.id());
                    for (x, z, u, v) in [
                        (x0, z0, 0.0, 0.0),
                        (x0 + r, z0, 1.0, 0.0),
                        (x0 + r, z0 + r, 1.0, 1.0),
                        (x0, z0 + r, 0.0, 1.0),
                    ] {
                        mesh.vertices.push(egui::epaint::Vertex {
                            pos: world_to_screen(x, z),
                            uv: pos2(u, v),
                            color: Color32::WHITE,
                        });
                    }
                    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
                    painter.add(egui::Shape::mesh(mesh));
                }
            }
        }
        if created >= FULL_RES_PER_FRAME {
            ui.ctx().request_repaint();
        }

        if let Some(pos) = response.hover_pos() {
            let w = screen_to_world(pos);
            let (x, z) = (w[0].floor() as i32, w[1].floor() as i32);
            if let Some((id, _)) = map.column(x, z) {
                *hovered = Some(Target {
                    x,
                    y: None,
                    z,
                    state: map.palette[id as usize].clone(),
                });
            }
        }
        // Colonne survolée ou choisie, dès qu'elle est assez grande pour être vue.
        if zoom >= 2.0 {
            for (target, stroke) in highlights(hovered) {
                if let Some(t) = target {
                    let (x, z) = (t.x as f64, t.z as f64);
                    let points = vec![
                        world_to_screen(x, z),
                        world_to_screen(x + 1.0, z),
                        world_to_screen(x + 1.0, z + 1.0),
                        world_to_screen(x, z + 1.0),
                    ];
                    painter.add(egui::Shape::closed_line(points, stroke));
                }
            }
        }
    }

    let installed = match side {
        Side::Mc => assets.mc.is_some(),
        Side::Vs => assets.vs.is_some(),
    };
    if !installed {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            match side {
                Side::Mc => tr("preview.no_mc"),
                Side::Vs => tr("preview.no_vs"),
            },
            egui::FontId::proportional(14.0),
            p.muted,
        );
    }

    // Titre et mode, en surimpression.
    let title = match side {
        Side::Mc => "Minecraft",
        Side::Vs => "Vintage Story",
    };
    let mode = if iso_mode {
        tr("preview.mode_iso")
    } else {
        tr("preview.mode_map")
    };
    let label = painter.layout_no_wrap(
        format!("{title}  ·  {mode}"),
        egui::FontId::proportional(13.5),
        Color32::WHITE,
    );
    let label_pos = rect.min + vec2(14.0, 12.0);
    let label_rect = Rect::from_min_size(label_pos, label.size()).expand2(vec2(10.0, 5.0));
    painter.rect_filled(label_rect, 8.0, Color32::from_black_alpha(150));
    painter.galley(label_pos, label, Color32::WHITE);
    if iso_mode && view.area.as_ref().is_some_and(Task::is_running) {
        let spinner = Rect::from_center_size(
            pos2(label_rect.right() + 18.0, label_rect.center().y),
            vec2(16.0, 16.0),
        );
        egui::Spinner::new().paint_at(ui, spinner);
    }

    controls(ui, side, rect, rotation, iso_mode, input);

    // Gestes.
    if response.dragged_by(egui::PointerButton::Primary) {
        input.drag += response.drag_delta();
    }
    if response.dragged_by(egui::PointerButton::Secondary) {
        input.orbit += response.drag_delta().x;
    }
    if response.hovered() {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
        let factor = (scroll * 0.0025).exp() * pinch;
        if factor != 1.0 {
            input.zoom = factor;
            if let Some(pos) = response.hover_pos() {
                input.zoom_anchor = pos - rect.center();
            }
        }
        if !iso_mode {
            ui.ctx().set_cursor_icon(if response.dragged() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
        }
    }
    if response.clicked() && hovered.is_some() {
        input.clicked = true;
    }
    if response.double_clicked() {
        input.zoom = 2.5;
        if let Some(pos) = response.hover_pos() {
            input.zoom_anchor = pos - rect.center();
        }
    }
}

/// Boussole (nord en rouge) et boutons de zoom et de rotation, dans le coin de la vue.
fn controls(ui: &mut Ui, side: Side, rect: Rect, rotation: u8, iso: bool, input: &mut Input) {
    let painter = ui.painter_at(rect);
    let center = pos2(rect.right() - 46.0, rect.top() + 46.0);
    let radius = 26.0;
    let compass = ui.interact(
        Rect::from_center_size(center, vec2(radius * 2.0 + 20.0, radius * 2.0 + 20.0)),
        ui.id().with(("compass", side as u8)),
        Sense::click(),
    );
    painter.circle(
        center,
        radius + 14.0,
        Color32::from_black_alpha(if compass.hovered() { 190 } else { 140 }),
        Stroke::new(1.0, Color32::from_white_alpha(50)),
    );
    // Direction à l'écran d'un vecteur du monde, selon le mode.
    let screen_dir = |v: [f64; 2]| -> Vec2 {
        let r = rot(rotation, v);
        let d = if iso {
            vec2((r[0] - r[1]) as f32, ((r[0] + r[1]) / 2.0) as f32)
        } else {
            vec2(r[0] as f32, r[1] as f32)
        };
        d.normalized()
    };
    let north = screen_dir([0.0, -1.0]);
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
    for (text, v, strong) in [
        (tr("preview.north"), [0.0, -1.0], true),
        (tr("preview.south"), [0.0, 1.0], false),
        (tr("preview.east"), [1.0, 0.0], false),
        (tr("preview.west"), [-1.0, 0.0], false),
    ] {
        painter.text(
            center + screen_dir(v) * (radius + 5.0),
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
        input.reset_north = true;
    }

    // Colonne de boutons sous la boussole.
    let mut y = center.y + radius + 26.0;
    for (text, hint, action) in [
        ("+", tr("preview.zoom_in"), 0u8),
        ("-", tr("preview.zoom_out"), 1),
        ("‹", tr("preview.rotate_left"), 2),
        ("›", tr("preview.rotate_right"), 3),
        ("", tr("preview.overview"), 4),
    ] {
        let button = Rect::from_center_size(pos2(center.x, y + 15.0), vec2(32.0, 30.0));
        let response = ui
            .interact(
                button,
                ui.id().with(("preview_button", side as u8, action)),
                Sense::click(),
            )
            .on_hover_text(hint);
        painter.rect_filled(
            button,
            8.0,
            Color32::from_black_alpha(if response.hovered() { 200 } else { 140 }),
        );
        if action == 4 {
            // Vue d'ensemble : coins d'un cadre (pas de glyphe fiable dans la police).
            let r = Rect::from_center_size(button.center(), vec2(14.0, 14.0));
            let stroke = Stroke::new(1.6, Color32::WHITE);
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
                egui::FontId::proportional(17.0),
                Color32::WHITE,
            );
        }
        if response.clicked() {
            match action {
                0 => input.zoom = 1.6,
                1 => input.zoom = 1.0 / 1.6,
                2 => input.rotate = -1,
                3 => input.rotate = 1,
                _ => input.overview = true,
            }
        }
        y += 34.0;
    }
}

fn apply_input(view: &mut ViewState, input: &Input, map: &TopMap, view_size: Vec2) {
    let iso_mode = view.zoom >= ISO_ZOOM;
    if input.overview {
        frame_world(view, map, view_size);
    }
    // Déplacement : le monde suit la souris.
    if input.drag != Vec2::ZERO {
        let d = input.drag / view.zoom;
        let world = if iso_mode {
            // Écran isométrique : (x - z, (x + z) / 2), à l'échelle de la diagonale.
            let (u, v) = (d.x as f64 / 1.414, d.y as f64 / 1.414);
            unrot(view.rotation, [u + 2.0 * v, 2.0 * v - u])
        } else {
            unrot(view.rotation, [d.x as f64, d.y as f64])
        };
        view.center[0] -= world[0];
        view.center[1] -= world[1];
    }
    if input.zoom != 0.0 {
        let old = view.zoom;
        let new = (old * input.zoom).clamp(0.01, MAX_ZOOM);
        // En carte, le point sous le curseur reste fixe.
        if !iso_mode {
            let a = input.zoom_anchor;
            let before = unrot(view.rotation, [(a.x / old) as f64, (a.y / old) as f64]);
            let after = unrot(view.rotation, [(a.x / new) as f64, (a.y / new) as f64]);
            view.center[0] += before[0] - after[0];
            view.center[1] += before[1] - after[1];
        }
        view.zoom = new;
    }
    // Orbite : un quart de tour tous les 90 pixels glissés au clic droit.
    view.orbit_drag += input.orbit;
    while view.orbit_drag > 90.0 {
        view.rotation = (view.rotation + 1) % 4;
        view.orbit_drag -= 90.0;
    }
    while view.orbit_drag < -90.0 {
        view.rotation = (view.rotation + 3) % 4;
        view.orbit_drag += 90.0;
    }
    if input.orbit == 0.0 {
        view.orbit_drag = 0.0;
    }
    match input.rotate {
        1 => view.rotation = (view.rotation + 1) % 4,
        -1 => view.rotation = (view.rotation + 3) % 4,
        _ => {}
    }
    if input.reset_north {
        view.rotation = 0;
    }
    if input.clicked {
        view.selected = view.hovered.clone();
    }
}

/// Barre du bas : position, bloc survolé ou choisi, et remappage.
fn info_bar(app: &mut OdasioApp, ui: &mut Ui, rect: Rect) {
    let p = Palette::of(ui);
    let target = app
        .preview
        .selected
        .clone()
        .or_else(|| app.preview.hovered.clone());
    let selected = app.preview.selected.is_some();
    let mut open = None;
    let mut clear = None;
    let mut deselect = false;
    let frame = egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(theme::RADIUS)
        .inner_margin(egui::Margin::symmetric(16, 0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        frame.show(ui, |ui| {
            ui.set_min_size(vec2(rect.width() - 34.0, rect.height() - 2.0));
            let row = egui::Layout::left_to_right(egui::Align::Center);
            ui.with_layout(row, |ui| {
                let Some(target) = &target else {
                    ui.label(RichText::new(tr("preview.hint")).color(p.muted));
                    return;
                };
                let resolved = app.mapping.resolve(&target.state);
                let position = match target.y {
                    Some(y) => trf(
                        "preview.position",
                        &[("x", &target.x), ("y", &y), ("z", &target.z)],
                    ),
                    None => trf("preview.column", &[("x", &target.x), ("z", &target.z)]),
                };
                ui.label(RichText::new(position).color(p.muted));
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
                if selected {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                }
            });
        });
    });
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
