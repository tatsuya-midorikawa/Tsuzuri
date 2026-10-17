use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static PHASES: Mutex<Vec<(String, f64)>> = Mutex::new(Vec::new());

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var_os("TSUZURI_TIME_PASSES").is_some_and(|value| value == "1"))
}

pub fn start() -> Option<Instant> {
    enabled().then(Instant::now)
}

pub fn finish(name: &str, started: Option<Instant>) {
    let Some(started) = started else { return };
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut phases = PHASES.lock().expect("compiler phase timings poisoned");
    match phases.iter_mut().find(|(phase, _)| phase == name) {
        Some((_, total)) => *total += ms,
        None => phases.push((name.to_owned(), ms)),
    }
}

pub struct Phase {
    name: &'static str,
    started: Option<Instant>,
}

impl Phase {
    pub fn start(name: &'static str) -> Self {
        Self {
            name,
            started: start(),
        }
    }
}

impl Drop for Phase {
    fn drop(&mut self) {
        finish(self.name, self.started);
    }
}

pub fn render(phases: &[(String, f64)], total_ms: f64) -> String {
    let phases = phases
        .iter()
        .map(|(name, ms)| {
            format!(
                "{{\"name\":{},\"ms\":{ms:.3}}}",
                serde_json::to_string(name).expect("a phase name is JSON serializable")
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"time_passes\":1,\"phases\":[{phases}],\"total_ms\":{total_ms:.3}}}")
}

pub struct Report(Option<Instant>);

impl Report {
    /// A report covers the phases since its start, so a process that runs commands one after the
    /// other (`tsuzuri watch`) reports each of them alone.
    pub fn start() -> Self {
        if enabled() {
            PHASES
                .lock()
                .expect("compiler phase timings poisoned")
                .clear();
        }
        Self(start())
    }

    /// Makes the report print nothing: the commands of `tsuzuri watch` have reports of their own.
    pub fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for Report {
    fn drop(&mut self) {
        if let Some(started) = self.0 {
            let total_ms = started.elapsed().as_secs_f64() * 1000.0;
            let phases = PHASES.lock().expect("compiler phase timings poisoned");
            eprintln!("{}", render(&phases, total_ms));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn render_keeps_order_and_three_decimals() {
        assert_eq!(
            render(&[("parse".into(), 1.23456), ("check".into(), 2.0)], 5.0),
            "{\"time_passes\":1,\"phases\":[{\"name\":\"parse\",\"ms\":1.235},{\"name\":\"check\",\"ms\":2.000}],\"total_ms\":5.000}"
        );
        let escaped = render(&[("tool.\"quoted\"".into(), 0.0)], 1.0);
        assert!(serde_json::from_str::<serde_json::Value>(&escaped).is_ok());
    }
}
