//! The results of the last 8 tool calls (G29 step 19, D12): a call with the same tool, the same
//! arguments and the same project inputs gets the same output without analyzing again.

use std::collections::VecDeque;

use super::backend::{ProjectBackend, ToolError, ToolOutput, ToolRequest};
use crate::cache::Sha256;

const ENTRIES: usize = 8;

#[derive(Default)]
pub struct ResultCache {
    entries: VecDeque<(String, ToolOutput)>,
}

impl ResultCache {
    fn get(&mut self, key: &str) -> Option<ToolOutput> {
        let at = self.entries.iter().position(|(entry, _)| entry == key)?;
        let entry = self.entries.remove(at)?;
        let output = entry.1.clone();
        self.entries.push_front(entry);
        Some(output)
    }

    fn put(&mut self, key: String, output: ToolOutput) {
        self.entries.retain(|(entry, _)| *entry != key);
        self.entries.push_front((key, output));
        self.entries.truncate(ENTRIES);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// The inputs a tool reads: the project's own sources and manifests (by the hash of the KG's
/// inputs), nothing for the guide and `explain`, `None` for a tool whose inputs are other
/// directories (`tz_impact`), which is not cached.
fn inputs(backend: &ProjectBackend, tool: &str) -> Option<String> {
    match tool {
        "tz_guide" | "tz_explain" => Some(String::new()),
        "tz_impact" => None,
        _ => {
            let project = crate::driver::Project::load_for_tests(&backend.root).ok()?;
            Some(crate::kg::input::hex(&crate::kg::input::source_hash(
                &project,
            )))
        }
    }
}

/// The output of `request` from the cache, or from `compute` (then cached; errors are not).
pub fn cached(
    backend: &ProjectBackend,
    request: &ToolRequest<'_>,
    compute: impl FnOnce(&ToolRequest<'_>) -> Result<ToolOutput, ToolError>,
) -> Result<ToolOutput, ToolError> {
    let Some(inputs) = inputs(backend, request.name) else {
        return compute(request);
    };
    let mut hash = Sha256::new();
    hash.field("tool", request.name.as_bytes());
    hash.field(
        "arguments",
        serde_json::to_string(request.arguments)
            .unwrap_or_default()
            .as_bytes(),
    );
    hash.field("inputs", inputs.as_bytes());
    let key = hash.hex();
    if let Some(output) = backend
        .results
        .lock()
        .ok()
        .and_then(|mut cache| cache.get(&key))
    {
        return Ok(output);
    }
    let output = compute(request)?;
    if let Ok(mut cache) = backend.results.lock() {
        cache.put(key, output.clone());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_the_last_eight() {
        let mut cache = ResultCache::default();
        let output = |n: i64| ToolOutput {
            text: n.to_string(),
            structured: json!({"n": n}),
        };
        for n in 0..10 {
            cache.put(n.to_string(), output(n));
        }
        assert_eq!(cache.len(), 8);
        assert!(cache.get("0").is_none() && cache.get("1").is_none());
        assert_eq!(cache.get("2"), Some(output(2)));
        cache.put("10".into(), output(10));
        // 2 was used last, so 3 went.
        assert!(cache.get("3").is_none());
        assert!(cache.get("2").is_some());
    }
}
