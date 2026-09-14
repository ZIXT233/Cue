pub struct CodexTitleProbe {
    pending: String,
    pub session_id: Option<String>,
    pub session_id_prefix: Option<String>,
    needs_input: bool,
    last_state: Option<String>,
}

impl CodexTitleProbe {
    pub fn new() -> Self {
        Self {
            pending: String::new(),
            session_id: None,
            session_id_prefix: None,
            needs_input: false,
            last_state: None,
        }
    }

    pub fn consume_needs_input(&mut self) -> bool {
        let waiting = self.needs_input;
        self.needs_input = false;
        waiting
    }

    pub fn push(&mut self, data: &str) -> Option<String> {
        self.pending.push_str(data);
        let mut state = None;
        loop {
            let Some(start) = self.pending.find("\x1b]") else {
                self.pending = if self.pending.ends_with('\x1b') { "\x1b".into() } else { String::new() };
                break;
            };
            self.pending = self.pending[start..].to_string();
            let bel = self.pending.find('\u{7}');
            let st = self.pending.find("\x1b\\");
            let end = match (bel, st) {
                (Some(a), Some(b)) if a <= b => Some((a, 1)),
                (Some(a), None) => Some((a, 1)),
                (None, Some(b)) => Some((b, 2)),
                _ => None,
            };
            let Some((end, term_len)) = end else {
                if self.pending.len() > 4096 { self.pending.clear(); }
                break;
            };
            let osc = self.pending[2..end].to_string();
            self.pending = self.pending[end + term_len..].to_string();
            if osc.starts_with("9;") {
                self.needs_input = true;
                state = Some("attention".into());
                continue;
            }
            if !osc.starts_with("0;") && !osc.starts_with("2;") {
                continue;
            }
            let title = &osc[2..];
            if !regex::Regex::new(r"^(?:\[ ! \] Action Required(?: \|)? )?codex(?:\s|$)").unwrap().is_match(title) {
                continue;
            }
            self.session_id_prefix = regex::Regex::new(r"\b([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{5})\.\.\.")
                .unwrap()
                .captures(title)
                .map(|c| c[1].to_string());
            self.session_id = regex::Regex::new(r"\b[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}\b")
                .unwrap()
                .find(title)
                .map(|m| m.as_str().to_string());
            if title.contains("Action Required") {
                self.needs_input = true;
                state = Some("attention".into());
            } else if regex::Regex::new(r"\b(Working|Thinking|Waiting)\b").unwrap().is_match(title) {
                state = Some("working".into());
            } else if regex::Regex::new(r"\bReady\b").unwrap().is_match(title) {
                state = Some("attention".into());
            } else if regex::Regex::new(r"\bStarting\b").unwrap().is_match(title) {
                state = Some("starting".into());
            } else {
                state = Some("unknown".into());
            }
        }
        match state {
            Some(next) if self.last_state.as_ref() != Some(&next) => {
                self.last_state = Some(next.clone());
                Some(next)
            }
            _ => None,
        }
    }
}
