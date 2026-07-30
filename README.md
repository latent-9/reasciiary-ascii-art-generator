# Asciiary

Turns a flat ASCII drawing into a solid you can turn, light and record.

The renderers this borrows from all start with a mesh somebody modelled. This
starts with a text file, so it needs one rule they do not: what the third
dimension of a drawing actually *is*. The rule is ink. A glyph that fills more of
its cell stands taller — `@` rises, `.` barely lifts, a space is a hole — and the
result is lit, projected back onto a character grid, and written out as an MP4,
a GIF, a PNG or more text.

## Choosing a character

A ramp — order glyphs by weight, take a cell's brightness, index in — is what
[emilwidlund/ASCII](https://github.com/emilwidlund/ASCII) does, and it cannot do
better than one value a cell, so an edge comes out as a staircase.
[alecjacobson/ascii3d](https://github.com/alecjacobson/ascii3d) rasterises finer
than the grid and gives each cell the character whose own bitmap looks most like
that patch. This follows the second: `/` wins a cell because the ink in `/` lies
where the light in that cell lies, which is where the sloped edges and the traced
silhouettes come from. The ramp is still there for cells with nothing to match —
flat ones — and it is measured from the same bitmaps rather than assumed.

## Running it

```sh
bun install
bun run tauri dev
```

Open a drawing, or press **Sample** for one that ships with the window. Drag the
preview to turn the solid, scroll to zoom, double-click to face it again. Four
sliders carry the rest: how far the ink stands out, how many cells the render
gets, and how many turns over how many seconds.

The preview does not chase the spin a frame at a time. One loop is rendered in
full, then played from memory at the rate that covers it exactly once, so what
the window shows is the animation an export writes rather than an approximation
of it that stutters when the machine is busy.

`ffmpeg` has to be installed for GIF and MP4; PNG and TXT need nothing.

```sh
brew install ffmpeg
```

It is looked for on the PATH first, then in the usual install prefixes — a
bundled app inherits almost no PATH from Finder, so a copy under
`/opt/homebrew/bin` is found without one. `ASCIIARY_FFMPEG` overrides both.

## The command line

The same pipeline, driven by a typed line:

```sh
cargo run --bin asciiary -- ascii logo.txt --depth 12 --spin 2 > out.mp4
```

A line is a source, the filters it flows through, and where the result lands.
The extension after `>` picks the format.

```text
ascii logo.txt --depth 12 | crt --curve 0.2 > out.mp4
```

| Flag | | Default |
| --- | --- | --- |
| `--depth` | how far the heaviest glyph stands out, in cell widths | `8` |
| `--zoom` | | `0.92` |
| `--yaw` `--pitch` | degrees | `34` `29` |
| `--spin` | radians a second, or `--still` for no motion | `1.2` |
| `--duration` `--fps` | how long the file runs, and how smoothly | `4` `20` |
| `--columns` `--rows` | the character grid | `160` `48` |
| `--scale` | pixels per point | `2` |
| `--ink` `--paper` | the two colours, as hex | `#e7e7e7` `#0c0c0e` |

An animated export is rounded to a whole number of turns so it ends where it
began, which is what lets a GIF loop without a seam. The window shows the turn
count it landed on.

## Layout

```text
src/                 the window: one panel, one preview, three themes
src-tauri/src/
  lib.rs             the Tauri commands the window calls
  repl/              the command language behind the typed line
  art/
    generators/      tools that produce a frame — `ascii` is the 3D lift
    filters/         post-processing, glyph domain and pixel domain
    canvas.rs        the character grid, and the ink ramp heights are read from
    glyphs.rs        which character a shaded cell comes out as
    paint.rs         grid to pixels
    export.rs        ffmpeg, and the numbers behind each argument
```

Adding a tool is one line in `generator::registry`, and a filter is one line in
`filter::registry`.

The font is [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono), under
the SIL Open Font License — see `src-tauri/assets/OFL.txt`.
