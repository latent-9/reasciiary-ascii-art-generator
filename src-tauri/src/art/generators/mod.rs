//! Tools that produce a frame.
//!
//! Three of these exist in the Swift original and are ports; `gen2d` is new.
//!
//! - `ascii` — a solid lifted out of a `.txt` drawing, height from ink
//!   coverage. Port of `Ascii3D.swift` plus `Ascii3DPaneView.swift`.
//! - `scene`  — a spinning primitive from a formula. Port of `AsciiScene.swift`.
//! - `media`  — a frame of an image or video, quantised to glyphs. Port of
//!   `AsciiMedia.swift`.
//! - `gen2d`  — flow fields and noise, drawn with `tiny-skia`. New.

pub mod ascii;
// pub mod scene;
// pub mod media;
// pub mod gen2d;
