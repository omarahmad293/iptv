#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod data;
mod icons;
mod mpv;
mod ui;

fn main() -> eframe::Result {
    std::panic::set_hook(Box::new(|info| {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(data::data_dir().join("crash.log")) {
            let _ = writeln!(f, "[{}] {info}", data::now_ms());
        }
    }));
    let saved = data::cache_read::<app::Settings>("settings").map(|x| x.1).unwrap_or_default();
    let mut vp = egui::ViewportBuilder::default().with_min_inner_size([900.0, 560.0]).with_title("IPTV Player");
    vp = match saved.win {
        Some([x, y, w, h]) if w >= 900.0 && h >= 560.0 => vp.with_inner_size([w, h]).with_position([x, y]),
        _ => vp.with_inner_size([1360.0, 820.0]),
    };
    if saved.win_maximized {
        vp = vp.with_maximized(true);
    }
    let opts = eframe::NativeOptions { viewport: vp, renderer: eframe::Renderer::Glow, ..Default::default() };
    eframe::run_native("IPTV Player", opts, Box::new(|cc| {
        ui::style(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        Ok(Box::new(app::App::new(cc)))
    }))
}
