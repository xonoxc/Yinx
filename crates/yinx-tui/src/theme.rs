use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use ratatui::style::{Color, Modifier};
use ratatui::widgets::BorderType as TuiBorderType;

pub fn relative_luminance(color: Color) -> f64 {
    let (r, g, b) = match color {
        Color::Rgb(r, g, b) => (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0),
        _ => return 0.5,
    };
    let linearize = |c: f64| -> f64 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linearize(r) + 0.7152 * linearize(g) + 0.0722 * linearize(b)
}

pub fn is_dark(color: Color) -> bool {
    relative_luminance(color) < 0.3
}

#[derive(Debug, Clone, PartialEq)]
pub struct DynamicTheme {
    pub bg_base: Color,
    pub bg_elevated: Color,
    pub bg_subtle: Color,
    pub fg: Color,
    pub fg_muted: Color,
    pub border_muted: Color,
    pub border_focus: Color,
    pub accent: Color,
    pub accent_dim: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub selection_bg: Color,
    pub panel_focus: Color,
}

impl DynamicTheme {
    pub fn from_terminal(bg: Color, fg: Color, cursor: Color) -> Self {
        let bg = match bg {
            Color::Rgb(_, _, _) => bg,
            _ => Color::Rgb(18, 18, 22),
        };
        let fg = match fg {
            Color::Rgb(_, _, _) => fg,
            _ => Color::Rgb(220, 220, 225),
        };
        let dark = is_dark(bg);

        let blend = |a: Color, b: Color, t: f64| -> Color {
            let (ar, ag, ab) = match a { Color::Rgb(r, g, b) => (r, g, b), _ => return a };
            let (br, bg, bb) = match b { Color::Rgb(r, g, b) => (r, g, b), _ => return b };
            Color::Rgb(
                (ar as f64 + (br as f64 - ar as f64) * t) as u8,
                (ag as f64 + (bg as f64 - ag as f64) * t) as u8,
                (ab as f64 + (bb as f64 - ab as f64) * t) as u8,
            )
        };

        let lighten = |c: Color, amt: f64| -> Color { blend(c, Color::Rgb(255, 255, 255), amt) };
        let darken = |c: Color, amt: f64| -> Color { blend(c, Color::Rgb(0, 0, 0), amt) };

        let bg_base = bg;
        let bg_elevated = if dark { lighten(bg, 0.08) } else { darken(bg, 0.08) };
        let bg_subtle = if dark { lighten(bg, 0.04) } else { darken(bg, 0.04) };
        let accent = cursor;

        Self {
            bg_base,
            bg_elevated,
            bg_subtle,
            fg,
            fg_muted: blend(bg, fg, 0.3),
            border_muted: blend(bg, fg, 0.15),
            border_focus: accent,
            accent,
            accent_dim: if dark { darken(accent, 0.3) } else { lighten(accent, 0.3) },
            success: if dark { Color::Rgb(80, 200, 120) } else { Color::Rgb(50, 160, 90) },
            warning: Color::Rgb(255, 184, 108),
            error: Color::Rgb(220, 50, 47),
            selection_bg: blend(bg_elevated, accent, 0.3),
            panel_focus: accent,
        }
    }

