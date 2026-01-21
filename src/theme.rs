//! Theme system for customizing the visual appearance of usage display.
//!
//! Themes control how progress bars, icons, and section headers are rendered
//! in both the tray label and dropdown menu.

/// Available theme variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum ThemeName {
    /// Modern Unicode style: ▰▱▱▱▱ 17% ◷2h15m
    #[default]
    Modern,
    /// Minimal text style: [##---] 17% ~2h15m
    Minimal,
    /// Retro ASCII style: [####------] 17% (2h15m)
    Retro,
    /// Block characters: ██░░░ 17% ⏱2h15m
    Blocks,
    /// Dots style: ●●○○○ 17% ↻2h15m
    Dots,
}

/// Theme configuration defining all visual elements
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Theme {
    /// Character for filled portion of mini progress bar (tray)
    pub bar_full: &'static str,
    /// Character for empty portion of mini progress bar (tray)
    pub bar_empty: &'static str,
    /// Number of segments in mini progress bar
    pub bar_segments: usize,
    /// Icon/prefix before reset time
    pub time_icon: &'static str,
    /// Character for filled portion of wide progress bar (menu)
    pub menu_bar_full: &'static str,
    /// Character for empty portion of wide progress bar (menu)
    pub menu_bar_empty: &'static str,
    /// Number of segments in wide progress bar
    pub menu_bar_segments: usize,
    /// Icon for session reset time
    pub session_icon: &'static str,
    /// Icon for weekly reset time
    pub weekly_icon: &'static str,
    /// Left bracket for section headers
    pub header_left: &'static str,
    /// Right bracket for section headers
    pub header_right: &'static str,
    /// Fill character for section headers
    pub header_fill: &'static str,
    /// Quit menu item prefix
    pub quit_icon: &'static str,
    /// Loading indicator
    pub loading: &'static str,
    /// Error indicator
    pub error_icon: &'static str,
    /// Placeholder for loading percentage
    pub loading_pct: &'static str,
}

impl ThemeName {
    /// Get the theme configuration for this theme name
    pub fn config(self) -> Theme {
        match self {
            Self::Modern => Theme {
                bar_full: "▰",
                bar_empty: "▱",
                bar_segments: 5,
                time_icon: "◷",
                menu_bar_full: "█",
                menu_bar_empty: "░",
                menu_bar_segments: 10,
                session_icon: "⏱",
                weekly_icon: "📅",
                header_left: "━━━━━━ ",
                header_right: " ━━━━━━",
                header_fill: "━",
                quit_icon: "✕",
                loading: "⏳",
                error_icon: "⚠",
                loading_pct: "··%",
            },
            Self::Minimal => Theme {
                bar_full: "#",
                bar_empty: "-",
                bar_segments: 5,
                time_icon: "~",
                menu_bar_full: "#",
                menu_bar_empty: "-",
                menu_bar_segments: 10,
                session_icon: "",
                weekly_icon: "",
                header_left: "[ ",
                header_right: " ]",
                header_fill: "-",
                quit_icon: "",
                loading: "",
                error_icon: "!",
                loading_pct: "--%",
            },
            Self::Retro => Theme {
                bar_full: "=",
                bar_empty: ".",
                bar_segments: 5,
                time_icon: "@",
                menu_bar_full: "#",
                menu_bar_empty: ".",
                menu_bar_segments: 10,
                session_icon: ">>",
                weekly_icon: ">>",
                header_left: "<<< ",
                header_right: " >>>",
                header_fill: "=",
                quit_icon: "[X]",
                loading: "...",
                error_icon: "[!]",
                loading_pct: "??%",
            },
            Self::Blocks => Theme {
                bar_full: "█",
                bar_empty: "░",
                bar_segments: 5,
                time_icon: "⏱",
                menu_bar_full: "█",
                menu_bar_empty: "░",
                menu_bar_segments: 10,
                session_icon: "⏱",
                weekly_icon: "📆",
                header_left: "▐ ",
                header_right: " ▌",
                header_fill: "─",
                quit_icon: "■",
                loading: "◌",
                error_icon: "✖",
                loading_pct: "░░%",
            },
            Self::Dots => Theme {
                bar_full: "●",
                bar_empty: "○",
                bar_segments: 5,
                time_icon: "↻",
                menu_bar_full: "●",
                menu_bar_empty: "○",
                menu_bar_segments: 10,
                session_icon: "◐",
                weekly_icon: "◑",
                header_left: "• ",
                header_right: " •",
                header_fill: "·",
                quit_icon: "×",
                loading: "◔",
                error_icon: "⊘",
                loading_pct: "○○%",
            },
        }
    }

