# Asciiary

Turns a flat ASCII drawing into a solid you can turn, light and record.

The renderers this borrows from all start with a mesh somebody modelled. This
starts with a text file, so it needs one rule they do not: what the third
dimension of a drawing actually *is*. The rule is ink. A glyph that fills more of
its cell stands taller — `@` rises, `.` barely lifts, a space is a hole — and the
result is lit, projected back onto a character grid, and written out as an MP4,
a GIF, a PNG or more text.

## Running it

```sh
bun install
bun run tauri dev
```

Open a drawing, or press **Sample** for one that ships with the window. The
preview turns at the rate the export will and redraws as the controls move.

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

An animated export is rounded to a whole number of turns so it ends where it
began, which is what lets a GIF loop without a seam. The window shows the turn
count it landed on.

## Layout

```text
src/                 the window: one panel, one preview, six themes
src-tauri/src/
  lib.rs             the Tauri commands the window calls
  repl/              the command language behind the typed line
  art/
    generators/      tools that produce a frame — `ascii` is the 3D lift
    filters/         post-processing, glyph domain and pixel domain
    canvas.rs        the character grid, and the ink ramp heights are read from
    paint.rs         grid to pixels
    export.rs        ffmpeg, and the numbers behind each argument
```

Adding a tool is one line in `generator::registry`, and a filter is one line in
`filter::registry`.

The font is [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono), under
the SIL Open Font License — see `src-tauri/assets/OFL.txt`.
