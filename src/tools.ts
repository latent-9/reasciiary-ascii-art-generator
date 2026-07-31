/// What each tool takes, in the tool's own terms.
///
/// The window used to be the ascii tool's window: one file to open, one depth
/// slider, one spin. Four more tools later that shape does not hold — a flow
/// field has no depth, a picture has no turn, a piece brings its own subject —
/// and writing each one's controls into the markup by hand would put the same
/// list in five places and let them drift. So what a tool takes is written down here as data, and the panel is
/// built from it. Adding the next tool is an entry in this table.
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

/// The window opens on whichever tool is first, so the first is the one that
/// has something to show without being given anything: the pieces bring their
/// own subject, and every other tool here waits for a file or a formula.
export const TOOLS: Tool[] = [
  {
    name: "loops",
    label: "Loop",
    blurb: "A finished piece that comes back round to where it began.",
    groups: [
      {
        title: "Piece",
        controls: [
          {
            kind: "choice",
            flag: "piece",
            label: "Piece",
            value: "hilbert",
            options: [
              "hilbert",
              "sinusoids",
              "sierpinski",
              "sliding",
              "spherewave",
              "toruscurve",
            ],
          },
        ],
      },
      {
        title: "Motion",
        controls: [
          { kind: "range", flag: "period", label: "Loop", min: 1, max: 20, step: 1, value: 6 },
          { kind: "range", flag: "seed", label: "Seed", min: 0, max: 99, step: 1, value: 7 },
          STILL,
        ],
      },
      READING,
    ],
  },
  {
    name: "ascii",
    label: "Drawing",
    blurb: "A .txt drawing lifted into a solid, ink for height.",
    source: { label: "ASCII drawing", extensions: ["txt", "asc", "ans"], sample: DIAMOND },
    camera: { yaw: 34, pitch: 29, zoom: 0.92 },
    groups: [
      {
        title: "Solid",
        controls: [{ kind: "range", flag: "depth", label: "Depth", min: 1, max: 40, step: 1, value: 8 }],
      },
      { title: "Motion", controls: [TURNS, ...MOVEMENT, STILL] },
    ],
  },
  {
    name: "scene",
    label: "Solid",
    blurb: "A primitive cut from a formula and turned.",
    camera: { yaw: 0, pitch: 26, zoom: 0.92 },
    groups: [
      {
        title: "Shape",
        controls: [
          {
            kind: "choice",
            flag: "shape",
            label: "Form",
            value: "torus",
            options: ["sphere", "torus", "cube", "knot"],
          },
          { kind: "range", flag: "steps", label: "Steps", min: 8, max: 160, step: 8, value: 64 },
          {
            kind: "range",
            flag: "thickness",
            label: "Thickness",
            min: 0.05,
            max: 1,
            step: 0.01,
            value: 0.42,
          },
        ],
      },
      { title: "Motion", controls: [TURNS, ...MOVEMENT, STILL] },
    ],
  },
  {
    name: "media",
    label: "Picture",
    blurb: "A picture or an animation quantised to glyphs.",
    source: {
      label: "Picture",
      extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff"],
    },
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
    name: "gen2d",
    label: "Field",
    blurb: "A flow field drawn in pixels and read back as glyphs.",
    groups: [
      {
        title: "Field",
        controls: [
          { kind: "choice", flag: "style", label: "Style", value: "flow", options: ["flow", "noise"] },
          { kind: "range", flag: "lines", label: "Lines", min: 64, max: 2000, step: 32, value: 640 },
          { kind: "range", flag: "steps", label: "Length", min: 16, max: 400, step: 8, value: 120 },
          { kind: "range", flag: "grain", label: "Grain", min: 0.1, max: 6, step: 0.1, value: 1.3 },
          { kind: "range", flag: "swirl", label: "Swirl", min: 0.05, max: 4, step: 0.05, value: 1 },
          { kind: "range", flag: "seed", label: "Seed", min: 0, max: 99, step: 1, value: 7 },
        ],
      },
      {
        title: "Motion",
        controls: [
          { kind: "range", flag: "period", label: "Loop", min: 1, max: 20, step: 1, value: 8 },
          STILL,
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
