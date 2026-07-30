import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

/// A scheme is the two colours the render is actually made of, and the window
/// is dressed in them so what is on screen is what lands in the file. There
/// were six terminal palettes here, which coloured the chrome and nothing else
/// — every one of them exported the same near-white on near-black.
type Theme = { name: string; ink: string; paper: string };

const THEMES: Theme[] = [
  { name: "Ink", ink: "#0b0b0b", paper: "#f6f5f2" },
  { name: "Paper", ink: "#fbfbfb", paper: "#080808" },
  { name: "Bone", ink: "#f2e3bd", paper: "#0a0908" },
];

const element = <T extends HTMLElement>(id: string) =>
  document.getElementById(id) as T;

const preview = element<HTMLPreElement>("preview");
const screen = document.querySelector<HTMLDivElement>(".screen")!;
const status = element("status");
const hint = element("hint");
const fileLabel = element("file");
const renderButton = element<HTMLButtonElement>("render");

const SLIDERS = ["depth", "detail", "turns", "seconds"] as const;
type SliderName = (typeof SLIDERS)[number];

/// Graded ink so the heightfield is obvious the moment the app opens: `@` rises,
/// `.` barely lifts.
const SAMPLE = [
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

/// Where the camera starts, and what a double-click on the preview puts back.
const HOME = { yaw: 34, pitch: 29, zoom: 0.92 };

const state = {
  file: null as string | null,
  text: null as string | null,
  still: false,
  format: "mp4",
  theme: THEMES[0],
  ...HOME,
};

const hasDrawing = () => state.file !== null || state.text !== null;

function slider(name: SliderName): number {
  return Number(element<HTMLInputElement>(name).value);
}

/// One number settles the whole grid: how many cells the render gets. What
/// shape they are laid out in is the drawing's business, not a slider's.
///
/// Columns and rows were two of those, and every pair but a few framed the
/// drawing badly — a tall drawing in a wide grid sits in the middle of two
/// empty margins. Holding the *count* rather than the width is what keeps a
/// frame that follows the drawing from also changing how long it takes to
/// render: a portrait subject gets a portrait grid of the same size, not one
/// three times larger.
function grid() {
  const detail = slider("detail");
  const cells = (detail * detail) / 4;
  const aspect = plan.frame ?? 16 / 9;
  const columns = clamp(Math.round(Math.sqrt(cells * aspect)), 20, 400);
  return { columns, rows: clamp(Math.round(cells / columns), 8, 200) };
}

function applyTheme(theme: Theme) {
  state.theme = theme;
  const root = document.documentElement.style;
  root.setProperty("--ink", theme.ink);
  root.setProperty("--paper", theme.paper);
}

function buildThemes() {
  const container = element("themes");
  THEMES.forEach((theme, index) => {
    const button = document.createElement("button");
    button.innerHTML = `<span class="swatch" style="background:${theme.ink};outline-color:${theme.paper}"></span>${theme.name}`;
    if (index === 0) button.classList.add("on");
    button.addEventListener("click", () => {
      container.querySelectorAll("button").forEach((b) => b.classList.remove("on"));
      button.classList.add("on");
      applyTheme(theme);
    });
    container.appendChild(button);
  });
}

function request(withOutput?: string) {
  const { columns, rows } = grid();
  const seconds = slider("seconds");
  const flags: Record<string, string> = {
    depth: String(slider("depth")),
    zoom: state.zoom.toFixed(3),
    yaw: String(Math.round(state.yaw)),
    pitch: String(Math.round(state.pitch)),
    // The backend turns at radians a second. Whole turns over the clip is the
    // same number said in the units somebody choosing one actually has in mind,
    // and it lands on a seamless loop by construction rather than by rounding.
    spin: ((slider("turns") * 2 * Math.PI) / seconds).toFixed(4),
    duration: String(seconds),
    columns: String(columns),
    rows: String(rows),
    ink: state.theme.ink,
    paper: state.theme.paper,
  };
  if (state.still) flags.still = "";
  if (state.text) flags.text = state.text;

  return {
    tool: "ascii",
    positional: state.file && !state.text ? [state.file] : [],
    flags,
    output: withOutput ?? null,
  };
}

/// A cell's width and height as fractions of the font size, read from the same
/// custom properties the stylesheet lays the preview out on. Keeping one copy
/// is what stops the preview from drifting out of shape against the export.
const cell = (name: "advance" | "line") =>
  Number(
    getComputedStyle(document.documentElement).getPropertyValue(`--cell-${name}`),
  );

/// Scales the preview so the whole grid is visible.
function fitPreview() {
  if (!hasDrawing()) {
    preview.style.fontSize = "12px";
    return;
  }
  const { columns, rows } = grid();
  const width = (screen.clientWidth - 24) / (columns * cell("advance"));
  const height = (screen.clientHeight - 24) / (rows * cell("line"));
  preview.style.fontSize = `${Math.max(Math.min(width, height), 1)}px`;
}

const FRAME_MS = 80;

/// What the export will do, mirrored so the preview shows the same frames.
type Plan = {
  period: number | null;
  loops: number;
  seconds: number;
  frame: number | null;
};

let time = 0;
let plan: Plan = { period: null, loops: 1, seconds: 4, frame: null };
let pending = false;
/// Whether the frame on screen is out of date. A still holds one image for as
/// long as nobody touches a control, and asking the backend to redraw it a
/// dozen times a second lit up a core for nothing — every one of those frames
/// lifts the whole drawing into a solid again.
let stale = false;

const invalidate = () => {
  stale = true;
};

/// Re-asks the backend what an export would do, and reshapes the grid to suit.
async function refreshPlan() {
  if (!hasDrawing()) return;
  try {
    plan = await invoke<Plan>("plan", { request: request() });
  } catch {
    plan = { period: null, loops: 1, seconds: slider("seconds"), frame: null };
  }
  updateHint();
  fitPreview();
  invalidate();
}

async function tick() {
  if (!hasDrawing() || pending) return;
  pending = true;
  // Cleared here rather than by the caller, so a change that arrives while a
  // frame is still in flight survives to the next pass instead of being lost.
  stale = false;
  try {
    preview.textContent = await invoke<string>("preview", {
      request: request(),
      time,
    });
  } catch (error) {
    preview.textContent = String(error);
  } finally {
    pending = false;
  }
}

setInterval(() => {
  const { period, loops, seconds } = plan;
  if (!state.still && period && seconds > 0) {
    const rate = (loops * period) / seconds;
    time = (time + (FRAME_MS / 1000) * rate) % period;
    invalidate();
  }
  if (stale) tick();
}, FRAME_MS);

function updateHint() {
  if (state.format === "gif" && slider("seconds") > 12) {
    hint.textContent =
      "a GIF this long usually lands over X's 15 MB limit — MP4 holds up better past ~12s";
  } else if (state.format === "txt" || state.format === "png") {
    hint.textContent = "a still — only the first frame is written";
  } else {
    const { columns, rows } = grid();
    hint.textContent = `${columns}×${rows} cells`;
  }
}

function bindSliders() {
  for (const name of SLIDERS) {
    const input = element<HTMLInputElement>(name);
    const output = element(`${name}-value`);
    const show = () => {
      output.textContent = String(slider(name));
    };
    show();
    input.addEventListener("input", () => {
      show();
      invalidate();
      if (name === "detail") {
        fitPreview();
        updateHint();
      }
      // Both of these move how fast the export turns, which is what the preview
      // paces itself by.
      if (name === "turns" || name === "seconds") refreshPlan();
    });
  }
}

/// Turning and zooming happen on the preview rather than on three more sliders.
///
/// They are the controls a drawing is actually framed with, and a slider is a
/// poor handle for an angle: finding the view you want means reading a number,
/// guessing, and looking again. Dragging is the same act as the thing it does.
function bindCamera() {
  let dragging = false;
  let lastX = 0;
  let lastY = 0;

  screen.addEventListener("pointerdown", (event) => {
    if (!hasDrawing()) return;
    dragging = true;
    lastX = event.clientX;
    lastY = event.clientY;
    screen.setPointerCapture(event.pointerId);
    screen.classList.add("turning");
  });

  screen.addEventListener("pointermove", (event) => {
    if (!dragging) return;
    // A quarter degree a pixel: a drag across the pane is most of a full turn,
    // and a few pixels is still a nudge.
    state.yaw = wrap(state.yaw + (event.clientX - lastX) * 0.25);
    state.pitch = clamp(state.pitch - (event.clientY - lastY) * 0.25, -90, 90);
    lastX = event.clientX;
    lastY = event.clientY;
    invalidate();
  });

  const release = (event: PointerEvent) => {
    if (!dragging) return;
    dragging = false;
    screen.releasePointerCapture(event.pointerId);
    screen.classList.remove("turning");
  };
  screen.addEventListener("pointerup", release);
  screen.addEventListener("pointercancel", release);

  screen.addEventListener(
    "wheel",
    (event) => {
      if (!hasDrawing()) return;
      event.preventDefault();
      // Multiplicative, so a notch is the same proportion of the size at every
      // zoom rather than a bigger jump the further out you are.
      state.zoom = clamp(state.zoom * Math.exp(-event.deltaY * 0.002), 0.25, 4);
      invalidate();
    },
    { passive: false },
  );

  screen.addEventListener("dblclick", () => {
    Object.assign(state, HOME);
    invalidate();
  });
}

const clamp = (value: number, low: number, high: number) =>
  Math.min(Math.max(value, low), high);

/// Yaw runs off either end of a turn, so it comes back round rather than
/// stopping — nothing about the model has a front.
const wrap = (degrees: number) => ((((degrees + 180) % 360) + 360) % 360) - 180;

function bindSegmented(id: string, onPick: (value: string) => void) {
  const container = element(id);
  container.querySelectorAll("button").forEach((button) => {
    button.addEventListener("click", () => {
      container.querySelectorAll("button").forEach((b) => b.classList.remove("on"));
      button.classList.add("on");
      onPick(button.dataset.motion ?? button.dataset.format ?? "");
    });
  });
}

element("open").addEventListener("click", async () => {
  const picked = await open({
    multiple: false,
    filters: [{ name: "ASCII drawing", extensions: ["txt", "asc", "ans"] }],
  });
  if (typeof picked !== "string") return;
  state.file = picked;
  state.text = null;
  await loadDrawing(picked.split("/").pop() ?? picked);
});

element("sample").addEventListener("click", async () => {
  state.file = null;
  state.text = SAMPLE;
  await loadDrawing("sample diamond");
});

async function loadDrawing(label: string) {
  fileLabel.textContent = label;
  status.className = "status";
  status.textContent = "";
  time = 0;
  await refreshPlan();
  fitPreview();
  invalidate();
  tick();
}

renderButton.addEventListener("click", async () => {
  if (!hasDrawing()) {
    status.className = "status error";
    status.textContent = "open a drawing first";
    return;
  }

  const target = await save({
    defaultPath: `asciiary.${state.format}`,
    filters: [{ name: state.format.toUpperCase(), extensions: [state.format] }],
  });
  if (!target) return;

  renderButton.disabled = true;
  const started = Date.now();
  const ticker = setInterval(() => {
    status.className = "status";
    status.textContent = `rendering… ${((Date.now() - started) / 1000).toFixed(1)}s`;
  }, 100);

  try {
    const path = await invoke<string>("render_art", { request: request(target) });
    status.className = "status done";
    status.textContent = `wrote ${path.split("/").pop()} in ${((Date.now() - started) / 1000).toFixed(1)}s`;
  } catch (error) {
    status.className = "status error";
    status.textContent = String(error);
  } finally {
    clearInterval(ticker);
    renderButton.disabled = false;
  }
});

bindSliders();
bindCamera();
bindSegmented("motion", (value) => {
  state.still = value === "still";
  refreshPlan();
});
bindSegmented("format", (value) => {
  state.format = value;
  updateHint();
});
buildThemes();
applyTheme(THEMES[0]);
updateHint();
new ResizeObserver(fitPreview).observe(screen);
fitPreview();
preview.textContent = "open a drawing to begin";
