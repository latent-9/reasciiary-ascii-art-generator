/// What each tool takes, in the tool's own terms.
///
/// The window used to be the ascii tool's window: one file to open, one depth
/// slider, one spin. A second tool later that shape does not hold — a picture
/// read flat has no depth and no turn — and writing each one's controls into
/// the markup by hand would put the same list in two places and let them drift.
/// So what a tool takes is written down here as data, and the panel is built
/// from it. Adding the next tool is an entry in this table.
///
/// Every `flag` is the name the backend already reads, so nothing translates
/// between here and `Params`; a control is a flag with a range around it.

/// One row in the panel.
export type Control =
  | {
      kind: "range";
      flag: string;
      label: string;
      min: number;
      max: number;
      step: number;
      value: number;
    }
  | {
      kind: "choice";
      flag: string;
      label: string;
      value: string;
      options: string[];
      /// Set where the options are initials rather than words, so `mp4` is
      /// offered as MP4 and not as Mp4.
      caps?: boolean;
    }
  | { kind: "switch"; flag: string; label: string; value: boolean };

/// Controls that belong together, under a heading.
export type Group = { title: string; controls: Control[] };

/// A file the tool reads, and what the dialog offering one should accept.
export type Source = {
  label: string;
  extensions: string[];
  /// A drawing carried in the binary, so the tool has something to show before
  /// anything has been opened. Inline rather than a file because a path that
  /// works in dev does not survive being bundled.
  sample?: string;
};

/// Where a turnable tool's camera starts, which is also what a double-click on
/// the preview puts back. Each tool wants its own: a relief is read from above
/// and a solid is not, so one shared angle would frame one of them badly.
export type Camera = { yaw: number; pitch: number; zoom: number };

export type Tool = {
  /// The key the backend's registry knows it by.
  name: string;
  label: string;
  blurb: string;
  source?: Source;
  camera?: Camera;
  groups: Group[];
};

/// Graded ink, so the heightfield is obvious the moment the app opens: `@`
/// rises, `.` barely lifts.
const DIAMOND = [
  "        @        ",
  "       @@@       ",
  "      @@#@@      ",
  "     @@###@@     ",
  "    @@##+##@@    ",
  "   @@##+++##@@   ",
  "  @@##+++++##@@  ",
  " @@##+++...+##@@ ",
  "  @@##+++++##@@  ",
  "   @@##+++##@@   ",
  "    @@##+##@@    ",
  "     @@###@@     ",
  "      @@#@@      ",
  "       @@@       ",
  "        @        ",
].join("\n");

/// What both tools read, because both read the same thing.
///
/// The lift and the flat read are one question asked with and without the third
/// dimension, so offering them different files would be inventing a difference
/// the backend does not have: a drawing is a grid of characters either way, and
/// a picture is a grid of light. Shared rather than written twice, so a format
/// added to one is never missing from the other.
const SUBJECT: Source = {
  label: "drawing or picture",
  extensions: ["txt", "asc", "ans", "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff"],
  sample: DIAMOND,
};

/// How much light a source is taken to carry, which is one question wherever a
/// source is read: which end of it is the subject, how far apart the rest are
/// pushed, and whether its own colours come with it. The backend reads these
/// three off the same three flags in every tool that takes a file, so they are
/// written once here too.
const TONES: Control[] = [
  { kind: "range", flag: "contrast", label: "Contrast", min: 0.2, max: 4, step: 0.1, value: 1 },
  { kind: "switch", flag: "color", label: "Source colour", value: false },
  { kind: "switch", flag: "invert", label: "Invert", value: false },
];

/// The tools that read a source back as marks offer the same choices about the
/// reading, because it is the same reader behind them. The controls are shared
/// rather than copied — nothing here is written to, so one list can serve two
/// tools without them seeing each other's settings.
const READING: Group = {
  title: "Reading",
  controls: [
    {
      kind: "choice",
      flag: "marks",
      label: "Marks",
      value: "match",
      options: ["match", "shades", "detailed", "ink"],
    },
    ...TONES,
  ],
};