    /// List all available theme names
    #[allow(dead_code)]
    pub fn all() -> &'static [ThemeName] {
        &[
            Self::Modern,
            Self::Minimal,
            Self::Retro,
            Self::Blocks,
            Self::Dots,
        ]
    }

    /// Get theme name as a string
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Modern => "modern",
            Self::Minimal => "minimal",
            Self::Retro => "retro",
            Self::Blocks => "blocks",
            Self::Dots => "dots",
        }
    }

    /// Parse theme name from string
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "modern" => Some(Self::Modern),
            "minimal" => Some(Self::Minimal),
            "retro" => Some(Self::Retro),
            "blocks" => Some(Self::Blocks),
            "dots" => Some(Self::Dots),
            _ => None,
        }
    }
}

impl Theme {
    /// Create a mini progress bar (for tray label)
    pub fn mini_bar(&self, percentage: f64) -> String {
        let filled = ((percentage / 100.0) * self.bar_segments as f64).round() as usize;
        let filled = filled.min(self.bar_segments);
        let empty = self.bar_segments - filled;
        format!(
            "{}{}",
            self.bar_full.repeat(filled),
            self.bar_empty.repeat(empty)
        )
    }

    /// Create a wide progress bar (for menu)
    pub fn wide_bar(&self, percentage: f64) -> String {
        let filled = ((percentage / 100.0) * self.menu_bar_segments as f64).round() as usize;
        let filled = filled.min(self.menu_bar_segments);
        let empty = self.menu_bar_segments - filled;
        format!(
            "{}{}",
            self.menu_bar_full.repeat(filled),
            self.menu_bar_empty.repeat(empty)
        )
    }

    /// Format a section header
    pub fn section_header(&self, title: &str) -> String {
        format!("{}{}{}", self.header_left, title, self.header_right)
    }

    /// Format the loading state for tray label
    pub fn loading_label(&self) -> String {
        format!(
            "{} {}",
            self.bar_empty.repeat(self.bar_segments),
            self.loading_pct
        )
    }

    /// Format the error state for tray label
    pub fn error_label(&self) -> String {
        format!("{} Error", self.error_icon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_name_default() {
        assert_eq!(ThemeName::default(), ThemeName::Modern);
    }

    #[test]
    fn test_theme_name_from_str() {
        assert_eq!(ThemeName::from_str("modern"), Some(ThemeName::Modern));
        assert_eq!(ThemeName::from_str("MINIMAL"), Some(ThemeName::Minimal));
        assert_eq!(ThemeName::from_str("unknown"), None);
    }

    #[test]
    fn test_theme_mini_bar() {
        let theme = ThemeName::Modern.config();
        assert_eq!(theme.mini_bar(0.0), "▱▱▱▱▱");
        assert_eq!(theme.mini_bar(100.0), "▰▰▰▰▰");
        assert_eq!(theme.mini_bar(40.0), "▰▰▱▱▱");
    }

    #[test]
    fn test_theme_wide_bar() {
        let theme = ThemeName::Modern.config();
        assert_eq!(theme.wide_bar(0.0), "░░░░░░░░░░");
        assert_eq!(theme.wide_bar(100.0), "██████████");
        assert_eq!(theme.wide_bar(50.0), "█████░░░░░");
    }

    #[test]
    fn test_theme_section_header() {
        let theme = ThemeName::Modern.config();
        assert_eq!(theme.section_header("TEST"), "━━━━━━ TEST ━━━━━━");

        let minimal = ThemeName::Minimal.config();
        assert_eq!(minimal.section_header("TEST"), "[ TEST ]");
    }

    #[test]
    fn test_all_themes_have_config() {
        for name in ThemeName::all() {
            let config = name.config();
            assert!(!config.bar_full.is_empty());
            assert!(!config.bar_empty.is_empty());
        }
    }
}
