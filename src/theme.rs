//! Theme system for customizing the visual appearance of usage display.
//!
//! Themes control how progress bars, icons, and section headers are rendered
//! in both the tray label and dropdown menu.

/// Available theme variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum ThemeName {
    /// Minimal ASCII style: ==··· 40% ~2h
    Minimal,
    /// Bold industrial blocks: ██░░░ 40% ▸2h
    Blocks,
    /// Friendly rounded circles: ◉◉○○○ 40% ⏲2h
    Soft,
    /// Precise box-drawing lines: ━━┄┄┄ 40% ›2h
    Lines,
    /// Technical angular diamonds: ◆◆◇◇◇ 40% »2h
    Sharp,
    /// Vibrant dark-friendly style: ▮▮▯▯▯ 40% ⧗2h
    #[default]
    Neon,
}

/// Theme configuration defining all visual elements
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Theme {
    /// Character for filled portion of mini progress bar (tray)
    pub bar_full: &'static str,
    /// Character for empty portion of mini progress bar (tray)
    pub bar_empty: &'static str,
    /// Number of segments in mini progress bar (max 255)
    pub bar_segments: u8,
    /// Icon/prefix before reset time
    pub time_icon: &'static str,
    /// Character for filled portion of wide progress bar (menu)
    pub menu_bar_full: &'static str,
    /// Character for empty portion of wide progress bar (menu)
    pub menu_bar_empty: &'static str,
    /// Number of segments in wide progress bar (max 255)
    pub menu_bar_segments: u8,
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
    #[allow(clippy::too_many_lines)]
    pub fn config(self) -> Theme {
        match self {
            // Minimal: Pure ASCII, maximum compatibility, professional
            Self::Minimal => Theme {
                bar_full: "=",
                bar_empty: "·",
                bar_segments: 5,
                time_icon: "~",
                menu_bar_full: "=",
                menu_bar_empty: "·",
                menu_bar_segments: 10,
                session_icon: ">",
                weekly_icon: ">>",
                header_left: "[ ",
                header_right: " ]",
                header_fill: "-",
                quit_icon: "x",
                loading: "...",
                error_icon: "!",
                loading_pct: "--%",
            },
            // Blocks: Bold industrial, maximum contrast
            Self::Blocks => Theme {
                bar_full: "█",
                bar_empty: "░",
                bar_segments: 5,
                time_icon: "▸",
                menu_bar_full: "█",
                menu_bar_empty: "░",
                menu_bar_segments: 10,
                session_icon: "▸",
                weekly_icon: "▹",
                header_left: "▌ ",
                header_right: " ▐",
                header_fill: "─",
                quit_icon: "■",
                loading: "▒",
                error_icon: "▲",
                loading_pct: "░░%",
            },
            // Soft: Friendly, approachable, organic circles
            Self::Soft => Theme {
                bar_full: "◉",
                bar_empty: "○",
                bar_segments: 5,
                time_icon: "⏲",
                menu_bar_full: "●",
                menu_bar_empty: "○",
                menu_bar_segments: 10,
                session_icon: "⟳",
                weekly_icon: "⟲",
                header_left: "╭─ ",
                header_right: " ─╮",
                header_fill: "─",
                quit_icon: "○",
                loading: "◌",
                error_icon: "⊗",
                loading_pct: "○○%",
            },
            // Lines: Geometric precision, box-drawing purity
            Self::Lines => Theme {
                bar_full: "━",
                bar_empty: "┄",
                bar_segments: 5,
                time_icon: "›",
                menu_bar_full: "━",
                menu_bar_empty: "┄",
                menu_bar_segments: 10,
                session_icon: "├",
                weekly_icon: "╞",
                header_left: "┌─ ",
                header_right: " ─┐",
                header_fill: "─",
                quit_icon: "┘",
                loading: "┄",
                error_icon: "╳",
                loading_pct: "┄┄%",
            },
            // Sharp: Technical, angular, power-user aesthetic
            Self::Sharp => Theme {
                bar_full: "◆",
                bar_empty: "◇",
                bar_segments: 5,
                time_icon: "»",
                menu_bar_full: "▰",
                menu_bar_empty: "▱",
                menu_bar_segments: 10,
                session_icon: "►",
                weekly_icon: "▻",
                header_left: "◄ ",
                header_right: " ►",
                header_fill: "═",
                quit_icon: "◆",
                loading: "◇",
                error_icon: "◈",
                loading_pct: "◇◇%",
            },
            // Neon: High contrast, optimized for dark themes
            Self::Neon => Theme {
                bar_full: "▮",
                bar_empty: "▯",
                bar_segments: 5,
                time_icon: "⧗",
                menu_bar_full: "▓",
                menu_bar_empty: "░",
                menu_bar_segments: 10,
                session_icon: "◈",
                weekly_icon: "◆",
                header_left: "╔═ ",
                header_right: " ═╗",
                header_fill: "═",
                quit_icon: "▪",
                loading: "▫",
                error_icon: "⬢",
                loading_pct: "░░%",
            },
        }
    }

    /// List all available theme names
    #[allow(dead_code)]
    pub fn all() -> &'static [ThemeName] {
        &[
            Self::Minimal,
            Self::Blocks,
            Self::Soft,
            Self::Lines,
            Self::Sharp,
            Self::Neon,
        ]
    }

    /// Get theme name as a string
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Blocks => "blocks",
            Self::Soft => "soft",
            Self::Lines => "lines",
            Self::Sharp => "sharp",
            Self::Neon => "neon",
        }
    }

    /// Parse theme name from string
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "minimal" => Some(Self::Minimal),
            "blocks" => Some(Self::Blocks),
            "soft" => Some(Self::Soft),
            "lines" => Some(Self::Lines),
            "sharp" => Some(Self::Sharp),
            "neon" => Some(Self::Neon),
            _ => None,
        }
    }
}

