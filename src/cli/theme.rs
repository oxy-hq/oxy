use colored::*;
use lazy_static::lazy_static;
use std::sync::Mutex;

#[derive(Debug, Clone)]
struct Theme {
    primary: Color,
    secondary: Color,
    tertiary: Color,
    success: Color,
    warning: Color,
    error: Color,
    info: Color,
    text: Color,
}

#[derive(Debug, Clone)]
pub enum ThemeMode {
    Dark,
    Light,
}

struct ThemeManager {
    current_theme: Theme,
    mode: ThemeMode,
}

lazy_static! {
    static ref THEME_MANAGER: Mutex<ThemeManager> = Mutex::new(ThemeManager::new());
}

impl Theme {
    fn dark() -> Self {
        Theme {
            primary: Color::TrueColor {
                r: 112,
                g: 237,
                b: 153,
            }, // green
            secondary: Color::TrueColor {
                r: 97,
                g: 154,
                b: 234,
            }, // blue
            tertiary: Color::TrueColor {
                r: 172,
                g: 106,
                b: 237,
            }, // purple
            success: Color::TrueColor {
                r: 112,
                g: 237,
                b: 153,
            }, // green
            warning: Color::TrueColor {
                r: 242,
                g: 231,
                b: 129,
            }, // yellow
            error: Color::TrueColor {
                r: 234,
                g: 106,
                b: 109,
            }, // red
            info: Color::TrueColor {
                r: 75,
                g: 220,
                b: 222,
            }, // emerald
            text: Color::TrueColor {
                r: 246,
                g: 246,
                b: 245,
            }, // white
        }
    }

    fn light() -> Self {
        Theme {
            primary: Color::TrueColor {
                r: 64,
                g: 202,
                b: 82,
            }, // green
            secondary: Color::TrueColor {
                r: 27,
                g: 143,
                b: 239,
            }, // blue
            tertiary: Color::TrueColor {
                r: 214,
                g: 105,
                b: 245,
            }, // purple
            success: Color::TrueColor {
                r: 64,
                g: 202,
                b: 82,
            },
            warning: Color::TrueColor {
                r: 221,
                g: 185,
                b: 87,
            }, // yellow
            error: Color::TrueColor {
                r: 221,
                g: 96,
                b: 94,
            }, // red
            info: Color::TrueColor {
                r: 16,
                g: 193,
                b: 167,
            }, // emerald
            text: Color::TrueColor {
                r: 53,
                g: 55,
                b: 58,
            }, // gray
        }
    }
}

impl ThemeManager {
    fn new() -> Self {
        let mode = Self::detect_terminal_theme();
        let current_theme = match mode {
            ThemeMode::Dark => Theme::dark(),
            ThemeMode::Light => Theme::light(),
        };

        ThemeManager {
            current_theme,
            mode,
        }
    }

    fn detect_terminal_theme() -> ThemeMode {
        match terminal_light::luma() {
            Ok(luma) if luma > 0.85 => ThemeMode::Light,
            Ok(luma) if luma < 0.2 => ThemeMode::Dark,
            _ => ThemeMode::Dark,
        }
    }

    fn get_instance() -> &'static Mutex<ThemeManager> {
        &THEME_MANAGER
    }

    fn switch_theme(&mut self, mode: ThemeMode) {
        self.mode = mode.clone();
        self.current_theme = match mode {
            ThemeMode::Dark => Theme::dark(),
            ThemeMode::Light => Theme::light(),
        };
    }
}

pub fn get_current_theme_mode() -> ThemeMode {
    ThemeManager::get_instance().lock().unwrap().mode.clone()
}

pub fn switch_theme(mode: ThemeMode) {
    ThemeManager::get_instance()
        .lock()
        .unwrap()
        .switch_theme(mode);
}

pub trait StyledText {
    fn primary(self) -> ColoredString;
    fn secondary(self) -> ColoredString;
    fn tertiary(self) -> ColoredString;
    fn success(self) -> ColoredString;
    fn warning(self) -> ColoredString;
    fn error(self) -> ColoredString;
    fn info(self) -> ColoredString;
    fn text(self) -> ColoredString;
}

impl StyledText for &str {
    fn primary(self) -> ColoredString {
        let theme_manager = ThemeManager::get_instance().lock().unwrap();
        self.color(theme_manager.current_theme.primary)
    }

    fn secondary(self) -> ColoredString {
        let theme_manager = ThemeManager::get_instance().lock().unwrap();
        self.color(theme_manager.current_theme.secondary)
    }

    fn tertiary(self) -> ColoredString {
        let theme_manager = ThemeManager::get_instance().lock().unwrap();
        self.color(theme_manager.current_theme.tertiary)
    }

    fn success(self) -> ColoredString {
        let theme_manager = ThemeManager::get_instance().lock().unwrap();
        self.color(theme_manager.current_theme.success)
    }

    fn warning(self) -> ColoredString {
        let theme_manager = ThemeManager::get_instance().lock().unwrap();
        self.color(theme_manager.current_theme.warning)
    }

    fn error(self) -> ColoredString {
        let theme_manager = ThemeManager::get_instance().lock().unwrap();
        self.color(theme_manager.current_theme.error)
    }

    fn info(self) -> ColoredString {
        let theme_manager = ThemeManager::get_instance().lock().unwrap();
        self.color(theme_manager.current_theme.info)
    }

    fn text(self) -> ColoredString {
        let theme_manager = ThemeManager::get_instance().lock().unwrap();
        self.color(theme_manager.current_theme.text)
    }
}
