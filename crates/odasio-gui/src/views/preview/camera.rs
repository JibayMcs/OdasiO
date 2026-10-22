//! Caméra orbitale libre de l'aperçu, façon globe virtuel : elle tourne autour d'un point
//! visé au sol, s'en éloigne ou s'en approche, et s'incline d'elle-même en se rapprochant
//! (vue du dessus de loin, vue plongeante de près) tant que l'utilisateur ne l'a pas
//! inclinée à la main. Les mouvements sont amortis pour rester fluides.

use glam::{DMat4, DVec3, Mat4, Vec4Swizzles};

pub const FOV: f64 = 50.0_f64.to_radians();
pub const MIN_DISTANCE: f64 = 4.0;
const MIN_PITCH: f64 = 0.12;
pub const TOP_PITCH: f64 = 1.5607;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub target: DVec3,
    /// Azimut : 0 = caméra au sud du point visé, nord en haut de l'écran.
    pub yaw: f64,
    /// Élévation au-dessus de l'horizon, en radians.
    pub pitch: f64,
    pub distance: f64,
}

impl Pose {
    pub fn eye(&self) -> DVec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + DVec3::new(cp * sy, sp, cp * cy) * self.distance
    }

    /// Vecteurs « droite » et « haut de l'écran » projetés au sol.
    pub fn ground_axes(&self) -> (DVec3, DVec3) {
        let (s, c) = self.yaw.sin_cos();
        (DVec3::new(c, 0.0, -s), DVec3::new(-s, 0.0, -c))
    }

    fn lerp(&self, to: &Pose, t: f64) -> Pose {
        // Azimut par le plus court chemin.
        let mut dyaw = (to.yaw - self.yaw) % std::f64::consts::TAU;
        if dyaw > std::f64::consts::PI {
            dyaw -= std::f64::consts::TAU;
        } else if dyaw < -std::f64::consts::PI {
            dyaw += std::f64::consts::TAU;
        }
        Pose {
            target: self.target.lerp(to.target, t),
            yaw: self.yaw + dyaw * t,
            pitch: self.pitch + (to.pitch - self.pitch) * t,
            // Interpolation géométrique : zoom régulier à toutes les échelles.
            distance: self.distance * (to.distance / self.distance).powf(t),
        }
    }
}

pub struct Camera {
    /// Pose affichée, qui rejoint `goal` en douceur.
    pub pose: Pose,
    pub goal: Pose,
    /// L'inclinaison suit la distance (désactivé dès que l'utilisateur incline à la main).
    pub auto_pitch: bool,
    pub max_distance: f64,
}

/// Inclinaison automatique : vue du dessus de loin, plongeante de près.
fn auto_pitch(distance: f64) -> f64 {
    let t = ((distance.ln() - 120f64.ln()) / (1500f64.ln() - 120f64.ln())).clamp(0.0, 1.0);
    0.75 + (TOP_PITCH - 0.75) * t
}

impl Camera {
    pub fn new(target: DVec3, distance: f64) -> Self {
        let pose = Pose {
            target,
            yaw: 0.0,
            pitch: auto_pitch(distance),
            distance,
        };
        Self {
            pose,
            goal: pose,
            auto_pitch: true,
            max_distance: distance * 1.5,
        }
    }

    /// Avance l'animation ; vrai tant que la caméra bouge encore.
    pub fn update(&mut self, dt: f64) -> bool {
        if self.auto_pitch {
            self.goal.pitch = auto_pitch(self.goal.distance);
        }
        let t = 1.0 - (-dt * 14.0).exp();
        let next = self.pose.lerp(&self.goal, t);
        let moving = (next.target - self.goal.target).length()
            > 0.01 * self.goal.distance.min(50.0)
            || (next.distance / self.goal.distance - 1.0).abs() > 0.002
            || (next.yaw - self.goal.yaw).abs() > 0.0005
            || (next.pitch - self.goal.pitch).abs() > 0.0005;
        self.pose = if moving { next } else { self.goal };
        moving
    }

    pub fn matrices(&self, aspect: f64) -> (DMat4, DMat4) {
        let p = &self.pose;
        let near = (p.distance * 0.02).clamp(0.05, 50.0);
        let far = p.distance * 6.0 + 3000.0;
        let view = DMat4::look_at_rh(p.eye(), p.target, DVec3::Y);
        let proj = DMat4::perspective_rh(FOV, aspect, near, far);
        (view, proj)
    }

    /// Distance de brouillard total.
    pub fn fog_distance(&self) -> f64 {
        self.pose.distance * 6.0 + 3000.0
    }

    /// Matrice vue × projection en f32, relative à l'origine du monde.
    pub fn view_proj(&self, aspect: f64) -> Mat4 {
        let (view, proj) = self.matrices(aspect);
        (proj * view).as_mat4()
    }

    /// Rayon (origine, direction) passant par un point de la vue (coordonnées 0..1).
    pub fn ray(&self, aspect: f64, ndc: [f64; 2]) -> (DVec3, DVec3) {
        let (view, proj) = self.matrices(aspect);
        let inverse = (proj * view).inverse();
        let x = ndc[0] * 2.0 - 1.0;
        let y = 1.0 - ndc[1] * 2.0;
        let near = inverse * glam::DVec4::new(x, y, 0.0, 1.0);
        let far = inverse * glam::DVec4::new(x, y, 1.0, 1.0);
        let (near, far) = (near.xyz() / near.w, far.xyz() / far.w);
        (near, (far - near).normalize())
    }

    /// Déplacement : le sol suit la souris (en pixels, pour une vue de `height` pixels).
    pub fn pan(&mut self, delta: [f64; 2], height: f64) {
        let g = &mut self.goal;
        let scale = g.distance * 2.0 * (FOV / 2.0).tan() / height;
        let (right, up) = g.ground_axes();
        // Vue rasante : un pixel vertical couvre plus de terrain.
        let vertical = scale / g.pitch.sin().max(0.35);
        g.target += -right * delta[0] * scale + up * delta[1] * vertical;
    }

    /// Orbite : glisser horizontalement tourne autour du point visé, verticalement incline.
    pub fn orbit(&mut self, delta: [f64; 2]) {
        self.goal.yaw -= delta[0] * 0.006;
        if delta[1] != 0.0 {
            self.auto_pitch = false;
            self.goal.pitch = (self.goal.pitch + delta[1] * 0.005).clamp(MIN_PITCH, TOP_PITCH);
        }
    }

    /// Zoom d'un facteur, en gardant fixe le point `anchor` (au sol, sous la souris).
    pub fn zoom(&mut self, factor: f64, anchor: Option<DVec3>) {
        let g = &mut self.goal;
        let distance = (g.distance * factor).clamp(MIN_DISTANCE, self.max_distance);
        let factor = distance / g.distance;
        if let Some(anchor) = anchor {
            g.target = anchor + (g.target - anchor) * factor;
        }
        g.distance = distance;
    }
}