/// Convert a percentage (0-100) to filled segment count.
/// Returns a value in range [0, segments].
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn percentage_to_filled(percentage: f64, segments: u8) -> usize {
    // Clamp to [0, 100], compute ratio, round to nearest segment
    let filled = (percentage.clamp(0.0, 100.0) / 100.0 * f64::from(segments) + 0.5).floor();
    // Safe: clamped percentage guarantees filled is in [0, segments]
    (filled as u8).min(segments) as usize
}

impl Theme {
    /// Create a mini progress bar (for tray label)
    pub fn mini_bar(&self, percentage: f64) -> String {
        let filled = percentage_to_filled(percentage, self.bar_segments);
        let empty = usize::from(self.bar_segments) - filled;
        format!(
            "{}{}",
            self.bar_full.repeat(filled),
            self.bar_empty.repeat(empty)
        )
    }

    /// Create a wide progress bar (for menu)
    pub fn wide_bar(&self, percentage: f64) -> String {
        let filled = percentage_to_filled(percentage, self.menu_bar_segments);
        let empty = usize::from(self.menu_bar_segments) - filled;
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
            self.bar_empty.repeat(usize::from(self.bar_segments)),
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

    /// Helper to count occurrences of a substring in a string
    fn count_occurrences(s: &str, pattern: &str) -> usize {
        s.matches(pattern).count()
    }

    #[test]
    fn test_theme_name_default_is_neon() {
        assert_eq!(ThemeName::default(), ThemeName::Neon);
    }

    #[test]
    fn test_theme_name_from_str_all_variants() {
        // All theme names should parse (case-insensitive)
        for theme in ThemeName::all() {
            let name = theme.as_str();
            assert_eq!(ThemeName::from_str(name), Some(*theme));
            assert_eq!(ThemeName::from_str(&name.to_uppercase()), Some(*theme));
        }
        // Unknown names return None
        assert_eq!(ThemeName::from_str("unknown"), None);
        assert_eq!(ThemeName::from_str(""), None);
    }

    #[test]
    fn test_mini_bar_has_correct_segment_count() {
        for theme_name in ThemeName::all() {
            let theme = theme_name.config();
            let segments = usize::from(theme.bar_segments);

            // 0% should be all empty chars
            let bar_0 = theme.mini_bar(0.0);
            assert_eq!(
                count_occurrences(&bar_0, theme.bar_empty),
                segments,
                "Theme {:?} at 0% should have {} empty segments",
                theme_name,
                segments
            );

            // 100% should be all full chars
            let bar_100 = theme.mini_bar(100.0);
            assert_eq!(
                count_occurrences(&bar_100, theme.bar_full),
                segments,
                "Theme {:?} at 100% should have {} full segments",
                theme_name,
                segments
            );

            // 50% should have roughly half of each (allowing for rounding)
            let bar_50 = theme.mini_bar(50.0);
            let full_count = count_occurrences(&bar_50, theme.bar_full);
            let empty_count = count_occurrences(&bar_50, theme.bar_empty);
            assert_eq!(
                full_count + empty_count,
                segments,
                "Theme {:?} at 50% should have {} total segments",
                theme_name,
                segments
            );
        }
    }

    #[test]
    fn test_wide_bar_has_correct_segment_count() {
        for theme_name in ThemeName::all() {
            let theme = theme_name.config();
            let segments = usize::from(theme.menu_bar_segments);

            // 0% should be all empty chars
            let bar_0 = theme.wide_bar(0.0);
            assert_eq!(
                count_occurrences(&bar_0, theme.menu_bar_empty),
                segments,
                "Theme {:?} wide bar at 0% should have {} empty segments",
                theme_name,
                segments
            );

            // 100% should be all full chars
            let bar_100 = theme.wide_bar(100.0);
            assert_eq!(
                count_occurrences(&bar_100, theme.menu_bar_full),
                segments,
                "Theme {:?} wide bar at 100% should have {} full segments",
                theme_name,
                segments
            );
        }
    }

    #[test]
    fn test_section_header_contains_title() {
        for theme_name in ThemeName::all() {
            let theme = theme_name.config();
            let header = theme.section_header("TEST");

            assert!(
                header.contains("TEST"),
                "Theme {:?} header should contain the title",
                theme_name
            );
            assert!(
                header.starts_with(theme.header_left),
                "Theme {:?} header should start with header_left",
                theme_name
            );
            assert!(
                header.ends_with(theme.header_right),
                "Theme {:?} header should end with header_right",
                theme_name
            );
        }
    }

    #[test]
    fn test_loading_label_structure() {
        for theme_name in ThemeName::all() {
            let theme = theme_name.config();
            let label = theme.loading_label();

            // Should contain the loading percentage placeholder
            assert!(
                label.contains(theme.loading_pct),
                "Theme {:?} loading label should contain loading_pct",
                theme_name
            );

            // Should start with empty bar (segments * bar_empty chars)
            let expected_bar = theme.bar_empty.repeat(usize::from(theme.bar_segments));
            assert!(
                label.starts_with(&expected_bar),
                "Theme {:?} loading label should start with {} empty bar segments",
                theme_name,
                theme.bar_segments
            );

            // Should have a space between bar and loading_pct
            assert!(
                label.contains(' '),
                "Theme {:?} loading label should have space separator",
                theme_name
            );
        }
    }

    #[test]
    fn test_loading_pct_ends_with_percent() {
        for theme_name in ThemeName::all() {
            let theme = theme_name.config();
            assert!(
                theme.loading_pct.ends_with('%'),
                "Theme {:?} loading_pct should end with '%'",
                theme_name
            );
            // Should have at least 2 chars before the %
            assert!(
                theme.loading_pct.len() >= 3,
                "Theme {:?} loading_pct should be at least 3 chars (XX%)",
                theme_name
            );
        }
    }

    #[test]
    fn test_error_label_contains_error_text() {
        for theme_name in ThemeName::all() {
            let theme = theme_name.config();
            let label = theme.error_label();

            assert!(
                label.contains("Error"),
                "Theme {:?} error label should contain 'Error'",
                theme_name
            );
            assert!(
                label.contains(theme.error_icon),
                "Theme {:?} error label should contain error_icon",
                theme_name
            );
        }
    }

    #[test]
    fn test_all_themes_have_valid_config() {
        for theme_name in ThemeName::all() {
            let config = theme_name.config();

            // All string fields should be non-empty
            assert!(
                !config.bar_full.is_empty(),
                "{:?} bar_full empty",
                theme_name
            );
            assert!(
                !config.bar_empty.is_empty(),
                "{:?} bar_empty empty",
                theme_name
            );
            assert!(
                !config.time_icon.is_empty(),
                "{:?} time_icon empty",
                theme_name
            );
            assert!(
                !config.menu_bar_full.is_empty(),
                "{:?} menu_bar_full empty",
                theme_name
            );
            assert!(
                !config.menu_bar_empty.is_empty(),
                "{:?} menu_bar_empty empty",
                theme_name
            );
            assert!(
                !config.header_left.is_empty(),
                "{:?} header_left empty",
                theme_name
            );
            assert!(
                !config.header_right.is_empty(),
                "{:?} header_right empty",
                theme_name
            );
            assert!(
                !config.error_icon.is_empty(),
                "{:?} error_icon empty",
                theme_name
            );
            assert!(
                !config.loading_pct.is_empty(),
                "{:?} loading_pct empty",
                theme_name
            );

            // Segment counts should be reasonable
            assert!(
                config.bar_segments >= 3,
                "{:?} bar_segments too small",
                theme_name
            );
            assert!(
                config.bar_segments <= 10,
                "{:?} bar_segments too large",
                theme_name
            );
            assert!(
                config.menu_bar_segments >= 5,
                "{:?} menu_bar_segments too small",
                theme_name
            );
            assert!(
                config.menu_bar_segments <= 20,
                "{:?} menu_bar_segments too large",
                theme_name
            );

            // Full and empty chars should be different
            assert_ne!(
                config.bar_full, config.bar_empty,
                "{:?} bar_full and bar_empty should differ",
                theme_name
            );
            assert_ne!(
                config.menu_bar_full, config.menu_bar_empty,
                "{:?} menu_bar_full and menu_bar_empty should differ",
                theme_name
            );
        }
    }

    #[test]
    fn test_all_themes_count() {
        assert_eq!(ThemeName::all().len(), 6);
    }

    #[test]
    fn test_as_str_roundtrip() {
        for theme_name in ThemeName::all() {
            let s = theme_name.as_str();
            let parsed = ThemeName::from_str(s);
            assert_eq!(
                parsed,
                Some(*theme_name),
                "Theme {:?} should roundtrip through as_str/from_str",
                theme_name
            );
        }
    }
}
