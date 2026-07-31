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

/// The two tools that draw pixels and read them back offer the same choices
/// about the reading, because it is the same reader behind both. The controls
/// are shared rather than copied — nothing here is written to, so one list can
/// serve two tools without them seeing each other's settings.
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
    { kind: "range", flag: "contrast", label: "Contrast", min: 0.2, max: 4, step: 0.1, value: 1 },
    { kind: "switch", flag: "color", label: "Source colour", value: false },
    { kind: "switch", flag: "invert", label: "Invert", value: false },
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
/// glyphs make of it, then lift it. One file, carried between the two.
///
/// The tools that brought their own subject are not in this list. A formula
/// turning on its own is a fine thing to watch and a poor thing to open an app
/// on: nothing on screen was asked for, and the one tool somebody came here to
/// use was three tabs along. They are still in the registry behind the command
/// line, where a piece is asked for by name.
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
