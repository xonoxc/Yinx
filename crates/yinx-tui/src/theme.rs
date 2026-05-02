use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use ratatui::style::Color;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub background: ColorDef,
    pub foreground: ColorDef,
    pub border: BorderStyle,
    pub highlight: HighlightStyle,
    pub semantic: SemanticColors,
    pub pane: PaneColors,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorDef {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ColorDef {
    pub fn as_color(&self) -> Color {
        Color::Rgb(self.r, self.g, self.b)
    }

    pub fn from_color(color: Color) -> Option<Self> {
        match color {
            Color::Rgb(r, g, b) => Some(Self { r, g, b }),
            _ => None,
        }
    }

    pub const BLACK: ColorDef = ColorDef { r: 0, g: 0, b: 0 };
    pub const WHITE: ColorDef = ColorDef { r: 255, g: 255, b: 255 };
    pub const RED: ColorDef = ColorDef { r: 220, g: 50, b: 47 };
    pub const GREEN: ColorDef = ColorDef { r: 80, g: 200, b: 120 };
    pub const YELLOW: ColorDef = ColorDef { r: 255, g: 184, b: 108 };
    pub const BLUE: ColorDef = ColorDef { r: 97, g: 175, b: 239 };
    pub const MAGENTA: ColorDef = ColorDef { r: 198, g: 120, b: 221 };
    pub const CYAN: ColorDef = ColorDef { r: 86, g: 182, b: 194 };
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
    pub background: ColorDef,
    pub title: ColorDef,
    pub status_bar_bg: ColorDef,
    pub status_bar_fg: ColorDef,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            name: "dark".to_string(),
            background: ColorDef::BLACK,
            foreground: ColorDef { r: 220, g: 220, b: 220 },
            border: BorderStyle {
                color: ColorDef { r: 60, g: 60, b: 60 },
                active_color: ColorDef::BLUE,
                style: BorderType::Rounded,
            },
            highlight: HighlightStyle {
                bg: ColorDef { r: 40, g: 44, b: 52 },
                fg: ColorDef { r: 220, g: 220, b: 220 },
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
                background: ColorDef { r: 20, g: 20, b: 30 },
                title: ColorDef::CYAN,
                status_bar_bg: ColorDef { r: 40, g: 44, b: 52 },
                status_bar_fg: ColorDef::WHITE,
            },
        }
    }

    pub fn light() -> Self {
        Self {
            name: "light".to_string(),
            background: ColorDef { r: 240, g: 240, b: 240 },
            foreground: ColorDef { r: 30, g: 30, b: 30 },
            border: BorderStyle {
                color: ColorDef { r: 180, g: 180, b: 180 },
                active_color: ColorDef { r: 50, g: 100, b: 200 },
                style: BorderType::Rounded,
            },
            highlight: HighlightStyle {
                bg: ColorDef { r: 220, g: 220, b: 220 },
                fg: ColorDef { r: 30, g: 30, b: 30 },
                selected_bg: ColorDef { r: 50, g: 100, b: 200 },
                selected_fg: ColorDef::WHITE,
            },
            semantic: SemanticColors {
                success: ColorDef { r: 0, g: 150, b: 50 },
                error: ColorDef { r: 200, g: 0, b: 0 },
                warning: ColorDef { r: 200, g: 150, b: 0 },
                info: ColorDef { r: 0, g: 100, b: 200 },
            },
            pane: PaneColors {
                background: ColorDef { r: 250, g: 250, b: 250 },
                title: ColorDef { r: 50, g: 100, b: 200 },
                status_bar_bg: ColorDef { r: 220, g: 220, b: 220 },
                status_bar_fg: ColorDef { r: 30, g: 30, b: 30 },
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
        assert_eq!(theme.background.r, 0);
        assert_eq!(theme.background.g, 0);
        assert_eq!(theme.background.b, 0);
    }

    #[test]
    fn test_dark_theme_semantic_colors() {
        let theme = Theme::dark();
        assert_eq!(theme.semantic.success.g, 200);
        assert_eq!(theme.semantic.error.r, 220);
        assert_eq!(theme.semantic.warning.r, 255);
        assert_eq!(theme.semantic.info.r, 97);
    }

    #[test]
    fn test_light_theme() {
        let theme = Theme::light();
        assert_eq!(theme.name, "light");
        assert_eq!(theme.background.r, 240);
        assert_eq!(theme.background.r, theme.background.g);
    }

    #[test]
    fn test_light_theme_semantic_colors() {
        let theme = Theme::light();
        assert_eq!(theme.semantic.success.r, 0);
        assert_eq!(theme.semantic.success.g, 150);
        assert_eq!(theme.semantic.error.r, 200);
        assert_eq!(theme.semantic.warning.r, 200);
    }

    #[test]
    fn test_theme_border_styles() {
        let dark = Theme::dark();
        assert_eq!(dark.border.style, BorderType::Rounded);
        assert_eq!(dark.border.color.r, 60);

        let light = Theme::light();
        assert_eq!(light.border.style, BorderType::Rounded);
        assert_eq!(light.border.active_color.r, 50);
    }

    #[test]
    fn test_theme_highlight_colors() {
        let theme = Theme::dark();
        assert_eq!(theme.highlight.selected_bg.r, 97);
        assert_eq!(theme.highlight.selected_fg.r, 255);
    }

    #[test]
    fn test_theme_pane_colors() {
        let theme = Theme::dark();
        assert_eq!(theme.pane.title.r, 86);
        assert_eq!(theme.pane.status_bar_bg.r, 40);
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
        assert_eq!(ColorDef::BLACK.r, 0);
        assert_eq!(ColorDef::WHITE.r, 255);
        assert_eq!(ColorDef::GREEN.g, 200);
        assert_eq!(ColorDef::BLUE.r, 97);
        assert_eq!(ColorDef::MAGENTA.r, 198);
        assert_eq!(ColorDef::CYAN.r, 86);
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
            background: ColorDef { r: 10, g: 20, b: 30 },
            foreground: ColorDef { r: 200, g: 210, b: 220 },
            border: BorderStyle {
                color: ColorDef { r: 50, g: 50, b: 50 },
                active_color: ColorDef { r: 100, g: 150, b: 200 },
                style: BorderType::Double,
            },
            highlight: HighlightStyle {
                bg: ColorDef { r: 30, g: 30, b: 30 },
                fg: ColorDef { r: 200, g: 200, b: 200 },
                selected_bg: ColorDef { r: 80, g: 100, b: 180 },
                selected_fg: ColorDef { r: 255, g: 255, b: 255 },
            },
            semantic: SemanticColors {
                success: ColorDef { r: 50, g: 180, b: 50 },
                error: ColorDef { r: 200, g: 30, b: 30 },
                warning: ColorDef { r: 220, g: 160, b: 30 },
                info: ColorDef { r: 30, g: 80, b: 200 },
            },
            pane: PaneColors {
                background: ColorDef { r: 15, g: 15, b: 25 },
                title: ColorDef { r: 100, g: 150, b: 200 },
                status_bar_bg: ColorDef { r: 30, g: 30, b: 30 },
                status_bar_fg: ColorDef { r: 200, g: 200, b: 200 },
            },
        };

        assert_eq!(theme.name, "custom");
        assert_eq!(theme.border.style, BorderType::Double);
        assert_eq!(theme.semantic.success.g, 180);
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
}
