use eframe::egui;
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(odasio_core::APP_NAME)
            .with_inner_size([1100.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        odasio_core::APP_NAME,
        options,
        Box::new(|_cc| Ok(Box::new(OdasioApp))),
    )
}

#[derive(Default)]
struct OdasioApp;

impl eframe::App for OdasioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(format!(
                "{} {}",
                odasio_core::APP_NAME,
                odasio_core::VERSION
            ));
            ui.label("Convertisseur de mondes Minecraft Java vers Vintage Story.");
        });
    }
}
