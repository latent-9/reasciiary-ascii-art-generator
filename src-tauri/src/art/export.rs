//! Writing frames out.
//!
//! The numbers here are ported from `asciiary/AsciiExport.swift`, not chosen
//! fresh. They were arrived at against real posted output, and the reasoning
//! behind each is kept because none of them are guessable from first
//! principles.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Text,
    Png,
    Gif,
    Mp4,
}

impl Format {
    pub fn is_animated(self) -> bool {
        matches!(self, Self::Gif | Self::Mp4)
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Text => "txt",
            Self::Png => "png",
            Self::Gif => "gif",
            Self::Mp4 => "mp4",
        }
    }

    pub fn from_path(path: &str) -> Option<Self> {
        match path.rsplit('.').next()? {
            "txt" => Some(Self::Text),
            "png" => Some(Self::Png),
            "gif" => Some(Self::Gif),
            "mp4" => Some(Self::Mp4),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub format: Format,
    pub columns: usize,
    pub rows: usize,
    pub frames_per_second: u32,
    /// Seconds of animation to record.
    pub duration: f64,
    /// Pixels per point. Two gives a Retina-sharp frame at a size timelines
    /// still accept.
    pub scale: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            format: Format::Gif,
            columns: 160,
            rows: 48,
            frames_per_second: 20,
            duration: 4.0,
            scale: 2.0,
        }
    }
}

impl Settings {
    /// The widest a GIF is drawn.
    ///
    /// A GIF quantises every frame to its own 256 colours, and that cost climbs
    /// with the pixel count: at twice this width a twelve-second loop takes
    /// about three minutes to write and lands as six megabytes — for a file a
    /// timeline re-encodes on upload anyway. Held to this, the same loop writes
    /// in seconds and looks no different once posted.
    pub const GIF_MAXIMUM_WIDTH: f64 = 1024.0;

    /// The scale this export is really drawn at. Everything but a GIF gets the
    /// resolution that was picked; a GIF gets the largest that stays inside
    /// [`GIF_MAXIMUM_WIDTH`](Self::GIF_MAXIMUM_WIDTH).
    pub fn resolved_scale(&self, cell_width: f64) -> f64 {
        let width = cell_width * self.columns as f64;
        if self.format != Format::Gif || width <= 0.0 {
            return self.scale;
        }
        self.scale.min((Self::GIF_MAXIMUM_WIDTH / width).max(1.0))
    }

    pub fn frame_count(&self) -> usize {
        if !self.format.is_animated() {
            return 1;
        }
        ((self.duration * self.frames_per_second as f64).round() as usize).max(1)
    }
}

/// Rounds a pixel size up to a multiple of two.
///
/// H.264 encodes in macroblocks and rejects an odd frame, and a grid times a
/// fractional cell width lands on odd constantly.
pub fn even_dimensions(width: u32, height: u32) -> (u32, u32) {
    (width + width % 2, height + height % 2)
}

/// Flat colour and hard glyph edges compress well, but a low bitrate turns thin
/// strokes to mush — which is the whole picture here.
pub fn h264_bitrate(width: u32, height: u32) -> u64 {
    width as u64 * height as u64 * 8
}

/// Arguments for the MP4 pass.
///
/// `yuv420p` is required for the file to play on X and in QuickTime, but its
/// chroma subsampling is exactly what hurts thin coloured strokes; the bitrate
/// above is what buys that back. AVFoundation handled this silently.
pub fn mp4_args(pattern: &str, fps: u32, width: u32, height: u32, out: &str) -> Vec<String> {
    vec![
        "-y".into(),
        "-framerate".into(), fps.to_string(),
        "-i".into(), pattern.into(),
        "-c:v".into(), "libx264".into(),
        "-profile:v".into(), "high".into(),
        "-pix_fmt".into(), "yuv420p".into(),
        "-b:v".into(), h264_bitrate(width, height).to_string(),
        out.into(),
    ]
}

/// Arguments for the GIF pass.
///
/// `stats_mode=single` plus `paletteuse=new=1` gives every frame its own 256
/// colours, which is what `CGImageDestination` did. ffmpeg's default builds one
/// palette for the whole animation and looks visibly different.
///
/// This has to be a single pass over a split graph: per-frame palettegen emits
/// one palette *per frame*, so there is no single palette file to hand to a
/// second invocation.
///
/// `-loop 0` means forever, which is what a posted loop wants.
pub fn gif_args(pattern: &str, fps: u32, out: &str) -> Vec<String> {
    vec![
        "-y".into(),
        "-framerate".into(), fps.to_string(),
        "-i".into(), pattern.into(),
        "-lavfi".into(),
        "split[a][b];[a]palettegen=stats_mode=single[p];[b][p]paletteuse=new=1".into(),
        "-loop".into(), "0".into(),
        out.into(),
    ]
}
