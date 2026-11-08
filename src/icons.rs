//! Vector icons drawn with the egui painter (no emoji / icon-font dependency).
use egui::{Color32, FontId, Painter, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Play,
    Pause,
    Prev,
    Next,
    Stop,
    Volume,
    Mute,
    Gear,
    Refresh,
    Grid,
    Close,
    Trash,
    Back,
    Tv,
    Pip,
    Fullscreen,
    Star,
    StarOutline,
    Dot,
}

const ACCENT: Color32 = Color32::from_rgb(99, 102, 241);

fn arc(p: &Painter, c: egui::Pos2, radius: f32, a0: f32, a1: f32, st: Stroke) {
    let n = 28;
    let pts: Vec<egui::Pos2> = (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            pos2(c.x + radius * a.cos(), c.y + radius * a.sin())
        })
        .collect();
    p.add(Shape::line(pts, st));
}

pub fn paint(p: &Painter, icon: Icon, rect: Rect, col: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) / 2.0;
    let w = (r * 0.16).max(1.4);
    let st = Stroke::new(w, col);
    let pt = |x: f32, y: f32| pos2(c.x + x * r, c.y + y * r);
    let tri = |a: (f32, f32), b: (f32, f32), d: (f32, f32)| {
        p.add(Shape::convex_polygon(vec![pt(a.0, a.1), pt(b.0, b.1), pt(d.0, d.1)], col, Stroke::NONE));
    };
    let fill = |x0: f32, y0: f32, x1: f32, y1: f32| {
        p.rect_filled(Rect::from_min_max(pt(x0, y0), pt(x1, y1)), 0.0, col);
    };
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([pt(a.0, a.1), pt(b.0, b.1)], st);
    };
    match icon {
        Icon::Play => tri((-0.4, -0.7), (-0.4, 0.7), (0.75, 0.0)),
        Icon::Pause => {
            fill(-0.55, -0.65, -0.15, 0.65);
            fill(0.15, -0.65, 0.55, 0.65);
        }
        Icon::Prev => {
            fill(-0.7, -0.6, -0.45, 0.6);
            tri((0.7, -0.65), (0.7, 0.65), (-0.35, 0.0));
        }
        Icon::Next => {
            fill(0.45, -0.6, 0.7, 0.6);
            tri((-0.7, -0.65), (-0.7, 0.65), (0.35, 0.0));
        }
        Icon::Stop => fill(-0.55, -0.55, 0.55, 0.55),
        Icon::Volume | Icon::Mute => {
            fill(-0.8, -0.3, -0.4, 0.3);
            p.add(Shape::convex_polygon(vec![pt(-0.4, -0.3), pt(0.05, -0.7), pt(0.05, 0.7), pt(-0.4, 0.3)], col, Stroke::NONE));
            if icon == Icon::Volume {
                arc(p, pt(0.05, 0.0), r * 0.5, -0.8, 0.8, st);
                arc(p, pt(0.05, 0.0), r * 0.85, -0.8, 0.8, st);
            } else {
                line((0.3, -0.3), (0.85, 0.3));
                line((0.3, 0.3), (0.85, -0.3));
            }
        }
        Icon::Gear => {
            p.circle_stroke(c, r * 0.5, Stroke::new(w * 1.4, col));
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let (s, co) = a.sin_cos();
                p.line_segment([pos2(c.x + co * r * 0.55, c.y + s * r * 0.55), pos2(c.x + co * r * 0.9, c.y + s * r * 0.9)], Stroke::new(w * 1.9, col));
            }
        }
        Icon::Refresh => {
            let (a0, a1) = (0.6f32, 0.6 + 4.9);
            arc(p, c, r * 0.62, a0, a1, st);
            let e = pos2(c.x + r * 0.62 * a1.cos(), c.y + r * 0.62 * a1.sin());
            let t = vec2(-a1.sin(), a1.cos());
            let n = vec2(a1.cos(), a1.sin());
            p.add(Shape::convex_polygon(vec![e + t * r * 0.45, e + n * r * 0.38, e - n * r * 0.38], col, Stroke::NONE));
        }
        Icon::Grid => {
            for (x, y) in [(-0.75, -0.75), (0.1, -0.75), (-0.75, 0.1), (0.1, 0.1)] {
                p.rect_filled(Rect::from_min_max(pt(x, y), pt(x + 0.65, y + 0.65)), 1.5, col);
            }
        }
        Icon::Close => {
            line((-0.6, -0.6), (0.6, 0.6));
            line((-0.6, 0.6), (0.6, -0.6));
        }
        Icon::Trash => {
            line((-0.8, -0.5), (0.8, -0.5));
            p.add(Shape::line(vec![pt(-0.25, -0.5), pt(-0.25, -0.8), pt(0.25, -0.8), pt(0.25, -0.5)], st));
            p.add(Shape::line(vec![pt(-0.6, -0.3), pt(-0.5, 0.85), pt(0.5, 0.85), pt(0.6, -0.3)], st));
            line((0.0, -0.1), (0.0, 0.6));
        }
        Icon::Back => {
            line((-0.8, 0.0), (0.8, 0.0));
            line((-0.8, 0.0), (-0.2, -0.6));
            line((-0.8, 0.0), (-0.2, 0.6));
        }
        Icon::Tv => {
            p.rect_stroke(Rect::from_min_max(pt(-0.85, -0.5), pt(0.85, 0.8)), 2.0, st, StrokeKind::Inside);
            line((0.0, -0.5), (-0.4, -0.9));
            line((0.0, -0.5), (0.4, -0.9));
        }
        Icon::Pip => {
            p.rect_stroke(Rect::from_min_max(pt(-0.9, -0.65), pt(0.9, 0.65)), 2.0, st, StrokeKind::Inside);
            fill(0.1, 0.0, 0.7, 0.4);
        }
        Icon::Fullscreen => {
            for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                line((sx * 0.8, sy * 0.8), (sx * 0.8, sy * 0.3));
                line((sx * 0.8, sy * 0.8), (sx * 0.3, sy * 0.8));
            }
        }
        Icon::Star | Icon::StarOutline => {
            let pts: Vec<egui::Pos2> = (0..10)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                    let rr = if i % 2 == 0 { r * 0.8 } else { r * 0.36 };
                    pos2(c.x + rr * a.cos(), c.y + rr * a.sin())
                })
                .collect();
            if icon == Icon::Star {
                // The star is concave, so fill it as a triangle fan from the centre.
                let mut mesh = egui::Mesh::default();
                mesh.colored_vertex(c, col);
                for q in &pts { mesh.colored_vertex(*q, col); }
                for i in 0..10u32 { mesh.add_triangle(0, 1 + i, 1 + (i + 1) % 10); }
                p.add(Shape::mesh(mesh));
                p.add(Shape::closed_line(pts, Stroke::new(1.0, col)));
            } else {
                p.add(Shape::closed_line(pts, Stroke::new(1.5, col)));
            }
        }
        Icon::Dot => {
            p.circle_filled(c, r * 0.5, col);
        }
    }
}