const STILL: Control = { kind: "switch", flag: "still", label: "Hold still", value: false };

/// Whole turns over one loop, which is the number somebody choosing one
/// actually has in mind. The backend turns by this over its period rather than
/// at a rate, so every setting of it meets itself at the seam.
const TURNS: Control = { kind: "range", flag: "turns", label: "Turns", min: 0, max: 8, step: 1, value: 2 };

/// What the surface does on top of the turn, and how hard. Off by default: the
/// subject is somebody's own drawing or a shape they picked, and warping it
/// without being asked is not the tool's call to make.
const MOVEMENT: Control[] = [
  {
    kind: "choice",
    flag: "motion",
    label: "Moves",
    value: "none",
    options: ["none", "ripple", "breathe", "drift"],
  },
  { kind: "range", flag: "amount", label: "Strength", min: 0, max: 1, step: 0.05, value: 0.35 },
];

/// The window is the lift's window, and the flat read is here because it is the
/// same question asked without the third dimension — open the file, see what the
/// glyphs make of it, then lift it. One file, carried between the three.
///
/// The spiral is the third thing to do with that file: lay it on the wave and
/// let one winding line draw it. It is also the only tool here that will stand
/// without one — the drift is a piece in its own right, and "Drift only" is how
/// to ask for it — and a piece that needs nothing opened is what the rest of the
/// registry is kept out of the window for: a formula turning on its own is a
/// fine thing to watch and a poor thing to open an app on, and the tool somebody
/// came here to use should not be three tabs along. The app still opens on the
/// lift and this is one tab from it. What earns it the tab is that the piece is
/// composed by eye — the angle it is seen from and how thick the crowd is are
/// the whole of it — and a command line answers a question about an angle one
/// rendered file at a time. The others are still in the registry behind it,
/// where a piece is asked for by name.
export const TOOLS: Tool[] = [
  {
    name: "ascii",
    label: "3D",
    blurb: "A drawing or a picture lifted into a solid, ink for height.",
    source: SUBJECT,
    camera: { yaw: 34, pitch: 29, zoom: 0.92 },
    groups: [
      {
        title: "Solid",
        controls: [
          { kind: "range", flag: "depth", label: "Depth", min: 1, max: 40, step: 1, value: 8 },
          // How wide a picture is read before it is lifted. A drawing brings its
          // own grid and ignores this — it was written at a size, and resampling
          // it would blur the very cells whose ink is the height.
          { kind: "range", flag: "relief", label: "Detail", min: 16, max: 320, step: 8, value: 120 },
          { kind: "range", flag: "contrast", label: "Contrast", min: 0.2, max: 4, step: 0.1, value: 1 },
          { kind: "switch", flag: "invert", label: "Invert", value: false },
          // Which characters a lit face is graded through. The outline is
          // traced against strokes whatever this says, so the choice buys
          // shades on the faces without costing the silhouette its edges.
          {
            kind: "choice",
            flag: "grade",
            label: "Characters",
            value: "detailed",
            options: ["shades", "detailed", "ink"],
          },
        ],
      },
      { title: "Motion", controls: [TURNS, ...MOVEMENT, STILL] },
    ],
  },
  {
    name: "media",
    label: "Flat",
    blurb: "The same file read straight back as glyphs, with no lift.",
    source: SUBJECT,
    groups: [
      {
        title: "Frame",
        controls: [
          {
            kind: "choice",
            flag: "fit",
            label: "Fit",
            value: "contain",
            options: ["contain", "cover"],
          },
        ],
      },
      READING,
    ],
  },
  {
    name: "spiral",
    label: "Spiral",
    blurb: "A wave winding out from the middle, drawn by one spiralling line.",
    source: SUBJECT,
    // Nearly overhead, because in here the piece is drawing the file the window
    // has open and a picture laid on the disc is only legible from above. Tipped
    // over far enough to see the swells standing in front of each other, a
    // photograph on the disc is a smear and a page of writing is nothing at all.
    // The drift alone is a finer thing from up around fifty, and that is a drag
    // away: what this settles is where a tab that has just been handed a file
    // opens, and what a double-click puts back.
    camera: { yaw: 0, pitch: 14, zoom: 1 },
    groups: [
      {
        title: "Subject",
        controls: [
          // The window always has a file to hand, so this is the only way from
          // in here to ask for the drift as it was composed, with nothing laid
          // under it. On a command line it is a line with no file on it.
          { kind: "switch", flag: "bare", label: "Drift only", value: false },
          // How far over the drift the picture is laid. Past a whole the crowd
          // is standing on the middle of it, which is a crop rather than a fit
          // and a fair thing to want.
          { kind: "range", flag: "spread", label: "Spread", min: 0.2, max: 2, step: 0.05, value: 1 },
          // The disc is round and a photograph is not, so one of the two has to
          // give. Contained leaves the near and far of the disc empty; covering
          // fills it and crops the ends of the picture. The same word and the
          // same two answers as the flat read.
          {
            kind: "choice",
            flag: "fit",
            label: "Fit",
            value: "contain",
            options: ["contain", "cover"],
          },
          // How far the picture is opened out to its own darkest and brightest
          // before anything below is asked. Sits above the tones because it runs
          // before them: contrast turns about the middle of the range, so a
          // picture with nothing near the middle is one it can only darken.
          // Fully open by default — the crowd draws light as the size of a mark,
          // and a screenshot of pale text on a dark field has no light in it to
          // speak of once it is read down to the size the crowd shows.
          { kind: "range", flag: "open", label: "Open out", min: 0, max: 1, step: 0.05, value: 1 },
          ...TONES,
          // Where the picture stops being a subject and starts being paper the
          // line crosses without marking. Sits under the tones because it is the
          // same question they ask: which of this picture is the subject.
          { kind: "range", flag: "floor", label: "Paper below", min: 0, max: 0.6, step: 0.01, value: 0.04 },
          // How closely the line that draws the picture is wound. Few turns is a
          // coarse coil with a subject somewhere in it; many is an engraving.
          // The default is where a photograph's branches survive the read.
          { kind: "range", flag: "windings", label: "Windings", min: 8, max: 200, step: 1, value: 110 },
        ],
      },
      {
        // What the disc does with nothing laid on it. None of it is reached
        // while a file is open: a picture is drawn by the winding line, and
        // these settle the scatter that stands in for it when there is none.
        title: "Drift",
        controls: [
          // Enough of them to read as a crowd rather than as specks, and the
          // ceiling is where a frame stops looking any different for the wait.
          { kind: "range", flag: "count", label: "Particles", min: 500, max: 40000, step: 500, value: 17000 },
          // The plane is invisible, so this buys nothing but the accuracy of the
          // edge the crowd is hidden behind.
          // Stepped in tens from thirty, so the default is a place the slider
          // can actually stand rather than the nearest notch to it.
          { kind: "range", flag: "mesh", label: "Surface", min: 30, max: 280, step: 10, value: 130 },
          // Not a setting so much as another draw of the same piece: it settles
          // where every particle lies and how large it is, and nothing else.
          { kind: "range", flag: "seed", label: "Arrangement", min: 1, max: 99, step: 1, value: 7 },
        ],
      },
      // The loop is as long as the export is, so the length in the output group
      // is the period as well and there is nothing to set here twice.
      { title: "Motion", controls: [STILL] },
    ],
  },
];

/// The values a set of groups starts on, as the strings a flag carries.
///
/// A switch that is off is absent rather than `"false"`: the backend asks
/// whether a flag is set, so sending one at all would turn it on.
export function defaults(groups: Group[]): Record<string, string> {
  const values: Record<string, string> = {};
  for (const group of groups) {
    for (const control of group.controls) {
      if (control.kind === "switch") {
        if (control.value) values[control.flag] = "";
      } else {
        values[control.flag] = String(control.value);
      }
    }
  }
  return values;
}
