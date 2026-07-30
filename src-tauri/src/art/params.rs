//! Flags carried by one stage of a REPL pipeline.
//!
//! The Swift original had three ad-hoc settings structs and a CLI whose
//! defaults disagreed with the GUI's (`depth 8` vs the slider's, and so on).
//! One representation, read the same way everywhere, is what stops that from
//! happening again.

use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct Params {
    pub positional: Vec<String>,
    pub flags: HashMap<String, Option<String>>,
}

impl Params {
    pub fn first_positional(&self) -> Option<&str> {
        self.positional.first().map(String::as_str)
    }

    pub fn string(&self, name: &str) -> Option<&str> {
        self.flags.get(name)?.as_deref()
    }

    /// A flag with no value, e.g. `--still`.
    pub fn is_set(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    pub fn f64(&self, name: &str, default: f64) -> Result<f64, String> {
        match self.string(name) {
            None => Ok(default),
            Some(raw) => raw
                .parse()
                .map_err(|_| format!("--{name} expects a number, got `{raw}`")),
        }
    }

    pub fn usize(&self, name: &str, default: usize) -> Result<usize, String> {
        match self.string(name) {
            None => Ok(default),
            Some(raw) => raw
                .parse()
                .map_err(|_| format!("--{name} expects a whole number, got `{raw}`")),
        }
    }

    /// Reproducibility is a new concept here — nothing in the Swift original
    /// used randomness. Any generator that does must route it through this, so
    /// a piece worth keeping can be drawn again.
    pub fn seed(&self, default: u64) -> Result<u64, String> {
        match self.string("seed") {
            None => Ok(default),
            Some(raw) => raw
                .parse()
                .map_err(|_| format!("--seed expects a whole number, got `{raw}`")),
        }
    }
}
