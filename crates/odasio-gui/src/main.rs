mod app;
mod i18n;
mod recent;
mod task;
mod theme;
mod views;

use eframe::egui;
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    i18n::init();

    let options = eframe::NativeOptions {
        centered: true,
        viewport: egui::ViewportBuilder::default()
            .with_title(odasio_core::APP_NAME)
            .with_app_id("odasio")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([560.0, 480.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!(
                    "../../../packaging/icons/hicolor/256x256/apps/odasio.png"
                ))
                .expect("icône embarquée valide"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        odasio_core::APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(app::OdasioApp::new(cc)))),
    )
}
