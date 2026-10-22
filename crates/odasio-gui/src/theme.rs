//! Identité visuelle : palette chaude inspirée de Vintage Story, en sombre et en clair,
//! et composants réutilisables (cartes, badges, boutons).

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Shadow, Stroke, TextStyle,
    Theme, Ui, Vec2,
};
use odasio_core::Quality;

pub const RADIUS: u8 = 10;
pub const GAP: f32 = 12.0;
/// Largeur maximale du contenu : au-delà, il est centré.
pub const CONTENT_MAX: f32 = 1040.0;

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
}

impl Palette {
    pub const DARK: Palette = Palette {
        bg: Color32::from_rgb(0x14, 0x13, 0x11),
        panel: Color32::from_rgb(0x1b, 0x1a, 0x17),
        card: Color32::from_rgb(0x23, 0x21, 0x1d),
        card_hover: Color32::from_rgb(0x2c, 0x29, 0x24),
        border: Color32::from_rgb(0x38, 0x34, 0x2d),
        text: Color32::from_rgb(0xec, 0xe6, 0xdb),
        muted: Color32::from_rgb(0x9d, 0x95, 0x88),
        accent: Color32::from_rgb(0xd9, 0x9a, 0x3e),
        accent_text: Color32::from_rgb(0x1a, 0x14, 0x0b),
        success: Color32::from_rgb(0x7c, 0xb8, 0x6a),
        warning: Color32::from_rgb(0xe0, 0xb0, 0x4a),
        danger: Color32::from_rgb(0xd8, 0x6a, 0x5a),
    };

    pub const LIGHT: Palette = Palette {
        bg: Color32::from_rgb(0xf6, 0xf2, 0xea),
        panel: Color32::from_rgb(0xef, 0xe9, 0xdd),
        card: Color32::from_rgb(0xff, 0xfd, 0xf8),
        card_hover: Color32::from_rgb(0xf8, 0xf3, 0xe8),
        border: Color32::from_rgb(0xdd, 0xd3, 0xc2),
        text: Color32::from_rgb(0x2a, 0x25, 0x1e),
        muted: Color32::from_rgb(0x7a, 0x70, 0x62),
        accent: Color32::from_rgb(0xb5, 0x72, 0x1c),
        accent_text: Color32::from_rgb(0xff, 0xfb, 0xf2),
        success: Color32::from_rgb(0x4b, 0x8a, 0x3a),
        warning: Color32::from_rgb(0xa8, 0x77, 0x10),
        danger: Color32::from_rgb(0xb4, 0x45, 0x36),
    };

    pub fn of(ui: &Ui) -> Palette {
        Self::for_dark(ui.visuals().dark_mode)
    }

    pub fn of_ctx(ctx: &egui::Context) -> Palette {
        Self::for_dark(ctx.style().visuals.dark_mode)
    }

    fn for_dark(dark: bool) -> Palette {
        if dark { Self::DARK } else { Self::LIGHT }
    }

    pub fn quality(&self, quality: Quality) -> Color32 {
        match quality {
            Quality::Exact => self.success,
            Quality::Equivalent => Color32::from_rgb(0x5a, 0xa8, 0xa0),
            Quality::Lossy => self.warning,
            Quality::Substitute => Color32::from_rgb(0xd0, 0x84, 0x3c),
            Quality::Removed => self.danger,
            Quality::Unmapped => Color32::from_rgb(0x9a, 0x7c, 0xc8),
        }
    }
}

fn visuals(p: Palette, dark: bool) -> egui::Visuals {
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.extreme_bg_color = p.panel;
    v.faint_bg_color = p.card_hover;
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.window_corner_radius = CornerRadius::same(RADIUS);
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_shadow = Shadow {
        offset: [0, 6],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 110 } else { 40 }),
    };
    v.popup_shadow = v.window_shadow;

    let radius = CornerRadius::same(8);
    for (w, fill) in [
        (&mut v.widgets.noninteractive, p.card),
        (&mut v.widgets.inactive, p.card_hover),
        (&mut v.widgets.hovered, p.border),
        (&mut v.widgets.active, p.border),
        (&mut v.widgets.open, p.card_hover),
    ] {
        w.corner_radius = radius;
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent.gamma_multiply(0.7));
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, p.text);
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.5, p.text);
    v
}

pub fn install(ctx: &egui::Context) {
    ctx.set_visuals_of(Theme::Dark, visuals(Palette::DARK, true));
    ctx.set_visuals_of(Theme::Light, visuals(Palette::LIGHT, false));
    for theme in [Theme::Dark, Theme::Light] {
        ctx.style_mut_of(theme, |style| {
            style.text_styles = [
                (
                    TextStyle::Small,
                    FontId::new(12.0, FontFamily::Proportional),
                ),
                (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
                (
                    TextStyle::Button,
                    FontId::new(15.0, FontFamily::Proportional),
                ),
                (
                    TextStyle::Heading,
                    FontId::new(26.0, FontFamily::Proportional),
                ),
                (
                    TextStyle::Monospace,
                    FontId::new(13.5, FontFamily::Monospace),
                ),
            ]
            .into();
            style.spacing.item_spacing = Vec2::new(10.0, 8.0);
            style.spacing.button_padding = Vec2::new(14.0, 7.0);
            style.spacing.interact_size.y = 32.0;
            style.spacing.scroll.bar_width = 8.0;
        });
    }
}

/// Carte : bloc de contenu en relief.
pub fn card(ui: &Ui) -> egui::Frame {
    let p = Palette::of(ui);
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::same(18))
}

/// Titre de section avec sous-titre discret.
pub fn section(ui: &mut Ui, title: &str, subtitle: &str) {
    let p = Palette::of(ui);
    ui.label(RichText::new(title).size(19.0).strong());
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).color(p.muted));
    }
    ui.add_space(6.0);
}

/// Pastille colorée avec libellé.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(9, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.5).color(color));
        });
}

pub fn primary_button(ui: &mut Ui, text: &str, enabled: bool) -> egui::Response {
    let p = Palette::of(ui);
    let fill = if enabled {
        p.accent
    } else {
        p.accent.gamma_multiply(0.35)
    };
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).strong().color(p.accent_text))
            .fill(fill)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(8))
            .min_size(Vec2::new(140.0, 36.0)),
    )
}

pub fn secondary_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(text).min_size(Vec2::new(110.0, 36.0)))
}

/// Statistique : grande valeur et libellé.
pub fn stat(ui: &mut Ui, value: &str, label: &str) {
    let p = Palette::of(ui);
    ui.vertical(|ui| {
        ui.label(RichText::new(value).size(24.0).strong());
        ui.label(RichText::new(label).size(12.5).color(p.muted));
    });
}

/// Nombre lisible : 1 234 567.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Pourcentage adaptatif : les petites parts restent visibles.
pub fn percent(part: u64, total: u64) -> String {
    if total == 0 {
        return "0 %".into();
    }
    let v = part as f64 * 100.0 / total as f64;
    match v {
        0.0 => "0 %".into(),
        v if v < 0.01 => "< 0,01 %".into(),
        v if v < 10.0 => format!("{v:.2} %").replace('.', ","),
        v => format!("{v:.1} %").replace('.', ","),
    }
}
