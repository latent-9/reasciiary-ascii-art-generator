import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

import { defaults, TOOLS } from "./tools";
import type { Camera, Control, Group, Tool } from "./tools";

/// A scheme is the two colours the render is actually made of, and the window
/// is dressed in them so what is on screen is what lands in the file. There
/// were six terminal palettes here, which coloured the chrome and nothing else
/// — every one of them exported the same near-white on near-black.
///
/// A scheme's ink is a starting point for the object rather than the last word
/// on it; see [applyObject].
type Theme = { name: string; ink: string; paper: string };

const THEMES: Theme[] = [
  { name: "Ink", ink: "#0b0b0b", paper: "#f6f5f2" },
  { name: "Paper", ink: "#fbfbfb", paper: "#080808" },
  { name: "Bone", ink: "#f2e3bd", paper: "#0a0908" },
];

/// What the export writes, which is the same question whatever tool answered
/// it — so it is one group at the foot of the panel rather than a copy in each
/// of five tables.
const OUTPUT: Group = {
  title: "Output",
  controls: [
    { kind: "range", flag: "detail", label: "Detail", min: 60, max: 240, step: 4, value: 160 },
    { kind: "range", flag: "duration", label: "Length", min: 1, max: 30, step: 1, value: 4 },
    {
      kind: "choice",
      flag: "format",
      label: "File",
      value: "mp4",
      options: ["mp4", "gif", "png", "txt"],
      caps: true,
    },
  ],
};

const element = <T extends HTMLElement>(id: string) =>
  document.getElementById(id) as T;

const preview = element<HTMLPreElement>("preview");
const screen = document.querySelector<HTMLDivElement>(".screen")!;
const status = element("status");
const hint = element("hint");
const blurb = element("blurb");
const options = element("options");
const fileLabel = element("file");
const openButton = element<HTMLButtonElement>("open");
const renderButton = element<HTMLButtonElement>("render");

/// Everything about a tool that survives switching away from it.
///
/// Kept per tool rather than in one pile, so turning the solid, opening a
/// picture and then coming back finds the solid at the angle it was left at.
type Session = {
  values: Record<string, string>;
  camera: Camera;
};

const sessions = new Map<string, Session>();

function session(tool: Tool = state.tool): Session {
  const found = sessions.get(tool.name);
  if (found) return found;
  const fresh: Session = {
    values: defaults(tool.groups),
    camera: { ...(tool.camera ?? { yaw: 0, pitch: 0, zoom: 1 }) },
  };
  sessions.set(tool.name, fresh);
  return fresh;
}

/// The file every tool that reads one is looking at.
///
/// Shared rather than kept per tool, which is what the settings beside it are.
/// A setting belongs to the tool that offers it — a depth means nothing to the
/// flat read — but the file does not: the two tools are the same question asked
/// with and without the third dimension, and the whole use of asking it flat
/// first is that it is about the file you are going to lift. Kept apart the
/// answer was to open it twice.
const source: {
  file: string | null;
  /// A drawing carried inline instead of read from a path — the built-in
  /// sample, which is what the window opens on.
  text: string | null;
} = { file: null, text: TOOLS[0].source?.sample ?? null };

const state = {
  tool: TOOLS[0],
  /// The output group's values, which belong to the window rather than to any
  /// one tool: a choice of MP4 is a choice about exporting, and following the
  /// tool around would mean making it five times.
  output: defaults([OUTPUT]),
  theme: THEMES[0],
  /// What the drawing is rendered in. A scheme proposes one and this holds it
  /// until the swatch is used, after which it is whatever was picked.
  object: THEMES[0].ink,
};

/// A tool that reads a file has nothing to draw until it has one.
const ready = () => {
  const tool = state.tool;
  return !tool.source || source.file !== null || source.text !== null;
};

const number = (flag: string) => Number(read(flag));

/// Which side of the panel a flag's value is kept on. Nothing at all is what a
/// switch that is off reads as, so the answer can be missing.
function read(flag: string): string | undefined {
  return flag in state.output ? state.output[flag] : session().values[flag];
}

