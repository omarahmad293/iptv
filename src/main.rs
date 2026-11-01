#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod data;
mod mpv;
mod ui;

fn main() -> eframe::Result {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1360.0, 820.0]).with_min_inner_size([900.0, 560.0]).with_title("IPTV Player"),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native("IPTV Player", opts, Box::new(|cc| {
        ui::style(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        Ok(Box::new(app::App::new(cc)))
    }))
}