    pub fn is_dark(&self) -> bool {
        is_dark(self.bg_base)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub background: Option<ColorDef>,
    pub foreground: ColorDef,
    pub border: BorderStyle,
    pub highlight: HighlightStyle,
    pub semantic: SemanticColors,
    pub pane: PaneColors,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorDef {
    Rgb(u8, u8, u8),
    Indexed(u8),
    Reset,
}

impl ColorDef {
    pub fn as_color(&self) -> Color {
        match self {
            ColorDef::Rgb(r, g, b) => Color::Rgb(*r, *g, *b),
            ColorDef::Indexed(index) => Color::Indexed(*index),
            ColorDef::Reset => Color::Reset,
        }
    }

    pub fn from_color(color: Color) -> Option<Self> {
        match color {
            Color::Rgb(r, g, b) => Some(Self::Rgb(r, g, b)),
            Color::Indexed(index) => Some(Self::Indexed(index)),
            Color::Reset => Some(Self::Reset),
            _ => None,
        }
    }

    pub const BLACK: ColorDef = ColorDef::Rgb(0, 0, 0);
    pub const WHITE: ColorDef = ColorDef::Rgb(255, 255, 255);
    pub const RED: ColorDef = ColorDef::Rgb(220, 50, 47);
    pub const GREEN: ColorDef = ColorDef::Rgb(80, 200, 120);
    pub const YELLOW: ColorDef = ColorDef::Rgb(255, 184, 108);
    pub const BLUE: ColorDef = ColorDef::Rgb(97, 175, 239);
    pub const MAGENTA: ColorDef = ColorDef::Rgb(198, 120, 221);
    pub const CYAN: ColorDef = ColorDef::Rgb(86, 182, 194);
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BorderStyle {
    pub color: ColorDef,
    pub active_color: ColorDef,
    pub style: BorderType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BorderType {
    Plain,
    Rounded,
    Double,
    Thick,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HighlightStyle {
    pub bg: ColorDef,
    pub fg: ColorDef,
    pub selected_bg: ColorDef,
    pub selected_fg: ColorDef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticColors {
    pub success: ColorDef,
    pub error: ColorDef,
    pub warning: ColorDef,
    pub info: ColorDef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypographyLevels {
    pub title: ColorDef,
    pub heading: ColorDef,
    pub body: ColorDef,
    pub caption: ColorDef,
    pub dim: ColorDef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneColors {
    pub background: Option<ColorDef>,
    pub active_background: Option<ColorDef>,
    pub inactive_background: Option<ColorDef>,
    pub subtle_background: Option<ColorDef>,
    pub title: ColorDef,
    pub inactive_title: ColorDef,
    pub muted: ColorDef,
    pub placeholder: ColorDef,
    pub status_bar_bg: ColorDef,
    pub status_bar_fg: ColorDef,
    pub typography: TypographyLevels,
}

impl PaneColors {
    pub fn bg_color(&self) -> Color {
        self.background
            .as_ref()
            .map(ColorDef::as_color)
            .unwrap_or(Color::Reset)
    }

    pub fn bg_for(&self, is_active: bool) -> Color {
        let preferred = if is_active {
            self.active_background.as_ref()
        } else {
            self.inactive_background.as_ref()
        };

        preferred
            .or(self.background.as_ref())
            .map(ColorDef::as_color)
            .unwrap_or(Color::Reset)
    }

    pub fn subtle_bg_color(&self) -> Color {
        self.subtle_background
            .as_ref()
            .map(ColorDef::as_color)
            .or_else(|| self.background.as_ref().map(ColorDef::as_color))
            .unwrap_or(Color::Reset)
    }
}

pub struct ThemeRegistry {
    themes: BTreeMap<String, Theme>,
    order: Vec<String>,
    current: String,
}

impl ThemeRegistry {
    pub fn new() -> Self {
        Self {
            themes: BTreeMap::new(),
            order: Vec::new(),
            current: String::new(),
        }
    }

    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        for theme in Theme::builtin_themes() {
            registry.register(theme.name.clone(), theme);
        }
        registry.set_current("terminal");
        registry
    }

    pub fn len(&self) -> usize {
        self.themes.len()
    }

    pub fn register(&mut self, name: String, theme: Theme) {
        if !self.themes.contains_key(&name) {
            self.order.push(name.clone());
        }
        self.themes.insert(name, theme);
    }

    pub fn names(&self) -> Vec<String> {
        self.order.clone()
    }

    pub fn get(&self, name: &str) -> Option<&Theme> {
        self.themes.get(name)
    }

    pub fn set_current(&mut self, name: &str) {
        if self.themes.contains_key(name) {
            self.current = name.to_string();
        }
    }

    pub fn current(&self) -> Option<&Theme> {
        if self.current.is_empty() {
            None
        } else {
            self.themes.get(&self.current)
        }
    }

    pub fn cycle_next(&mut self) -> &Theme {
        if self.order.is_empty() {
            panic!("No themes registered");
        }

        if self.current.is_empty() {
            self.current = self.order[0].clone();
        } else {
            let current_idx = self
                .order
                .iter()
                .position(|name| name == &self.current)
                .unwrap_or(0);
            let next_idx = (current_idx + 1) % self.order.len();
            self.current = self.order[next_idx].clone();
        }

        self.themes
            .get(&self.current)
            .expect("current theme should exist")
    }
}

impl Theme {
    pub fn terminal_default() -> Self {
        let dt = DynamicTheme::from_terminal(
            Color::Reset,
            Color::Reset,
            Color::Rgb(150, 200, 255),
        );
        Self {
            name: "terminal".to_string(),
            background: None,
            foreground: ColorDef::Reset,
            border: BorderStyle {
                color: ColorDef::from_color(dt.border_muted).unwrap_or(ColorDef::Reset),
                active_color: ColorDef::from_color(dt.border_focus).unwrap_or(ColorDef::Reset),
                style: BorderType::Plain,
            },
            highlight: HighlightStyle {
                bg: ColorDef::Reset,
                fg: ColorDef::Reset,
                selected_bg: ColorDef::from_color(dt.selection_bg).unwrap_or(ColorDef::Reset),
                selected_fg: ColorDef::Reset,
            },
            semantic: SemanticColors {
                success: ColorDef::from_color(dt.success).unwrap_or(ColorDef::Reset),
                error: ColorDef::from_color(dt.error).unwrap_or(ColorDef::Reset),
                warning: ColorDef::from_color(dt.warning).unwrap_or(ColorDef::Reset),
                info: ColorDef::from_color(dt.border_focus).unwrap_or(ColorDef::Reset),
            },
            pane: PaneColors {
                background: None,
                active_background: None,
                inactive_background: None,
                subtle_background: None,
                title: ColorDef::from_color(dt.border_focus).unwrap_or(ColorDef::Reset),
                inactive_title: ColorDef::from_color(dt.fg_muted).unwrap_or(ColorDef::Reset),
                muted: ColorDef::from_color(dt.fg_muted).unwrap_or(ColorDef::Reset),
                placeholder: ColorDef::from_color(dt.fg_muted).unwrap_or(ColorDef::Reset),
                status_bar_bg: ColorDef::Reset,
                status_bar_fg: ColorDef::Reset,
                typography: TypographyLevels {
                    title: ColorDef::from_color(dt.border_focus).unwrap_or(ColorDef::Reset),
                    heading: ColorDef::from_color(dt.fg_muted).unwrap_or(ColorDef::Reset),
                    body: ColorDef::Reset,
                    caption: ColorDef::from_color(dt.fg_muted).unwrap_or(ColorDef::Reset),
                    dim: ColorDef::from_color(dt.fg_muted).unwrap_or(ColorDef::Reset),
                },
            },
        }
    }

    pub fn dark() -> Self {
        Self {
            name: "dark".to_string(),
            background: Some(ColorDef::Rgb(13, 16, 22)),
            foreground: ColorDef::Rgb(224, 230, 238),
            border: BorderStyle {
                color: ColorDef::Rgb(82, 96, 116),
                active_color: ColorDef::Rgb(150, 198, 255),
                style: BorderType::Plain,
            },
            highlight: HighlightStyle {
                bg: ColorDef::Rgb(30, 38, 50),
                fg: ColorDef::Rgb(230, 235, 242),
                selected_bg: ColorDef::Rgb(78, 118, 178),
                selected_fg: ColorDef::WHITE,
            },
            semantic: SemanticColors {
                success: ColorDef::Rgb(104, 187, 129),
                error: ColorDef::Rgb(235, 120, 120),
                warning: ColorDef::Rgb(232, 196, 106),
                info: ColorDef::Rgb(125, 185, 235),
            },
            pane: PaneColors {
                background: Some(ColorDef::Rgb(16, 20, 28)),
                active_background: Some(ColorDef::Rgb(19, 25, 34)),
                inactive_background: Some(ColorDef::Rgb(14, 18, 25)),
                subtle_background: Some(ColorDef::Rgb(22, 28, 38)),
                title: ColorDef::Rgb(200, 220, 245),
                inactive_title: ColorDef::Rgb(170, 183, 205),
                muted: ColorDef::Rgb(182, 195, 215),
                placeholder: ColorDef::Rgb(168, 180, 198),
                status_bar_bg: ColorDef::Rgb(18, 23, 32),
                status_bar_fg: ColorDef::Rgb(224, 230, 238),
                typography: TypographyLevels {
                    title: ColorDef::Rgb(200, 220, 245),
                    heading: ColorDef::Rgb(180, 192, 210),
                    body: ColorDef::Rgb(224, 230, 238),
                    caption: ColorDef::Rgb(150, 165, 185),
                    dim: ColorDef::Rgb(80, 95, 115),
                },
            },
        }
    }

    pub fn postman() -> Self {
        Self {
            name: "postman".to_string(),
            background: None,
            foreground: ColorDef::Reset,
            border: BorderStyle {
                color: ColorDef::Indexed(242),
                active_color: ColorDef::Indexed(6),
                style: BorderType::Plain,
            },
            highlight: HighlightStyle {
                bg: ColorDef::Reset,
                fg: ColorDef::Reset,
                selected_bg: ColorDef::Indexed(6),
                selected_fg: ColorDef::Indexed(15),
            },
            semantic: SemanticColors {
                success: ColorDef::Indexed(2),
                error: ColorDef::Indexed(1),
                warning: ColorDef::Indexed(3),
                info: ColorDef::Indexed(4),
            },
            pane: PaneColors {
                background: None,
                active_background: None,
                inactive_background: None,
                subtle_background: None,
                title: ColorDef::Indexed(6),
                inactive_title: ColorDef::Indexed(248),
                muted: ColorDef::Indexed(248),
                placeholder: ColorDef::Indexed(250),
                status_bar_bg: ColorDef::Reset,
                status_bar_fg: ColorDef::Reset,
                typography: TypographyLevels {
                    title: ColorDef::Indexed(6),
                    heading: ColorDef::Indexed(250),
                    body: ColorDef::Indexed(252),
                    caption: ColorDef::Indexed(245),
                    dim: ColorDef::Indexed(240),
                },
            },
        }
    }

    pub fn light() -> Self {
        Self {
            name: "light".to_string(),
            background: Some(ColorDef::Rgb(248, 245, 239)),
            foreground: ColorDef::Rgb(48, 43, 37),
            border: BorderStyle {
                color: ColorDef::Rgb(187, 177, 165),
                active_color: ColorDef::Rgb(119, 131, 196),
                style: BorderType::Plain,
            },
            highlight: HighlightStyle {
                bg: ColorDef::Rgb(237, 231, 223),
                fg: ColorDef::Rgb(54, 48, 41),
                selected_bg: ColorDef::Rgb(119, 131, 196),
                selected_fg: ColorDef::Rgb(252, 251, 248),
            },
            semantic: SemanticColors {
                success: ColorDef::Rgb(63, 136, 93),
                error: ColorDef::Rgb(187, 74, 86),
                warning: ColorDef::Rgb(175, 125, 52),
                info: ColorDef::Rgb(86, 111, 177),
            },
            pane: PaneColors {
                background: Some(ColorDef::Rgb(252, 250, 246)),
                active_background: Some(ColorDef::Rgb(255, 253, 249)),
                inactive_background: Some(ColorDef::Rgb(246, 242, 236)),
                subtle_background: Some(ColorDef::Rgb(239, 234, 227)),
                title: ColorDef::Rgb(85, 97, 150),
                inactive_title: ColorDef::Rgb(143, 132, 121),
                muted: ColorDef::Rgb(135, 124, 112),
                placeholder: ColorDef::Rgb(152, 142, 131),
                status_bar_bg: ColorDef::Rgb(235, 229, 221),
                status_bar_fg: ColorDef::Rgb(48, 43, 37),
                typography: TypographyLevels {
                    title: ColorDef::Rgb(85, 97, 150),
                    heading: ColorDef::Rgb(115, 105, 95),
                    body: ColorDef::Rgb(48, 43, 37),
                    caption: ColorDef::Rgb(140, 130, 120),
                    dim: ColorDef::Rgb(175, 165, 155),
                },
            },
        }
    }

    pub fn forest() -> Self {
        Self {
            name: "forest".to_string(),
            background: Some(ColorDef::Rgb(14, 20, 17)),
            foreground: ColorDef::Rgb(222, 230, 221),
            border: BorderStyle {
                color: ColorDef::Rgb(84, 106, 92),
                active_color: ColorDef::Rgb(140, 210, 172),
                style: BorderType::Plain,
            },
            highlight: HighlightStyle {
                bg: ColorDef::Rgb(30, 44, 37),
                fg: ColorDef::Rgb(228, 235, 225),
                selected_bg: ColorDef::Rgb(73, 120, 98),
                selected_fg: ColorDef::WHITE,
            },
            semantic: SemanticColors {
                success: ColorDef::Rgb(117, 191, 136),
                error: ColorDef::Rgb(209, 104, 104),
                warning: ColorDef::Rgb(209, 180, 107),
                info: ColorDef::Rgb(116, 188, 182),
            },
            pane: PaneColors {
                background: Some(ColorDef::Rgb(17, 25, 21)),
                active_background: Some(ColorDef::Rgb(20, 29, 24)),
                inactive_background: Some(ColorDef::Rgb(14, 22, 18)),
                subtle_background: Some(ColorDef::Rgb(22, 32, 27)),
                title: ColorDef::Rgb(182, 222, 198),
                inactive_title: ColorDef::Rgb(172, 188, 176),
                muted: ColorDef::Rgb(182, 198, 188),
                placeholder: ColorDef::Rgb(168, 182, 170),
                status_bar_bg: ColorDef::Rgb(18, 27, 22),
                status_bar_fg: ColorDef::Rgb(222, 230, 221),
                typography: TypographyLevels {
                    title: ColorDef::Rgb(182, 222, 198),
                    heading: ColorDef::Rgb(182, 198, 188),
                    body: ColorDef::Rgb(222, 230, 221),
                    caption: ColorDef::Rgb(160, 178, 165),
                    dim: ColorDef::Rgb(85, 105, 90),
                },
            },
        }
    }

    pub fn builtin_themes() -> Vec<Self> {
        vec![
            Self::postman(),
            Self::dark(),
            Self::light(),
            Self::forest(),
            Self::terminal_default(),
        ]
    }

    pub fn builtin_theme_names() -> Vec<String> {
        Self::builtin_themes()
            .into_iter()
            .map(|theme| theme.name)
            .collect()
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ThemeError> {
        let content = fs::read_to_string(&path)?;
        let theme: Theme = serde_json::from_str(&content)?;
        Ok(theme)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), ThemeError> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    pub fn get_by_name(name: &str) -> Option<Self> {
        match name {
            "postman" => Some(Self::postman()),
            "terminal" | "terminal_default" => Some(Self::terminal_default()),
            "dark" => Some(Self::dark()),
            "light" => Some(Self::light()),
            "forest" => Some(Self::forest()),
            _ => None,
        }
    }

    pub fn border_color(&self, is_active: bool) -> Color {
        if is_active {
            self.border.active_color.as_color()
        } else {
            self.border.color.as_color()
        }
    }

    pub fn pane_bg(&self, is_active: bool) -> Color {
        self.pane.bg_for(is_active)
    }

    pub fn subtle_bg(&self) -> Color {
        self.pane.subtle_bg_color()
    }

    pub fn title_color(&self, is_active: bool) -> Color {
        if is_active {
            self.pane.title.as_color()
        } else {
            self.pane.inactive_title.as_color()
        }
    }

    pub fn muted_color(&self) -> Color {
        self.pane.muted.as_color()
    }

    pub fn placeholder_color(&self) -> Color {
        self.pane.placeholder.as_color()
    }

    pub fn typography_level(&self, level: u8) -> (Color, Modifier) {
        match level {
            0 => (self.pane.typography.title.as_color(), Modifier::BOLD),
            1 => (self.pane.typography.heading.as_color(), Modifier::BOLD),
            2 => (self.pane.typography.body.as_color(), Modifier::empty()),
            3 => (self.pane.typography.caption.as_color(), Modifier::DIM),
            _ => (self.pane.typography.dim.as_color(), Modifier::DIM),
        }
    }

    pub fn dim_border_color(&self) -> Color {
        let mut c = self.border.color.as_color();
        if let Some(bg) = &self.pane.background {
            let bg_c = bg.as_color();
            if let (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) = (c, bg_c) {
                c = Color::Rgb(
                    (r1 as u16 * 1 + r2 as u16 * 2 / 3) as u8,
                    (g1 as u16 * 1 + g2 as u16 * 2 / 3) as u8,
                    (b1 as u16 * 1 + b2 as u16 * 2 / 3) as u8,
                );
            }
        }
        c
    }

    pub fn bg_panel(&self) -> Color {
        self.pane.bg_color()
    }

    pub fn bg_element(&self) -> Color {
        self.subtle_bg()
    }

    pub fn text_muted(&self) -> Color {
        self.muted_color()
    }

    pub fn border_subtle(&self) -> Color {
        self.dim_border_color()
    }

    pub fn section_title(&self) -> Color {
        self.pane.title.as_color()
    }

    pub fn section_title_inactive(&self) -> Color {
        self.pane.inactive_title.as_color()
    }

    pub fn tui_border_type(&self) -> TuiBorderType {
        match self.border.style {
            BorderType::Plain => TuiBorderType::Plain,
            BorderType::Rounded => TuiBorderType::Rounded,
            BorderType::Double => TuiBorderType::Double,
            BorderType::Thick => TuiBorderType::Thick,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Theme not found: {0}")]
    NotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_theme_uses_derived_colors() {
        let theme = Theme::terminal_default();
        assert_eq!(theme.name, "terminal");
        assert!(theme.background.is_none());
        assert_eq!(theme.foreground, ColorDef::Reset);
        assert_ne!(theme.border.color, ColorDef::Reset);
    }

    #[test]
    fn test_dark_theme_semantic_colors() {
        let theme = Theme::dark();
        match theme.semantic.success {
            ColorDef::Rgb(_, g, _) => assert_eq!(g, 187),
            _ => panic!("expected rgb"),
        }
    }

    #[test]
    fn test_light_theme_background() {
        let theme = Theme::light();
        match theme.background {
            Some(ColorDef::Rgb(r, g, b)) => {
                assert_eq!((r, g, b), (248, 245, 239));
            }
            _ => panic!("expected rgb background"),
        }
    }

    #[test]
    fn test_forest_theme_exists() {
        let theme = Theme::forest();
        assert_eq!(theme.name, "forest");
    }

    #[test]
    fn test_postman_theme_exists() {
        let theme = Theme::postman();
        assert_eq!(theme.name, "postman");
        assert_eq!(theme.tui_border_type(), TuiBorderType::Plain);
    }

    #[test]
    fn test_color_def_as_color_supports_indexed() {
        assert_eq!(ColorDef::Indexed(8).as_color(), Color::Indexed(8));
    }

    #[test]
    fn test_color_def_from_color_supports_indexed() {
        assert_eq!(
            ColorDef::from_color(Color::Indexed(6)),
            Some(ColorDef::Indexed(6))
        );
    }

    #[test]
    fn test_serialize_deserialize_theme() {
        let theme = Theme::dark();
        let serialized = serde_json::to_string(&theme).unwrap();
        let deserialized: Theme = serde_json::from_str(&serialized).unwrap();
        assert_eq!(theme, deserialized);
    }

    #[test]
    fn test_save_and_load_theme() {
        let theme = Theme::forest();
        let path = std::env::temp_dir().join("test_theme.json");
        theme.save_to_file(&path).unwrap();
        let loaded = Theme::load_from_file(&path).unwrap();
        assert_eq!(theme, loaded);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_get_by_name_supports_all_builtins() {
        for name in ["postman", "terminal", "dark", "light", "forest"] {
            assert!(Theme::get_by_name(name).is_some(), "{name} should resolve");
        }
    }

    #[test]
    fn test_builtin_theme_names_are_stable() {
        assert_eq!(
            Theme::builtin_theme_names(),
            vec![
                "postman".to_string(),
                "dark".to_string(),
                "light".to_string(),
                "forest".to_string(),
                "terminal".to_string(),
            ]
        );
    }

    #[test]
    fn test_theme_helpers_use_pane_styles() {
        let theme = Theme::dark();
        assert_eq!(
            theme.border_color(true),
            theme.border.active_color.as_color()
        );
        assert_eq!(
            theme.title_color(false),
            theme.pane.inactive_title.as_color()
        );
        assert_eq!(
            theme.pane_bg(true),
            theme.pane.active_background.as_ref().unwrap().as_color()
        );
    }

    #[test]
    fn test_theme_registry_defaults_have_order() {
        let registry = ThemeRegistry::with_defaults();
        assert_eq!(
            registry.names(),
            vec![
                "postman".to_string(),
                "dark".to_string(),
                "light".to_string(),
                "forest".to_string(),
                "terminal".to_string(),
            ]
        );
    }

    #[test]
    fn test_theme_registry_cycle_next_rotates_in_order() {
        let mut registry = ThemeRegistry::with_defaults();
        registry.set_current("terminal");
        assert_eq!(registry.cycle_next().name, "postman");
        assert_eq!(registry.cycle_next().name, "dark");
    }
}