function write(flag: string, value: string | null) {
  const into = flag in state.output ? state.output : session().values;
  if (value === null) delete into[flag];
  else into[flag] = value;
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
///
/// A subject that was already made at a grid keeps it. Somebody's drawing was
/// written at a size, and laying it on a larger one leaves it a small figure in
/// the middle of an empty frame — so the count the slider carries is not asked
/// for. The bounds still hold: a drawing wider than the window will render is
/// scaled down to fit like anything else.
function grid() {
  if (plan.grid) {
    const [columns, rows] = plan.grid;
    return { columns: clamp(columns, 20, 400), rows: clamp(rows, 8, 200) };
  }
  const detail = number("detail");
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
  // Picking a scheme is picking both of its colours, so the object goes back to
  // the one it proposes — a tint chosen against black would otherwise follow the
  // scheme onto white and be invisible there.
  applyObject(theme.ink);
}

/// The colour of the drawing itself.
///
/// Nothing is re-rendered for this. The frames are characters and the preview is
/// text, so the colour lives entirely in a CSS property until an export reads it
/// — which is why the object can be recoloured while a loop is playing without
/// so much as a dropped frame.
function applyObject(colour: string) {
  state.object = colour;
  document.documentElement.style.setProperty("--object", colour);
  element<HTMLInputElement>("tint").value = colour;
}

function buildThemes() {
  const container = element("themes");
  THEMES.forEach((theme, index) => {
    const button = document.createElement("button");
    button.className = "swatch";
    button.title = theme.name;
    // Split down the middle rather than filled: a scheme is two colours, and a
    // dot showing only its ink says nothing about what that ink lands on.
    button.style.background = `linear-gradient(135deg, ${theme.ink} 0 50%, ${theme.paper} 50% 100%)`;
    if (index === 0) button.classList.add("on");
    button.addEventListener("click", () => {
      container.querySelectorAll("button").forEach((b) => b.classList.remove("on"));
      button.classList.add("on");
      applyTheme(theme);
    });
    container.appendChild(button);
  });
}

/* The panel */

const title = (word: string) => word[0].toUpperCase() + word.slice(1);

/// The tool's own groups, plus the one group that is nobody's tool flag: what
/// the export writes.
function panelGroups(tool: Tool): Group[] {
  return [...tool.groups, OUTPUT];
}

function row(label: string, control: HTMLElement) {
  const line = document.createElement("div");
  line.className = "row";
  const name = document.createElement("span");
  name.textContent = label;
  line.append(name, control);
  return line;
}

function buildControl(control: Control): HTMLElement {
  const holder = document.createElement("div");
  holder.className = "control";

  if (control.kind === "range") {
    const value = document.createElement("output");
    const input = document.createElement("input");
    input.type = "range";
    input.min = String(control.min);
    input.max = String(control.max);
    input.step = String(control.step);
    input.value = read(control.flag) ?? String(control.value);
    const show = () => {
      value.textContent = input.value;
    };
    show();
    input.addEventListener("input", () => {
      show();
      write(control.flag, input.value);
      changed(control.flag);
    });
    holder.append(row(control.label, value), input);
    return holder;
  }

  if (control.kind === "choice") {
    const popup = document.createElement("span");
    popup.className = "popup";
    const select = document.createElement("select");
    for (const option of control.options) {
      const item = document.createElement("option");
      item.value = option;
      item.textContent = control.caps ? option.toUpperCase() : title(option);
      select.append(item);
    }
    select.value = read(control.flag) ?? control.value;
    select.addEventListener("change", () => {
      write(control.flag, select.value);
      changed(control.flag);
    });
    popup.append(select);
    holder.append(row(control.label, popup));
    return holder;
  }

  const toggle = document.createElement("label");
  toggle.className = "toggle";
  const box = document.createElement("input");
  box.type = "checkbox";
  box.checked = read(control.flag) !== undefined;
  box.addEventListener("change", () => {
    // A flag that carries no value is on by being there at all, so switching it
    // off means taking it out rather than setting it to something falsy.
    write(control.flag, box.checked ? "" : null);
    changed(control.flag);
  });
  toggle.append(box, document.createElement("i"));
  holder.append(row(control.label, toggle));
  return holder;
}

function buildPanel() {
  const tool = state.tool;
  blurb.textContent = tool.blurb;
  options.replaceChildren(
    ...panelGroups(tool).map((group) => {
      const section = document.createElement("section");
      const heading = document.createElement("h2");
      heading.textContent = group.title;
      section.append(heading, ...group.controls.map(buildControl));
      return section;
    }),
  );
}

function buildTools() {
  const container = element("tools");
  TOOLS.forEach((tool, index) => {
    const button = document.createElement("button");
    button.textContent = tool.label;
    if (index === 0) button.classList.add("on");
    button.addEventListener("click", () => {
      if (state.tool === tool) return;
      container.querySelectorAll("button").forEach((b) => b.classList.remove("on"));
      button.classList.add("on");
      pickTool(tool);
    });
    container.append(button);
  });
}

async function pickTool(tool: Tool) {
  subject += 1;
  state.tool = tool;
  buildPanel();
  openButton.hidden = tool.source === undefined;
  if (tool.source) openButton.textContent = `Open ${tool.source.label.toLowerCase()}…`;
  screen.classList.toggle("still", tool.camera === undefined);
  showSource();
  updateHint();
  if (!ready()) {
    film = { frames: [`open a ${tool.source?.label.toLowerCase()} to begin`], fps: 1 };
    shown = -1;
    fitPreview();
    return;
  }
  // Nothing of the tool that was on screen a moment ago survives the switch,
  // however long the new one takes to arrive.
  film = { frames: [], fps: 1 };
  preview.textContent = "";
  await settle();
}

/* Talking to the backend */

function request(withOutput?: string) {
  const tool = state.tool;
  const here = session();
  const seconds = number("duration");
  const { columns, rows } = grid();

  const flags: Record<string, string> = { ...here.values };

  if (tool.camera) {
    flags.yaw = String(Math.round(here.camera.yaw));
    flags.pitch = String(Math.round(here.camera.pitch));
    flags.zoom = here.camera.zoom.toFixed(3);
  }

  flags.columns = String(columns);
  flags.rows = String(rows);
  flags.duration = String(seconds);
  flags.ink = state.object;
  flags.paper = state.theme.paper;
  if (source.text) flags.text = source.text;

  return {
    tool: tool.name,
    positional: source.file && !source.text ? [source.file] : [],
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
  if (!ready()) {
    preview.style.fontSize = "12px";
    return;
  }
  const { columns, rows } = grid();
  const width = (screen.clientWidth - 24) / (columns * cell("advance"));
  const height = (screen.clientHeight - 24) / (rows * cell("line"));
  preview.style.fontSize = `${Math.max(Math.min(width, height), 1)}px`;
}

/// What the export will do, mirrored so the preview shows the same frames.
type Plan = {
  period: number | null;
  loops: number;
  seconds: number;
  frame: number | null;
  /// Columns and rows the subject was made at, when it was made at any.
  grid: [number, number] | null;
};

/// One whole loop, already rendered.
type Film = { frames: string[]; fps: number };

let plan: Plan = { period: null, loops: 1, seconds: 4, frame: null, grid: null };
let film: Film = { frames: [], fps: 1 };
let shown = -1;

/// The backend call in flight, if there is one.
///
/// One at a time: a render is not cheap and the window can ask for one faster than
/// it can be answered — dragging the camera asks once a pixel. A drag drops the
/// asks it cannot keep up with, which is what a drag wants. A tool switch waits for
/// the line to clear instead, because no later ask is coming to make up for it.
let pending: Promise<unknown> | null = null;

/// Which subject the window is on.
///
/// A render is asked for and answered some time later, and the tool can change in
/// between — so an answer is checked against the count it was asked under and
/// dropped if that has moved. Without it the frames of the tool just left arrive
/// into the pane of the one just chosen, over the top of the line saying there is
/// nothing to draw yet.
let subject = 0;

const quiet = () => pending?.catch(() => {});

/// Re-asks the backend what an export would do, and reshapes the grid to suit.
async function refreshPlan() {
  const mark = subject;
  let answer: Plan;
  try {
    answer = await invoke<Plan>("plan", { request: request() });
  } catch {
    answer = { period: null, loops: 1, seconds: number("duration"), frame: null, grid: null };
  }
  if (mark !== subject) return;
  plan = answer;
  updateHint();
  fitPreview();
}

/// How long a change is left to settle before the loop behind it is rendered.
///
/// Long enough that dragging a slider across its range asks for one film rather
/// than forty, short enough that letting go feels immediate.
const SETTLE_MS = 180;
let settling: number | undefined;

/// Rebuilds the loop after whatever is being changed stops changing.
function invalidate() {
  if (!ready()) return;
  clearTimeout(settling);
  settling = setTimeout(settle, SETTLE_MS);
}

async function settle() {
  if (!ready()) return;
  const mark = subject;
  // Whatever is in flight belongs to the tool that was on screen a moment ago. Its
  // answer is going to be thrown away, but the guard holding the backend to one
  // call at a time would otherwise drop this ask rather than that one — and leave
  // the pane blank, since there is nothing else coming to fill it.
  await quiet();
  if (mark !== subject || !ready()) return;
  await refreshPlan();
  // One frame before the whole loop. A field of six hundred strokes takes long
  // enough to draw a hundred and sixty of them that the pane would otherwise go
  // on showing the last tool's work for seconds after the panel had changed —
  // and the first frame is a hundred and sixtieth of that wait.
  await showFrame();
  await loadFilm();
}

/// What a control does beyond carrying its own value.
///
/// Detail changes the grid and nothing else, so the preview is refitted at once
/// rather than after the render; the file format changes neither, so it only
/// reprints the line at the bottom. Everything else means new frames.
function changed(flag: string) {
  if (flag === "detail") fitPreview();
  updateHint();
  if (flag !== "format") invalidate();
}

async function loadFilm() {
  if (!ready() || pending) return;
  const mark = subject;
  // Long enough on the heavier tools to be worth saying so, and it is the same
  // line an export reports into, so nothing new has to be found room for.
  if (!renderButton.disabled) {
    status.className = "status";
    status.textContent = "drawing…";
  }
  const call = invoke<Film>("sequence", { request: request() });
  pending = call;
  try {
    const frames = await call;
    if (mark !== subject) return;
    film = frames;
    shown = -1;
  } catch (error) {
    if (mark !== subject) return;
    film = { frames: [String(error)], fps: 1 };
    shown = -1;
  } finally {
    pending = null;
    if (status.textContent === "drawing…") status.textContent = "";
  }
}

/// Plays the loop off the animation clock.
///
/// Which frame belongs on screen is a question about elapsed time, so it is
/// asked that way — rather than by counting ticks, which loses count the moment
/// one of them runs late. Nothing here waits on the backend: the frames are
/// already in hand, so a slow render can no longer show up as a stutter.
///
/// Reading the clock rather than holding a position also means a film that gets
/// rebuilt picks up where the last one was in its own cycle, so changing the
/// depth or the palette does not jog the spin.
function play(now: number) {
  requestAnimationFrame(play);
  const { frames, fps } = film;
  if (frames.length === 0) return;

  const index =
    frames.length === 1
      ? 0
      : Math.floor((now / 1000) * fps) % frames.length;
  if (index === shown) return;
  shown = index;
  preview.textContent = frames[index];
}
requestAnimationFrame(play);

/// One frame, now, for while the camera is being dragged.
///
/// Turning the model by hand wants an answer per movement, not a whole loop per
/// movement — the loop it would render is one nobody is going to see, because
/// the next drag event replaces it.
async function showFrame() {
  if (!ready() || pending) return;
  const mark = subject;
  const call = invoke<string>("preview", { request: request(), time: 0 });
  pending = call;
  try {
    const frame = await call;
    if (mark !== subject) return;
    film = { frames: [frame], fps: 1 };
    shown = -1;
  } catch (error) {
    if (mark === subject) preview.textContent = String(error);
  } finally {
    pending = null;
  }
}

function updateHint() {
  const format = state.output.format;
  if (format === "gif" && number("duration") > 12) {
    hint.textContent = "a GIF this long usually lands over X's 15 MB limit";
  } else if (format === "txt" || format === "png") {
    hint.textContent = "a still — only the first frame is written";
  } else {
    const { columns, rows } = grid();
    hint.textContent = `${columns}×${rows} · ${plan.loops} × ${plan.period?.toFixed(1) ?? number("duration")}s`;
  }
}

/// The line at bottom left: what is being drawn, and how to frame it.
function showSource() {
  const tool = state.tool;
  const parts: string[] = [];
  if (tool.source) {
    if (source.file) parts.push(source.file.split("/").pop() ?? source.file);
    else if (source.text) parts.push("built-in sample");
    else parts.push(`no ${tool.source.label.toLowerCase()} yet`);
  }
  if (tool.camera) parts.push("drag to turn · scroll to zoom · double-click to reset");
  fileLabel.textContent = parts.join("   ·   ");
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

  const turnable = () => ready() && state.tool.camera !== undefined;

  screen.addEventListener("pointerdown", (event) => {
    if (!turnable()) return;
    dragging = true;
    lastX = event.clientX;
    lastY = event.clientY;
    screen.setPointerCapture(event.pointerId);
    screen.classList.add("turning");
  });

  screen.addEventListener("pointermove", (event) => {
    if (!dragging) return;
    const camera = session().camera;
    // A quarter degree a pixel: a drag across the pane is most of a full turn,
    // and a few pixels is still a nudge.
    camera.yaw = wrap(camera.yaw + (event.clientX - lastX) * 0.25);
    camera.pitch = clamp(camera.pitch - (event.clientY - lastY) * 0.25, -90, 90);
    lastX = event.clientX;
    lastY = event.clientY;
    showFrame();
  });

  const release = (event: PointerEvent) => {
    if (!dragging) return;
    dragging = false;
    screen.releasePointerCapture(event.pointerId);
    screen.classList.remove("turning");
    // Back to a moving picture now the angle has been settled on.
    invalidate();
  };
  screen.addEventListener("pointerup", release);
  screen.addEventListener("pointercancel", release);

  screen.addEventListener(
    "wheel",
    (event) => {
      if (!turnable()) return;
      event.preventDefault();
      const camera = session().camera;
      // Multiplicative, so a notch is the same proportion of the size at every
      // zoom rather than a bigger jump the further out you are.
      camera.zoom = clamp(camera.zoom * Math.exp(-event.deltaY * 0.002), 0.25, 4);
      showFrame();
      invalidate();
    },
    { passive: false },
  );

  screen.addEventListener("dblclick", () => {
    if (!turnable()) return;
    Object.assign(session().camera, state.tool.camera);
    invalidate();
  });
}

const clamp = (value: number, low: number, high: number) =>
  Math.min(Math.max(value, low), high);

/// Yaw runs off either end of a turn, so it comes back round rather than
/// stopping — nothing about the model has a front.
const wrap = (degrees: number) => ((((degrees + 180) % 360) + 360) % 360) - 180;

openButton.addEventListener("click", async () => {
  const offered = state.tool.source;
  if (!offered) return;
  const picked = await open({
    multiple: false,
    filters: [{ name: offered.label, extensions: offered.extensions }],
  });
  if (typeof picked !== "string") return;
  // A different file is a different subject, so a render of the last one is of no
  // more use than a render of the last tool.
  subject += 1;
  source.file = picked;
  source.text = null;
  status.className = "status";
  status.textContent = "";
  showSource();
  await settle();
});

async function render() {
  if (!ready()) {
    status.className = "status error";
    status.textContent = "open something first";
    return;
  }

  const format = state.output.format;
  const target = await save({
    defaultPath: `${state.tool.name}.${format}`,
    filters: [{ name: format.toUpperCase(), extensions: [format] }],
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
}

renderButton.addEventListener("click", render);

// The shortcut every Mac app renders with.
window.addEventListener("keydown", (event) => {
  if (event.metaKey && event.key === "r") {
    event.preventDefault();
    if (!renderButton.disabled) render();
  }
});

bindCamera();
buildTools();
buildThemes();
element<HTMLInputElement>("tint").addEventListener("input", (event) => {
  applyObject((event.target as HTMLInputElement).value);
});
applyTheme(THEMES[0]);
new ResizeObserver(fitPreview).observe(screen);
pickTool(state.tool);
