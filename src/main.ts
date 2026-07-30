import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

type Theme = {
  name: string;
  bg: string;
  panel: string;
  fg: string;
  dim: string;
  accent: string;
  line: string;
};

/// The same schemes Ghostty ships, which come from iTerm2-Color-Schemes.
const THEMES: Theme[] = [
  { name: "Tokyo Night", bg: "#1a1b26", panel: "#16161e", fg: "#c0caf5", dim: "#565f89", accent: "#7aa2f7", line: "#232433" },
  { name: "Catppuccin", bg: "#1e1e2e", panel: "#181825", fg: "#cdd6f4", dim: "#6c7086", accent: "#89b4fa", line: "#313244" },
  { name: "Nord", bg: "#2e3440", panel: "#272c36", fg: "#d8dee9", dim: "#616e88", accent: "#88c0d0", line: "#3b4252" },
  { name: "Gruvbox", bg: "#282828", panel: "#1d2021", fg: "#ebdbb2", dim: "#928374", accent: "#fabd2f", line: "#3c3836" },
  { name: "Dracula", bg: "#282a36", panel: "#21222c", fg: "#f8f8f2", dim: "#6272a4", accent: "#bd93f9", line: "#343746" },
  { name: "Rosé Pine", bg: "#191724", panel: "#1f1d2e", fg: "#e0def4", dim: "#6e6a86", accent: "#ebbcba", line: "#26233a" },
];

const element = <T extends HTMLElement>(id: string) =>
  document.getElementById(id) as T;

const preview = element<HTMLPreElement>("preview");
const screen = document.querySelector<HTMLDivElement>(".screen")!;
const status = element("status");
const hint = element("hint");
const fileLabel = element("file");
const renderButton = element<HTMLButtonElement>("render");

const SLIDERS = ["depth", "zoom", "yaw", "pitch", "spin", "duration", "fps", "columns", "rows"] as const;
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

const state = {
  file: null as string | null,
  text: null as string | null,
  still: false,
  format: "mp4",
};

const hasDrawing = () => state.file !== null || state.text !== null;

function slider(name: SliderName): number {
  return Number(element<HTMLInputElement>(name).value);
}

function applyTheme(theme: Theme) {
  const root = document.documentElement.style;
  root.setProperty("--bg", theme.bg);
  root.setProperty("--panel", theme.panel);
  root.setProperty("--fg", theme.fg);
  root.setProperty("--dim", theme.dim);
  root.setProperty("--accent", theme.accent);
  root.setProperty("--line", theme.line);
}

function buildThemes() {
  const container = element("themes");
  THEMES.forEach((theme, index) => {
    const button = document.createElement("button");
    button.innerHTML = `<span class="swatch" style="background:${theme.accent}"></span>${theme.name}`;
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
  const flags: Record<string, string> = {
    depth: String(slider("depth")),
    zoom: String(slider("zoom")),
    yaw: String(slider("yaw")),
    pitch: String(slider("pitch")),
    spin: String(slider("spin")),
    duration: String(slider("duration")),
    fps: String(slider("fps")),
    columns: String(slider("columns")),
    rows: String(slider("rows")),
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

/// Scales the preview so the whole grid is visible. JetBrains Mono's advance is
/// almost exactly 0.6em, which is close enough to fit without measuring.
function fitPreview() {
  if (!hasDrawing()) {
    preview.style.fontSize = "12px";
    return;
  }
  const columns = slider("columns");
  const rows = slider("rows");
  const width = (screen.clientWidth - 24) / (columns * 0.6);
  const height = (screen.clientHeight - 24) / rows;
  preview.style.fontSize = `${Math.max(Math.min(width, height), 1)}px`;
}

let time = 0;
let period: number | null = null;
let pending = false;

async function refreshPeriod() {
  if (!hasDrawing()) return;
  try {
    period = await invoke<number | null>("loop_duration", { request: request() });
  } catch {
    period = null;
  }
}

async function tick() {
  if (!hasDrawing() || pending) return;
  pending = true;
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
  if (!state.still && period) {
    time = (time + 0.08) % period;
  }
  tick();
}, 80);

function updateHint() {
  if (state.format === "gif" && slider("duration") > 12) {
    hint.textContent =
      "a GIF this long usually lands over X's 15 MB limit — MP4 holds up better past ~12s";
  } else if (state.format === "txt" || state.format === "png") {
    hint.textContent = "a still — only the first frame is written";
  } else {
    hint.textContent = "";
  }
}

function bindSliders() {
  for (const name of SLIDERS) {
    const input = element<HTMLInputElement>(name);
    const output = element(`${name}-value`);
    const show = () => {
      const value = slider(name);
      output.textContent =
        name === "yaw" || name === "pitch"
          ? `${value}°`
          : name === "zoom" || name === "spin"
            ? value.toFixed(2)
            : String(value);
    };
    show();
    input.addEventListener("input", () => {
      show();
      if (name === "columns" || name === "rows") fitPreview();
      if (name === "spin") refreshPeriod();
      if (name === "duration") updateHint();
    });
  }
}

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
  await refreshPeriod();
  fitPreview();
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
bindSegmented("motion", (value) => {
  state.still = value === "still";
  refreshPeriod();
});
bindSegmented("format", (value) => {
  state.format = value;
  updateHint();
});
buildThemes();
applyTheme(THEMES[0]);
new ResizeObserver(fitPreview).observe(screen);
fitPreview();
preview.textContent = "open a drawing to begin";
