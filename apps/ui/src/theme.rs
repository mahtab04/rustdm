use eframe::egui::{self, Color32, FontFamily, FontId, TextStyle};
pub(crate) const BACKGROUND: Color32 = Color32::from_rgb(15, 19, 28);
pub(crate) const SIDEBAR: Color32 = Color32::from_rgb(19, 24, 35);
pub(crate) const SURFACE: Color32 = Color32::from_rgb(25, 32, 46);
pub(crate) const BORDER: Color32 = Color32::from_rgb(43, 54, 73);
pub(crate) const TEXT: Color32 = Color32::from_rgb(231, 237, 249);
pub(crate) const MUTED: Color32 = Color32::from_rgb(143, 158, 181);
pub(crate) const BLUE: Color32 = Color32::from_rgb(89, 142, 255);
pub(crate) const GREEN: Color32 = Color32::from_rgb(70, 207, 163);
pub(crate) const AMBER: Color32 = Color32::from_rgb(242, 190, 99);

pub(crate) fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = BACKGROUND;
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.selection.bg_fill = BLUE.gamma_multiply(0.3);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(33, 43, 61);
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(45, 61, 88);
    style.visuals.widgets.active.bg_fill = BLUE.gamma_multiply(0.5);
    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 10.0);
    style.spacing.interact_size.y = 36.0;
    for (kind, size) in [
        (TextStyle::Body, 15.0),
        (TextStyle::Button, 14.0),
        (TextStyle::Small, 12.0),
        (TextStyle::Heading, 28.0),
    ] {
        style
            .text_styles
            .insert(kind, FontId::new(size, FontFamily::Proportional));
    }
    ctx.set_global_style(style);
    #[cfg(target_os = "windows")]
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("system".into(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "system".into());
        ctx.set_fonts(fonts);
    }
}
