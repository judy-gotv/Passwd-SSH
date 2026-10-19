//! Themes: named color schemes + font sizes, JSON-serializable.
//! Author: Passwd Word

use egui::Color32;
use serde::{Deserialize, Serialize};

/// A UI theme.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub dark: bool,
    pub bg: [u8; 3],
    pub panel: [u8; 3],
    pub text: [u8; 3],
    pub weak_text: [u8; 3],
    pub accent: [u8; 3],
    pub terminal_bg: [u8; 3],
    pub terminal_fg: [u8; 3],
    pub font_size: f32,
    pub terminal_font_size: f32,
}

impl Theme {
    /// Default dark theme.
    pub fn dark_default() -> Self {
        Self {
            name: "Dark".to_string(),
            dark: true,
            bg: [0x1e, 0x1e, 0x1e],
            panel: [0x25, 0x25, 0x26],
            text: [0xd4, 0xd4, 0xd4],
            weak_text: [0x88, 0x88, 0x88],
            accent: [0x4e, 0xc1, 0xff],
            terminal_bg: [0x0c, 0x0c, 0x0c],
            terminal_fg: [0xcc, 0xcc, 0xcc],
            font_size: 14.0,
            terminal_font_size: 14.0,
        }
    }

    /// Default light theme.
    pub fn light_default() -> Self {
        Self {
            name: "Light".to_string(),
            dark: false,
            bg: [0xf3, 0xf3, 0xf3],
            panel: [0xe8, 0xe8, 0xe8],
            text: [0x1e, 0x1e, 0x1e],
            weak_text: [0x77, 0x77, 0x77],
            accent: [0x00, 0x6e, 0xc1],
            terminal_bg: [0xff, 0xff, 0xff],
            terminal_fg: [0x1e, 0x1e, 0x1e],
            font_size: 14.0,
            terminal_font_size: 14.0,
        }
    }

    /// Built-in theme list.
    pub fn builtins() -> Vec<Theme> {
        vec![Self::dark_default(), Self::light_default()]
    }

    /// Convert to egui visuals.
    pub fn egui_visuals(&self) -> egui::Visuals {
        let mut visuals = if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.panel_fill = rgb(self.panel);
        visuals.window_fill = rgb(self.panel);
        visuals.extreme_bg_color = rgb(self.bg);
        visuals.widgets.noninteractive.bg_fill = rgb(self.bg);
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, rgb(self.text));
        visuals.selection.bg_fill = rgb(self.accent);
        visuals.hyperlink_color = rgb(self.accent);
        visuals
    }

    /// Apply font sizes to the egui style.
    pub fn apply_text_sizes(&self, ctx: &egui::Context) {
        let theme = if self.dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        let font_size = self.font_size;
        ctx.style_mut_of(theme, move |style| {
            let body = egui::FontId::new(font_size, egui::FontFamily::Proportional);
            let mono = egui::FontId::new(font_size, egui::FontFamily::Monospace);
            style
                .text_styles
                .insert(egui::TextStyle::Body, body.clone());
            style
                .text_styles
                .insert(egui::TextStyle::Button, body.clone());
            style.text_styles.insert(
                egui::TextStyle::Heading,
                egui::FontId::new(font_size + 4.0, egui::FontFamily::Proportional),
            );
            style.text_styles.insert(egui::TextStyle::Monospace, mono);
            style.text_styles.insert(
                egui::TextStyle::Small,
                egui::FontId::new((font_size - 2.0).max(9.0), egui::FontFamily::Proportional),
            );
        });
    }

    /// Apply the whole theme to a context.
    pub fn apply(&self, ctx: &egui::Context) {
        ctx.set_theme(if self.dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        });
        ctx.set_visuals(self.egui_visuals());
        self.apply_text_sizes(ctx);
    }

    pub fn bg(&self) -> Color32 {
        rgb(self.bg)
    }
    pub fn panel(&self) -> Color32 {
        rgb(self.panel)
    }
    pub fn text(&self) -> Color32 {
        rgb(self.text)
    }
    pub fn weak_text(&self) -> Color32 {
        rgb(self.weak_text)
    }
    pub fn accent(&self) -> Color32 {
        rgb(self.accent)
    }
    pub fn terminal_bg(&self) -> Color32 {
        rgb(self.terminal_bg)
    }
    pub fn terminal_fg(&self) -> Color32 {
        rgb(self.terminal_fg)
    }
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Terminal default colors for the render model.
pub fn terminal_defaults(
    theme: &Theme,
) -> (opsdeck_terminal::RgbColor, opsdeck_terminal::RgbColor) {
    use opsdeck_terminal::RgbColor;
    (
        RgbColor::new(
            theme.terminal_fg[0],
            theme.terminal_fg[1],
            theme.terminal_fg[2],
        ),
        RgbColor::new(
            theme.terminal_bg[0],
            theme.terminal_bg[1],
            theme.terminal_bg[2],
        ),
    )
}
