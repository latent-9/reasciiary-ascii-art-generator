# Asciiary

Turns a flat ASCII drawing into a solid you can turn, light and record.

The renderers this borrows from all start with a mesh somebody modelled. This
starts with a text file, so it needs one rule they do not: what the third
dimension of a drawing actually *is*. The rule is ink. A glyph that fills more of
its cell stands taller — `@` rises, `.` barely lifts, a space is a hole — and the
result is lit, projected back onto a character grid, and written out as an MP4,
a GIF, a PNG or more text.

## What the solid is

Ink coverage gives every cell a height, and the drawing is struck *through* the
slab rather than raised off a backing plate: a column of ink `h` tall runs from
`-h/2` to `+h/2`. A plate is one flat quad with one normal spanning the whole
drawing — head-on it hides behind the ink and costs nothing, but turned past a
quarter it *is* the picture, so half of every spin arrived as a featureless mass
however carefully the front had been lit. Struck through, there is no angle with
nothing to show, at the price of the drawing being open work: every space in it
is a hole clean through.

## Where the shade comes from

The light has to be able to tell one cell from the next or the whole thing
renders as a slab. A box's top is flat, so reading its normal off the box says
nothing — every cap in the model takes exactly the same light and the relief only
survives as a step at the silhouette. The normal is taken from the drawing
instead: the gradient of the heightfield under that cell, so a cap leans back
against the ink rising beside it and catches the key light the way the slope it
stands on would.

That gradient is a Sobel rather than a difference of the two neighbours either
side. ASCII art dithers — `#@#@` is a tone, not a staircase — and a stencil one
cell wide reads a cliff at every cell, which breaks the surface into noise.
Asking the same question of a three by three patch cancels the alternation, the
way the eye does with it. A wall gets the same treatment from the other end: its
normal is rolled a third of the way off the face it descends from, because a rim
on a cut relief turns over into its own side rather than meeting it at a knife
edge, and a bare axis is one normal shared by every wall in the drawing that runs
the same way.

Which way a surface faces is the whole of direct lighting, and it cannot tell the
floor of a pit from the same floor out in the open: both look at the sky, both
take the same shade, both come out the same character. So the heightfield is
asked a second question. A neighbour standing `rise` above a cell `run` away
walls off everything below `rise / run`, and the steepest such ratio along each
of eight directions says how much sky the cell has left. That belongs to the
drawing rather than to the angle it is seen from, which is the point — it is
still there at the yaws where the direct light has nothing left to separate.

## The camera

The eye stands off by a multiple of the solid's own reach. Under a parallel
camera a face keeps its size however far away it is, so the two ends of a turning
slab are drawn identically and nothing in the picture says which one is nearer —
a spin reads as a shape shearing about on the page rather than as a body turning
in space. The frame is fitted by carrying the bounding box's own corners through
that same projection and sweeping them over a whole turn, so the model is drawn
as large as it can be without breathing as it spins.

## Choosing a character

A ramp — order glyphs by weight, take a cell's brightness, index in — is what
[emilwidlund/ASCII](https://github.com/emilwidlund/ASCII) does, and it cannot do
better than one value a cell, so an edge comes out as a staircase.
[alecjacobson/ascii3d](https://github.com/alecjacobson/ascii3d) rasterises finer
than the grid and gives each cell the character whose own bitmap looks most like
that patch. This follows the second: `/` wins a cell because the ink in `/` lies
where the light in that cell lies, which is where the sloped edges and the traced
silhouettes come from.

Which of the two a cell gets is decided by coverage, not by anything in the
shading: the rasteriser already knows how many of its samples the solid reached,
so a cell it fills completely is interior and gets graded, and a cell it fills
partly is on the silhouette and gets matched.

The ramp is `.:-=+*#%@`, measured from the same bitmaps rather than assumed. A
longer one is only better if every step looks like the ones either side — `*`,
`+`, `?`, `!` and `|` carry nearly the same ink and look nothing alike, so a face
graded through them changes texture where it should only change tone, and reads
as noise. The silhouette is matched against strokes for the same reason: a `W` or
a `J` may fit an edge best by least squares, but what the eye does with a row of
letters is read it.

## Running it

```sh
bun install
bun run tauri dev
```

Open a drawing, or press **Sample** for one that ships with the window. Drag the
preview to turn the solid, scroll to zoom, double-click to face it again. Four
sliders carry the rest: how far the ink stands out, how many cells the render
gets, and how many turns over how many seconds.

Three schemes sit above the preview, and the swatch beside them recolours the
solid itself to anything — the scheme keeps the paper and the window's own text,
so a green object on black cannot take the controls with it. Nothing re-renders
for a colour: the frames are characters, so it costs a CSS property until an
export reads it.

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
| `--ink` `--paper` | the object and what is behind it, as hex | `#e7e7e7` `#0c0c0e` |

An animated export is rounded to a whole number of turns so it ends where it
began, which is what lets a GIF loop without a seam. The window shows the turn
count it landed on.

## Layout

```text
src/                 the window: one panel, one preview, three schemes and a tint
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
