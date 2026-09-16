use std::sync::atomic::{AtomicBool, Ordering};

static APP_DARK: AtomicBool = AtomicBool::new(false);

pub fn set_app_dark(dark: bool) {
    APP_DARK.store(dark, Ordering::Relaxed);
}

pub fn app_dark() -> bool {
    APP_DARK.load(Ordering::Relaxed)
}

/// rxvt COLORFGBG: background ≥ 8 is treated as a light terminal.
///
/// Pushed through the spawn environment, not a query reply — xterm cannot supply
/// it, which is why this stayed in the backend after the query responders moved.
pub fn colorfgbg(dark: bool) -> &'static str {
    if dark { "15;0" } else { "0;15" }
}

pub fn is_dark_colorfgbg(value: &str) -> bool {
    value
        .split(';')
        .nth(1)
        .and_then(|bg| bg.parse::<u8>().ok())
        .is_some_and(|bg| bg < 8)
}

/// CSI ? 2031 h/l: the CLI wants (or stops wanting) theme-change reports.
pub fn observe_theme_notify(data: &str, current: bool) -> bool {
    let bytes = data.as_bytes();
    let mut enabled = current;
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i] == 0x1b && bytes[i + 1] == b'[' && bytes[i + 2] == b'?' {
            let rest = &data[i + 3..];
            if let Some(end) = rest.find(|c: char| c == 'h' || c == 'l') {
                let on = rest.as_bytes()[end] == b'h';
                if rest[..end].split(';').any(|mode| mode == "2031") {
                    enabled = on;
                }
                i += 3 + end + 1;
                continue;
            }
        }
        i += 1;
    }
    enabled
}

/// Mode 2031: 1 = became dark, 2 = became light. Prompts a new OSC 10/11 probe.
pub fn theme_change_report(dark: bool) -> &'static str {
    if dark { "\x1b[?997;1n" } else { "\x1b[?997;2n" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_colorfgbg_is_the_rxvt_light_pair() {
        assert_eq!(colorfgbg(false), "0;15");
        assert!(!is_dark_colorfgbg("0;15"));
        assert!(is_dark_colorfgbg("15;0"));
    }

    #[test]
    fn theme_notify_tracks_decset_2031() {
        assert!(observe_theme_notify("\x1b[?2031h", false));
        assert!(observe_theme_notify("\x1b[?1004;2031h", false));
        assert!(!observe_theme_notify("\x1b[?2031l", true));
        assert_eq!(theme_change_report(true), "\x1b[?997;1n");
        assert_eq!(theme_change_report(false), "\x1b[?997;2n");
    }
}
