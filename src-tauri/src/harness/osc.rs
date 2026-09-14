use super::signals::HookSignal;
use base64::Engine;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct HookOscProbe {
    token: String,
    pending: String,
}

impl HookOscProbe {
    pub fn new(token: String) -> Self {
        Self { token, pending: String::new() }
    }

    pub fn push(&mut self, data: &str) -> Option<HookSignal> {
        self.pending.push_str(data);
        loop {
            let Some(start) = self.pending.find("\x1b]777;cue;") else {
                self.pending = take_suffix(&self.pending, 14);
                return None;
            };
            self.pending = self.pending[start..].to_string();
            let Some(end) = self.pending.find('\u{7}') else {
                if self.pending.len() > 16384 { self.pending.clear(); }
                return None;
            };
            if end < 14 {
                self.pending = self.pending[end + 1..].to_string();
                continue;
            }
            let encoded = self.pending[14..end].to_string();
            self.pending = self.pending[end + 1..].to_string();
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) {
                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    if value.get("token").and_then(|v| v.as_str()) == Some(self.token.as_str()) {
                        if let Some(signal) = value.get("signal") {
                            if let Ok(mut parsed) = serde_json::from_value::<HookSignal>(signal.clone()) {
                                parsed.at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
                                return Some(parsed);
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn take_suffix(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_string();
    }
    let mut start = value.len() - max;
    while start > 0 && !value.is_char_boundary(start) {
        start -= 1;
    }
    value[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_does_not_panic_on_multibyte() {
        let text = "你好世界".repeat(8);
        let _ = take_suffix(&text, 14);
        let mut probe = HookOscProbe::new("token".into());
        assert!(probe.push(&text).is_none());
        assert!(probe.push(&format!("{text}\x1b]777;cue;")).is_none());
    }
}
