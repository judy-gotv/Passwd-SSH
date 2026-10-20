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
    /// Tokyo Night — the default theme.
    pub fn tokyo_night() -> Self {
        Self {
            name: "Tokyo Night".to_string(),
            dark: true,
            bg: [0x1a, 0x1b, 0x26],
            panel: [0x16, 0x16, 0x1e],
            text: [0xc0, 0xca, 0xf5],
            weak_text: [0x9a, 0xa5, 0xce],
            accent: [0x7a, 0xa2, 0xf7],
            terminal_bg: [0x1a, 0x1b, 0x26],
            terminal_fg: [0xc0, 0xca, 0xf5],
            font_size: 14.0,
            terminal_font_size: 14.0,
        }
    }

    /// Catppuccin Mocha.
    pub fn catppuccin_mocha() -> Self {
        Self {
            name: "Catppuccin Mocha".to_string(),
            dark: true,
            bg: [0x1e, 0x1e, 0x2e],
            panel: [0x18, 0x18, 0x25],
            text: [0xcd, 0xd6, 0xf4],
            weak_text: [0xa6, 0xad, 0xc8],
            accent: [0xcb, 0xa6, 0xf7],
            terminal_bg: [0x1e, 0x1e, 0x2e],
            terminal_fg: [0xcd, 0xd6, 0xf4],
            font_size: 14.0,
            terminal_font_size: 14.0,
        }
    }

    /// Nord.
    pub fn nord() -> Self {
        Self {
            name: "Nord".to_string(),
            dark: true,
            bg: [0x2e, 0x34, 0x40],
            panel: [0x26, 0x2c, 0x36],
            text: [0xd8, 0xde, 0xe9],
            weak_text: [0x8a, 0x96, 0xad],
            accent: [0x88, 0xc0, 0xd0],
            terminal_bg: [0x2e, 0x34, 0x40],
            terminal_fg: [0xd8, 0xde, 0xe9],
            font_size: 14.0,
            terminal_font_size: 14.0,
        }
    }

    /// Kanagawa Wave.
    pub fn kanagawa_wave() -> Self {
        Self {
            name: "Kanagawa Wave".to_string(),
            dark: true,
            bg: [0x1f, 0x1f, 0x28],
            panel: [0x16, 0x16, 0x1d],
            text: [0xdc, 0xd7, 0xba],
            weak_text: [0xa0, 0x9e, 0x8f],
            accent: [0x7e, 0x9c, 0xd8],
            terminal_bg: [0x1f, 0x1f, 0x28],
            terminal_fg: [0xdc, 0xd7, 0xba],
            font_size: 14.0,
            terminal_font_size: 14.0,
        }
    }

    /// Everforest Dark.
    pub fn everforest_dark() -> Self {
        Self {
            name: "Everforest Dark".to_string(),
            dark: true,
            bg: [0x2d, 0x35, 0x3b],
            panel: [0x23, 0x2a, 0x2e],
            text: [0xd3, 0xc6, 0xaa],
            weak_text: [0x9d, 0xa6, 0x8f],
            accent: [0xa7, 0xc0, 0x80],
            terminal_bg: [0x2d, 0x35, 0x3b],
            terminal_fg: [0xd3, 0xc6, 0xaa],
            font_size: 14.0,
            terminal_font_size: 14.0,
        }
    }

    /// Light.
    pub fn light() -> Self {
        Self {
            name: "Light".to_string(),
            dark: false,
            bg: [0xf5, 0xf5, 0xf5],
            panel: [0xea, 0xea, 0xea],
            text: [0x1e, 0x1e, 0x1e],
            weak_text: [0x70, 0x70, 0x70],
            accent: [0x00, 0x6e, 0xc1],
            terminal_bg: [0xff, 0xff, 0xff],
            terminal_fg: [0x1e, 0x1e, 0x1e],
            font_size: 14.0,
            terminal_font_size: 14.0,
        }
    }

    /// Light chrome, dark terminal.
    pub fn light_black() -> Self {
        Self {
            name: "Light-Black".to_string(),
            dark: false,
            bg: [0xf7, 0xf7, 0xf7],
            panel: [0xec, 0xec, 0xec],
            text: [0x16, 0x16, 0x16],
            weak_text: [0x6e, 0x6e, 0x6e],
            accent: [0x1a, 0x5c, 0xb8],
            terminal_bg: [0x0c, 0x0c, 0x0c],
            terminal_fg: [0xcc, 0xcc, 0xcc],
            font_size: 14.0,
            terminal_font_size: 14.0,
        }
    }

    /// Legacy dark theme (kept for loading old settings).
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

    /// Legacy light theme (kept for loading old settings).
    pub fn light_default() -> Self {
        Self::light()
    }

    /// Built-in theme list.
    pub fn builtins() -> Vec<Theme> {
        vec![
            Self::tokyo_night(),
            Self::catppuccin_mocha(),
            Self::nord(),
            Self::kanagawa_wave(),
            Self::everforest_dark(),
            Self::light(),
            Self::light_black(),
        ]
    }

    /// Convert to egui visuals.
    pub fn egui_visuals(&self) -> egui::Visuals {
        let mut visuals = if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        let bg = rgb(self.bg);
        let panel = rgb(self.panel);
        let text = rgb(self.text);
        let weak = rgb(self.weak_text);
        let accent = rgb(self.accent);
        // Text drawn on top of a solid accent fill.
        let on_accent = if self.dark { bg } else { Color32::WHITE };
        // Accent blended toward the panel for hover states.
        let accent_hover = mix(accent, panel, 0.78);
        // Faint 1px borders around buttons/inputs.
        let border = Color32::from_rgba_unmultiplied(text.r(), text.g(), text.b(), 26);

        visuals.panel_fill = panel;
        visuals.window_fill = panel;
        visuals.window_stroke = egui::Stroke::new(1.0, border);
        visuals.extreme_bg_color = bg;
        visuals.code_bg_color = bg;
        visuals.faint_bg_color = mix(panel, bg, 0.5);
        visuals.weak_text_color = Some(weak);
        visuals.hyperlink_color = accent;
        visuals.selection.bg_fill = accent;
        visuals.selection.stroke = egui::Stroke::new(1.0, on_accent);
        visuals.window_corner_radius = egui::CornerRadius::same(10);
        visuals.menu_corner_radius = egui::CornerRadius::same(8);

        let rounding = egui::CornerRadius::same(6);
        let w = &mut visuals.widgets;
        w.noninteractive.bg_fill = bg;
        w.noninteractive.weak_bg_fill = bg;
        w.noninteractive.bg_stroke = egui::Stroke::NONE;
        w.noninteractive.fg_stroke = egui::Stroke::new(1.0, text);
        w.noninteractive.corner_radius = rounding;

        w.inactive.bg_fill = mix(panel, bg, 0.45);
        w.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        w.inactive.bg_stroke = egui::Stroke::new(1.0, border);
        w.inactive.fg_stroke = egui::Stroke::new(1.0, text);
        w.inactive.corner_radius = rounding;

        w.hovered.bg_fill = accent_hover;
        w.hovered.weak_bg_fill = egui::Color32::TRANSPARENT;
        w.hovered.bg_stroke = egui::Stroke::new(1.0, accent);
        w.hovered.fg_stroke = egui::Stroke::new(1.5, text);
        w.hovered.corner_radius = rounding;
        w.hovered.expansion = 1.0;

        // NOTE: `RichText::strong()` reads `widgets.active.fg_stroke` as the
        // strong-text color, so it must stay the bright theme text color —
        // a dark "text on accent fill" color would make every heading
        // unreadable on dark themes.
        w.active.bg_fill = mix(accent, panel, 0.45);
        w.active.weak_bg_fill = egui::Color32::TRANSPARENT;
        w.active.bg_stroke = egui::Stroke::new(1.0, accent);
        w.active.fg_stroke = egui::Stroke::new(1.5, text);
        w.active.corner_radius = rounding;
        w.active.expansion = 1.0;

        w.open.bg_fill = accent_hover;
        w.open.weak_bg_fill = egui::Color32::TRANSPARENT;
        w.open.bg_stroke = egui::Stroke::new(1.0, accent);
        w.open.fg_stroke = egui::Stroke::new(1.5, text);
        w.open.corner_radius = rounding;
        w.open.expansion = 1.0;

        visuals
    }

    /// Apply font sizes + spacing polish to the egui style.
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
            // A little more breathing room everywhere.
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(10.0, 6.0);
            style.spacing.indent = 20.0;
            style.spacing.scroll.bar_width = 10.0;
            style.spacing.menu_margin = egui::Margin::same(8);
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

/// Linear blend of two opaque colors: `mix(a, b, t)` → `a*t + b*(1-t)`.
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        (a.r() as f32 * t + b.r() as f32 * (1.0 - t)).round() as u8,
        (a.g() as f32 * t + b.g() as f32 * (1.0 - t)).round() as u8,
        (a.b() as f32 * t + b.b() as f32 * (1.0 - t)).round() as u8,
    )
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