fn chrome(ui: &Ui, resp: &Response, rect: Rect, selected: bool) -> Color32 {
    let v = ui.style().interact(resp);
    if selected {
        ui.painter().rect_filled(rect, v.corner_radius, ACCENT.gamma_multiply(if resp.hovered() { 1.0 } else { 0.85 }));
        Color32::WHITE
    } else {
        if resp.hovered() || resp.has_focus() {
            ui.painter().rect_filled(rect, v.corner_radius, v.weak_bg_fill);
        }
        v.fg_stroke.color
    }
}

/// Square icon-only button.
pub fn button(ui: &mut Ui, icon: Icon, size: f32, selected: bool, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size + 16.0, size + 12.0), Sense::click());
    let fg = chrome(ui, &resp, rect, selected);
    paint(ui.painter(), icon, Rect::from_center_size(rect.center(), Vec2::splat(size)), fg);
    resp.on_hover_text(tip)
}

/// Button with a leading icon and a text label.
pub fn text_button(ui: &mut Ui, icon: Icon, text: &str, selected: bool) -> Response {
    let font = FontId::proportional(14.0);
    let g = ui.painter().layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER);
    let isz = 15.0;
    let size = vec2(10.0 + isz + 6.0 + g.size().x + 12.0, 28.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fg = chrome(ui, &resp, rect, selected);
    let ir = Rect::from_center_size(egui::pos2(rect.min.x + 10.0 + isz / 2.0, rect.center().y), Vec2::splat(isz));
    paint(ui.painter(), icon, ir, fg);
    ui.painter().galley(egui::pos2(ir.max.x + 6.0, rect.center().y - g.size().y / 2.0), g, fg);
    resp
}
