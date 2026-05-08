use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use ratatui::style::Color;

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColorDef {
    Rgb(u8, u8, u8),
    Reset,
}

impl ColorDef {
    pub fn as_color(&self) -> Color {
        match self {
            ColorDef::Rgb(r, g, b) => Color::Rgb(*r, *g, *b),
            ColorDef::Reset => Color::Reset,
        }
    }

    pub fn from_color(color: Color) -> Option<Self> {
        match color {
            Color::Rgb(r, g, b) => Some(Self::Rgb(r, g, b)),
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
pub struct PaneColors {
    pub background: Option<ColorDef>,
    pub title: ColorDef,
    pub status_bar_bg: ColorDef,
    pub status_bar_fg: ColorDef,
}

impl PaneColors {
    pub fn bg_color(&self) -> Color {
        self.background
            .as_ref()
            .map(|c| c.as_color())
            .unwrap_or(Color::Reset)
    }
}

pub struct ThemeRegistry {
    themes: std::collections::HashMap<String, Theme>,
    current: String,
}

impl ThemeRegistry {
    pub fn new() -> Self {
        Self {
            themes: std::collections::HashMap::new(),
            current: String::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.themes.len()
    }

    pub fn register(&mut self, name: String, theme: Theme) {
        self.themes.insert(name, theme);
    }

    pub fn get(&self, name: &str) -> Option<&Theme> {
        self.themes.get(name)
    }

    pub fn set_current(&mut self, name: &str) {
        self.current = name.to_string();
    }

    pub fn current(&self) -> Option<&Theme> {
        if self.current.is_empty() {
            None
        } else {
            self.themes.get(&self.current)
        }
    }

    pub fn cycle_next(&mut self) -> &Theme {
        let keys: Vec<String> = self.themes.keys().cloned().collect();
        if keys.is_empty() {
            panic!("No themes registered");
        }
        if self.current.is_empty() {
            self.current = keys[0].clone();
        } else {
            let current_idx = keys.iter().position(|k| k == &self.current).unwrap_or(0);
            let next_idx = (current_idx + 1) % keys.len();
            self.current = keys[next_idx].clone();
        }
        self.themes.get(&self.current).unwrap()
    }
}

impl Theme {
    pub fn terminal_default() -> Self {
        Self {
            name: "terminal_default".to_string(),
            background: None,
            foreground: ColorDef::Rgb(220, 220, 220),
            border: BorderStyle {
                color: ColorDef::Rgb(60, 60, 60),
                active_color: ColorDef::BLUE,
                style: BorderType::Rounded,
            },
            highlight: HighlightStyle {
                bg: ColorDef::Rgb(40, 44, 52),
                fg: ColorDef::Rgb(220, 220, 220),
                selected_bg: ColorDef::BLUE,
                selected_fg: ColorDef::WHITE,
            },
            semantic: SemanticColors {
                success: ColorDef::GREEN,
                error: ColorDef::RED,
                warning: ColorDef::YELLOW,
                info: ColorDef::BLUE,
            },
            pane: PaneColors {
                background: None,
                title: ColorDef::CYAN,
                status_bar_bg: ColorDef::Rgb(40, 44, 52),
                status_bar_fg: ColorDef::WHITE,
            },
        }
    }

    pub fn dark() -> Self {
        Self {
            name: "dark".to_string(),
            background: Some(ColorDef::BLACK),
            foreground: ColorDef::Rgb(220, 220, 220),
            border: BorderStyle {
                color: ColorDef::Rgb(60, 60, 60),
                active_color: ColorDef::BLUE,
                style: BorderType::Rounded,
            },
            highlight: HighlightStyle {
                bg: ColorDef::Rgb(40, 44, 52),
                fg: ColorDef::Rgb(220, 220, 220),
                selected_bg: ColorDef::BLUE,
                selected_fg: ColorDef::WHITE,
            },
            semantic: SemanticColors {
                success: ColorDef::GREEN,
                error: ColorDef::RED,
                warning: ColorDef::YELLOW,
                info: ColorDef::BLUE,
            },
            pane: PaneColors {
                background: None,
                title: ColorDef::CYAN,
                status_bar_bg: ColorDef::Rgb(40, 44, 52),
                status_bar_fg: ColorDef::WHITE,
            },
        }
    }

    pub fn light() -> Self {
        Self {
            name: "light".to_string(),
            background: Some(ColorDef::Rgb(240, 240, 240)),
            foreground: ColorDef::Rgb(30, 30, 30),
            border: BorderStyle {
                color: ColorDef::Rgb(180, 180, 180),
                active_color: ColorDef::Rgb(50, 100, 200),
                style: BorderType::Rounded,
            },
            highlight: HighlightStyle {
                bg: ColorDef::Rgb(220, 220, 220),
                fg: ColorDef::Rgb(30, 30, 30),
                selected_bg: ColorDef::Rgb(50, 100, 200),
                selected_fg: ColorDef::WHITE,
            },
            semantic: SemanticColors {
                success: ColorDef::Rgb(0, 150, 50),
                error: ColorDef::Rgb(200, 0, 0),
                warning: ColorDef::Rgb(200, 150, 0),
                info: ColorDef::Rgb(0, 100, 200),
            },
            pane: PaneColors {
                background: None,
                title: ColorDef::Rgb(50, 100, 200),
                status_bar_bg: ColorDef::Rgb(220, 220, 220),
                status_bar_fg: ColorDef::Rgb(30, 30, 30),
            },
        }
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
            "dark" => Some(Self::dark()),
            "light" => Some(Self::light()),
            _ => None,
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
    fn test_default_dark_theme() {
        let theme = Theme::dark();
        assert_eq!(theme.name, "dark");
        match theme.background {
            Some(ColorDef::Rgb(r, g, b)) => {
                assert_eq!(r, 0);
                assert_eq!(g, 0);
                assert_eq!(b, 0);
            }
            _ => panic!("Expected Some(Rgb) variant"),
        }
    }

    #[test]
    fn test_dark_theme_semantic_colors() {
        let theme = Theme::dark();
        match theme.semantic.success {
            ColorDef::Rgb(_, g, _) => assert_eq!(g, 200),
            _ => panic!("Expected Rgb"),
        }
        match theme.semantic.error {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 220),
            _ => panic!("Expected Rgb"),
        }
        match theme.semantic.warning {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 255),
            _ => panic!("Expected Rgb"),
        }
        match theme.semantic.info {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 97),
            _ => panic!("Expected Rgb"),
        }
    }

    #[test]
    fn test_light_theme() {
        let theme = Theme::light();
        assert_eq!(theme.name, "light");
        match theme.background {
            Some(ColorDef::Rgb(r, g, _)) => {
                assert_eq!(r, 240);
                assert_eq!(r, g);
            }
            _ => panic!("Expected Some(Rgb)"),
        }
    }

    #[test]
    fn test_light_theme_semantic_colors() {
        let theme = Theme::light();
        match theme.semantic.success {
            ColorDef::Rgb(r, g, _) => {
                assert_eq!(r, 0);
                assert_eq!(g, 150);
            }
            _ => panic!("Expected Rgb"),
        }
        match theme.semantic.error {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 200),
            _ => panic!("Expected Rgb"),
        }
        match theme.semantic.warning {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 200),
            _ => panic!("Expected Rgb"),
        }
    }

