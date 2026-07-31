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

    /// How many seconds one loop takes, which is the whole clip unless asked
    /// otherwise.
    ///
    /// Every animated tool here wants the same answer, and wants it in the same
    /// shape: the movement is the piece, so one loop is the clip, and asking for
    /// a shorter period is how the piece gets repeated inside it on purpose. A
    /// period of nought is a division by nought wherever it lands, so it is
    /// refused here rather than at each of the places it would land.
    pub fn period(&self) -> Result<f64, String> {
        let period = self.f64("period", self.f64("duration", 4.0)?)?;
        if period <= 0.0 {
            return Err("--period is how many seconds a loop takes, so it has to be positive".into());
        }
        Ok(period)
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