    #[test]
    fn test_theme_border_styles() {
        let dark = Theme::dark();
        assert_eq!(dark.border.style, BorderType::Rounded);
        match dark.border.color {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 60),
            _ => panic!("Expected Rgb"),
        }

        let light = Theme::light();
        assert_eq!(light.border.style, BorderType::Rounded);
        match light.border.active_color {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 50),
            _ => panic!("Expected Rgb"),
        }
    }

    #[test]
    fn test_theme_highlight_colors() {
        let theme = Theme::dark();
        match theme.highlight.selected_bg {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 97),
            _ => panic!("Expected Rgb"),
        }
        match theme.highlight.selected_fg {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 255),
            _ => panic!("Expected Rgb"),
        }
    }

    #[test]
    fn test_theme_pane_colors() {
        let theme = Theme::dark();
        match theme.pane.title {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 86),
            _ => panic!("Expected Rgb"),
        }
        match theme.pane.status_bar_bg {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 40),
            _ => panic!("Expected Rgb"),
        }
    }

    #[test]
    fn test_color_def_as_color() {
        let color_def = ColorDef::RED;
        let color = color_def.as_color();
        match color {
            Color::Rgb(r, g, b) => {
                assert_eq!(r, 220);
                assert_eq!(g, 50);
                assert_eq!(b, 47);
            }
            _ => panic!("Expected Rgb color"),
        }
    }

    #[test]
    fn test_color_def_constants() {
        match ColorDef::BLACK {
            ColorDef::Rgb(r, g, b) => {
                assert_eq!(r, 0);
                assert_eq!(g, 0);
                assert_eq!(b, 0);
            }
            _ => panic!("Expected Rgb"),
        }
        match ColorDef::WHITE {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 255),
            _ => panic!("Expected Rgb"),
        }
        match ColorDef::GREEN {
            ColorDef::Rgb(_, g, _) => assert_eq!(g, 200),
            _ => panic!("Expected Rgb"),
        }
        match ColorDef::BLUE {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 97),
            _ => panic!("Expected Rgb"),
        }
        match ColorDef::MAGENTA {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 198),
            _ => panic!("Expected Rgb"),
        }
        match ColorDef::CYAN {
            ColorDef::Rgb(r, _, _) => assert_eq!(r, 86),
            _ => panic!("Expected Rgb"),
        }
    }

    #[test]
    fn test_serialize_deserialize_theme() {
        let theme = Theme::dark();
        let serialized = serde_json::to_string(&theme).unwrap();
        let deserialized: Theme = serde_json::from_str(&serialized).unwrap();
        assert_eq!(theme, deserialized);
    }

    #[test]
    fn test_load_from_file_nonexistent() {
        let result = Theme::load_from_file("/nonexistent/path/theme.json");
        assert!(result.is_err());
        match result {
            Err(ThemeError::Io(_)) => (),
            _ => panic!("Expected Io error"),
        }
    }

    #[test]
    fn test_save_and_load_theme() {
        let theme = Theme::dark();
        let path = std::env::temp_dir().join("test_theme.json");
        theme.save_to_file(&path).unwrap();
        let loaded = Theme::load_from_file(&path).unwrap();
        assert_eq!(theme.name, loaded.name);
        assert_eq!(theme.background, loaded.background);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_get_by_name_dark() {
        let theme = Theme::get_by_name("dark");
        assert!(theme.is_some());
        assert_eq!(theme.unwrap().name, "dark");
    }

    #[test]
    fn test_get_by_name_light() {
        let theme = Theme::get_by_name("light");
        assert!(theme.is_some());
        assert_eq!(theme.unwrap().name, "light");
    }

    #[test]
    fn test_get_by_name_invalid() {
        let theme = Theme::get_by_name("nonexistent");
        assert!(theme.is_none());
    }

    #[test]
    fn test_theme_with_custom_colors() {
        let theme = Theme {
            name: "custom".to_string(),
            background: Some(ColorDef::Rgb(10, 20, 30)),
            foreground: ColorDef::Rgb(200, 210, 220),
            border: BorderStyle {
                color: ColorDef::Rgb(50, 50, 50),
                active_color: ColorDef::Rgb(100, 150, 200),
                style: BorderType::Double,
            },
            highlight: HighlightStyle {
                bg: ColorDef::Rgb(30, 30, 30),
                fg: ColorDef::Rgb(200, 200, 200),
                selected_bg: ColorDef::Rgb(80, 100, 180),
                selected_fg: ColorDef::Rgb(255, 255, 255),
            },
            semantic: SemanticColors {
                success: ColorDef::Rgb(50, 180, 50),
                error: ColorDef::Rgb(200, 30, 30),
                warning: ColorDef::Rgb(220, 160, 30),
                info: ColorDef::Rgb(30, 80, 200),
            },
            pane: PaneColors {
                background: Some(ColorDef::Rgb(15, 15, 25)),
                title: ColorDef::Rgb(100, 150, 200),
                status_bar_bg: ColorDef::Rgb(30, 30, 30),
                status_bar_fg: ColorDef::Rgb(200, 200, 200),
            },
        };

        assert_eq!(theme.name, "custom");
        assert_eq!(theme.border.style, BorderType::Double);
        match theme.semantic.success {
            ColorDef::Rgb(_, g, _) => assert_eq!(g, 180),
            _ => panic!("Expected Rgb"),
        }
    }

    #[test]
    fn test_border_type_serialization() {
        let border = BorderType::Rounded;
        let serialized = serde_json::to_string(&border).unwrap();
        let deserialized: BorderType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(border, deserialized);
    }

    #[test]
    fn test_theme_error_display() {
        let err = ThemeError::NotFound("test".to_string());
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_color_def_reset_variant() {
        let reset = ColorDef::Reset;
        let color = reset.as_color();
        assert_eq!(color, Color::Reset);
    }

    #[test]
    fn test_color_def_black_is_rgb() {
        match ColorDef::BLACK {
            ColorDef::Rgb(r, g, b) => {
                assert_eq!(r, 0);
                assert_eq!(g, 0);
                assert_eq!(b, 0);
            }
            _ => panic!("BLACK should be Rgb variant"),
        }
    }

    #[test]
    fn test_terminal_default_theme_has_no_background() {
        let theme = Theme::terminal_default();
        assert!(theme.background.is_none());
    }

    #[test]
    fn test_dark_theme_background_is_some_black() {
        let theme = Theme::dark();
        match theme.background {
            Some(ColorDef::Rgb(0, 0, 0)) => (),
            _ => panic!("Dark theme should have black background"),
        }
    }

    #[test]
    fn test_background_style_uses_reset_when_none() {
        let theme = Theme::terminal_default();
        let bg_color = theme.background
            .as_ref()
            .map(|c| c.as_color())
            .unwrap_or(Color::Reset);
        assert_eq!(bg_color, Color::Reset);
    }

    #[test]
    fn test_pane_background_optional() {
        let theme = Theme::terminal_default();
        let pane_bg = theme.pane.background
            .as_ref()
            .map(|c| c.as_color())
            .unwrap_or(Color::Reset);
        assert_eq!(pane_bg, Color::Reset);
    }

    // Issue 6: Theme Switcher - Task 6.1
    #[test]
    fn test_theme_registry_new_is_empty() {
        let registry = ThemeRegistry::new();
        assert_eq!(registry.len(), 0);
    }

    // Task 6.2
    #[test]
    fn test_register_theme() {
        let mut registry = ThemeRegistry::new();
        let theme = Theme::dark();
        registry.register("dark".to_string(), theme);
        assert_eq!(registry.len(), 1);
    }

    // Task 6.3
    #[test]
    fn test_get_registered_theme() {
        let mut registry = ThemeRegistry::new();
        registry.register("dark".to_string(), Theme::dark());
        
        let theme = registry.get("dark").unwrap();
        assert_eq!(theme.name, "dark");
    }

    // Task 6.4
    #[test]
    fn test_cycle_next_rotates_themes() {
        let mut registry = ThemeRegistry::new();
        registry.register("dark".to_string(), Theme::dark());
        registry.register("light".to_string(), Theme::light());
        registry.set_current("dark");
        
        let next = registry.cycle_next();
        assert_eq!(next.name, "light");
    }
}
