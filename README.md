# Asciiary

Six tools and one exporter. Five of them end in a grid of characters, lit, and
written out as an MP4, a GIF, a PNG or more text. The sixth ends in pixels: the
grid is one way to land a frame rather than the point of the thing.

| Tool | |
| --- | --- |
| `loops` | a finished piece that comes back round to where it began |
| `ascii` | a drawing or a picture lifted into a solid, ink for height |
| `scene` | a sphere, torus, cube or knot cut from a formula and turned |
| `media` | a drawing, a picture or an animation read flat as glyphs |
| `gen2d` | a flow field drawn in pixels and read back as glyphs |
| `spiral` | a wave winding out from the middle, drawn by one spiralling line |

The renderers `ascii` borrows from all start with a mesh somebody modelled. It
starts with a file nobody modelled anything in, so it needs one rule they do not:
what the third dimension of a flat thing actually *is*. The rule is ink. A glyph
that fills more of its cell stands taller — `@` rises, `.` barely lifts, a space
is a hole — and the result is lit, projected back onto a character grid, and
written out.

A picture is the same rule with the reading swapped: light for ink. It is resized
to `--relief` cells across, corrected for how much taller a cell is than it is
wide, and every cell hands over how much light is in it. Past that point nothing
can tell which kind of file it was given — the heightfield, the lighting, the
camera and the alphabet are one path, not two. `--invert` decides which end of
either source is the subject, because a photograph is usually lit and a drawing
is usually dark on white paper, and `--contrast` opens the middle of the range
where a flat scan leaves the relief mumbling.

## What the solid is

Ink coverage gives every cell a height, and the drawing is struck *through* the
slab rather than raised off a backing plate: a column of ink `h` tall runs from
`-h/2` to `+h/2`. A plate is one flat quad with one normal spanning the whole
drawing — head-on it hides behind the ink and costs nothing, but turned past a
quarter it *is* the picture, so half of every spin arrived as a featureless mass
however carefully the front had been lit. Struck through, there is no angle with
nothing to show, at the price of the drawing being open work: every space in it
is a hole clean through.

`scene` skips the heightfield entirely — it hands the same renderer the quads a
formula cuts, so a torus is a torus rather than a relief of one.

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
in space. The frame is fitted by carrying the solid's own hull through that same
projection and sweeping it over a whole turn, so the model is drawn as large as
it can be without breathing as it spins.

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

Which characters the graded half runs through is `--grade`, and the three sets it
offers are a real trade rather than a preference:

| `--grade` | | |
| --- | --- | --- |
| `shades` | `.:-=+*#%@` | nine marks that thicken one into the next |
| `detailed` | `.,:;i1tfLCG08@` | fifteen, the default |
| `ink` | every printable glyph | ninety-five, ordered by coverage |

Every one of them is *measured* rather than indexed. An ordering made by eye is
never evenly spaced — `'` and `-` are nearly the same weight while `|` to `0` is
a jump — so indexing by position spends the same range of brightness on each step
and a face at half brightness comes out at whatever the middle character happens
to weigh. The bitmaps have already been rasterised, so a brightness buys the glyph
whose ink is nearest instead, and anything the ordering had out of place is put
right by the same stroke.

Nine is the safe answer and it is what a renderer whose output reads as a solid
usually uses, `donut.c`'s `.,-~:;=!*#$@` included. It is not the best answer here:
a lit body's faces sit in a narrow band of brightness, and nine steps quantise
that band into terraces the shading never put there. Fifteen resolve it and still
thicken visibly from one step to the next.

Ninety-five do not. `%`, `&`, `M` and `W` are neighbours by coverage and look
nothing alike, so a face crossing that stretch changes texture where it should
only change tone: the brightness field underneath is smooth and the picture is
not. It is offered because it is the honest maximum and it suits flat artwork,
not because a solid should be drawn with it.

None of this reaches the silhouette, which is matched against strokes whatever
`--grade` says — a `W` or a `J` may fit an edge best by least squares, but what
the eye does with a row of letters is read it. So a longer set buys shades on the
faces without costing the outline its edges.

## Reading a picture back

`media` and `gen2d` model nothing. One resamples a file and the other draws
strokes with a rasteriser; both then hand the pixels to the same reader, which is
the matcher above pointed at a picture instead of at a shaded solid. `--marks`
chooses between them by name, so the same field can come out traced or graded.

`media` takes the same files the lift does, and a drawing is the one it has least
to do to. It was written as characters already, so it is laid on the grid as
written — the artist's own `#` and `+`, not the reader's opinion of them — and
the window sizes the grid from the file rather than from the detail slider. Only
two things overrule that. Asking anything of the reading (`--marks`, `--invert`,
`--contrast`) is asking for the drawing to be read rather than shown; and a grid
too small to hold it has to shrink it, because characters do not. Either way the
drawing is drawn out as light at the reader's own five by eleven pixels a cell
and matched back, which is a resampling of the whole of it rather than a crop.

The field is 4D Perlin noise sampled on a *circle* in time rather than along a
line, so a period arrives back exactly where it started — the trick
[Bleuje's animations](https://github.com/Bleuje/processing-animations-code) turn
on. Each stroke also carries its own offset into that period and fades in and out
at both ends of it, so no frame is the one where every line restarts at once.

## The pieces

`loops` is the one tool that brings its own subject; every other tool is a way of
looking at something somebody else made. The spiral below is the near miss — it
takes a file like the rest, and is the only one of them that will stand without
one. A piece is a finished animation with a dial or two on it, made to be
exported — a loop that meets itself, at a size that can be posted.

| `--piece` | |
| --- | --- |
| `hilbert` | a space-filling curve whose blocks pivot about their own middles |
| `sinusoids` | circles packed into the frame, each with a wave running through it |
| `sierpinski` | a gasket whose three copies slide round its corners |
| `sliding` | a quadtree whose quarters slide while the whole of it doubles |
| `spherewave` | a front crossing a sphere of loose elements, once a period |
| `toruscurve` | a swell travelling along a tube wrapped round a knotted curve |

Four of them paint into the same sub-cell raster `gen2d` uses and are read back
by the same matcher; two hand quads to the lit renderer, the way `scene` does.
Each takes its own dial where it wants one — `--order`, `--count`, `--depth`,
`--twists` — rather than every piece answering for a row that means nothing to
five of the six.

The pieces are written from the ideas in
[Bleuje's collection](https://github.com/Bleuje/processing-animations-code)
rather than from its source, which reserves its rights. What is borrowed is the
craft: phase instead of a clock, noise walked round a circle, easing that
arrives, and the taste for a figure that rearranges itself and comes back.

## The one that is not characters

`spiral` is a plane with marks over it. The plane's height is a plain sine wave
delayed by how far out a point lies *and* by which way round it lies — and a
delay that reads the angle is a spiral, so the crest winds outward instead of
ringing. How much of each delay there is settles what the piece is, and both are
asked for: `--rings` is how many times the wave repeats on the way out, `--arms`
is how many arms it winds them into. One arm is the piece as it was composed; six
is a rosette, none is rings growing out of the middle rather than a spiral, and
below none it winds the other way. Whole numbers, and not for tidiness — the
second delay is read off an angle, and an angle wraps, so anything else leaves a
crease running out of the middle of every frame. What stands over the plane is
drawn one of two ways, and which one depends on whether a file was opened.

With nothing laid on the disc it is a drift: seventeen thousand particles, each
on its own fixed ray, crawling out from the middle a little above the surface and
rising and falling with it. That is the piece as it was composed, and `--bare`
asks for it.

With a drawing or a picture laid on the disc, the drift goes and the marks stand
along one line wound out from the middle instead — an engraver's line, `--windings`
turns of it, thickening where the light is and thinning to nothing where it is
not. A scatter cannot draw a picture and was never asked to: most of a scatter
lands on the dark and is swept away, what is left holds no edge, and what arrives
is a cloud in roughly the right shape. Wound instead, every mark lands somewhere
the last one did not, the whole disc is covered once, and the subject comes back
as the subject. It is also more of a spiral than the drift ever was, rather than
less.

Each mark takes the light it is standing over as its size, the way every halftone
ever printed takes a tone, and one standing over the picture's paper is not drawn
at all. Sizing the mark is what makes a photograph arrive — a photograph is lit
nearly everywhere, so drawing the marks faintly instead would leave every one of
them standing and merely grey the line, and a greyed line is the line. A mark is
half a winding across at the full of the light, so there it closes on the
windings either side of it and the line goes solid; anything less leaves paper
showing, and that is the whole of how a tone arrives. The picture holds still
while the line turns a whole revolution through it over the loop, riding the
swell wherever the wave happens to be under it.

It takes the file the same way the other two do, and the same flags say how to
read it — `--invert` and `--contrast` for which end of it is the subject and how
hard, `--color` to give each mark the colour it is standing on instead of the
ink. `--spread` is how much of the disc it covers, and past a whole the line is
wound over the middle of it, which is a crop rather than a fit. `--fit` is the
other half of that question, and it is the flat read's word for the same thing: a
disc is round and a photograph is not, so `contain` stands the whole picture
inside the disc and leaves the near and far of it empty, while `cover` fills the
disc and lets the ends of the picture walk out past the rim. `--floor` is where
the picture stops being a subject and starts being paper: below it a mark is not
drawn at all, which is what keeps the dark of a picture dark rather than hatched
over — and where that line falls is a judgement about the picture, low for a
subject sitting in shadow and high for one meant to read as a stencil.

Before any of those are asked, the picture is opened out to its own darkest and
brightest — `--open`, a whole by default and nought to read the light exactly as
it stands. It has to come first and it has to be on: light is carried as the
*size* of a mark, so a picture whose light never rises far is a picture drawn
entirely in marks too fine to see. A screenshot of pale text on a dark field is
the worst case and the commonest one — read down to the size the line can show,
every stroke in it averages away to a few hundredths and the whole picture comes
out under the paper floor, so the frame arrives bare, with no sign a file was
ever opened. `--contrast` cannot rescue that, and this is why: it
turns about the middle of the range, so a picture with nothing near the middle is
one it can only push further down.

Which end of the picture is its subject is then found rather than assumed. A
picture whose light averages high is mostly its own paper, and it is the ink that
gets drawn. The same arithmetic is behind it: light is the size of a mark, so a
picture that is bright nearly everywhere is a line that runs solid nearly
everywhere, which is a coil with nothing in it to say a file was ever opened.
That is not an unusual file to be handed — a signature, a logo, a
diagram, a screenshot of a page, all of them a little ink on a great deal of
white. Well above a half before it says so, because a picture with tones either
side of the middle is a photograph and turning a photograph over is never what
was meant. `--invert` still has the last word: it swaps the ends of whatever was
found, so a line that knows which end it wants gets it.

The disc also settles under a picture. At its full height the wave carries the
plane about a quarter of a frame toward the eye and away again, eight times over
between the middle and the rim, and the lens magnifies what is near it and
shrinks what is far — so on a drawing each swell drags the part standing on it
outward and the next hauls it back, and the subject arrives torn into eight rings
of itself. On a scatter that reads as depth, because a scatter has no shape to
lose. So a laid picture leaves a quarter of the swell standing: enough to see the
line rise and fall, not enough to pull the drawing apart. The drift keeps the
whole of it.

A picture still wants a low `--pitch` to be read at — from overhead the disc is
a disc, and tipped far enough over to see the swells standing in front of each
other a photograph is foreshortened into a band and a page of writing is nothing.
Around 15 the subject survives and there is still a surface under it; past 40 or
so it is gone. The window's spiral tab opens there for that reason, and the drift
alone is the finer thing from up around fifty — which is a drag away.

With no file at all it is the drift alone, which is how the piece was
composed and a thing in its own right; `--bare` asks for that with one open,
because the window always has one.

The plane is drawn in the paper's own colour, so none of it is ever seen. That is
deliberate, and it is the whole reason this one ends in pixels: what the plane is
there for is to stand in the way. A mark over the far slope of a swell is hidden
by the near one, and that occlusion is the only thing saying what stands over it
lies on a surface rather than swimming in a fog. Read back as characters the
plane would have to take a shade of its own, and the piece would be a lit relief
with some dust on it — a different picture.

So it hands the exporter a frame directly, and what it draws into is a raster
with a depth buffer: `1/z` interpolated across a triangle, because that is the
part that stays straight in screen space; a near plane measured in the world
rather than on the picture, so the same scene survives being asked for at another
size; and discs rather than squares, at the sizes the particles run to. Nothing
about the export changes for any of it. The tool reports a period like every
other, the loop is closed the same way, and the same MP4 comes out the far end.

Every length in it is a fraction of the frame's height, which is what lets one
set of numbers compose both a preview and a poster: the eye stands at the
distance where the lens takes in exactly one frame from top to bottom, so a
figure half a frame tall fills half the picture whatever size the picture was
asked for.

Like the pieces, it is written from the idea rather than from the
[sketch of the same surface](https://github.com/Bleuje/processing-animations-code)
it follows, which reserves its rights.

## Moving a surface

`ascii` and `scene` take `--motion` on top of the turn — `ripple` for rings
travelling out from the middle, `breathe` for the whole body at once, `drift` for
noise walked round the same circle — with `--amount` for how hard.

The two apply it differently, because their subjects are different. A drawing is
a heightfield, so the movement scales heights rather than adding to them: a cell
the drawing left blank stays blank, where a movement that added would fill the
paper in and leave a rippling slab. A solid is displaced along each corner's own
outward direction, which every sample already knows — away from the middle for a
sphere, away from the ring for a torus, away from the curve for the knot — so one
line moves all four and no shape has to be told about movement at all.

Every motion is a whole number of cycles over the period by construction, so the
loop closes exactly rather than nearly. A body that moves is framed for the
largest it ever gets rather than for the frame in hand, or it would breathe in
and out of shot as the movement travelled over it.

## Running it

```sh
bun install
bun run tauri dev
```

The toolbar picks the tool; the panel beside the preview is that tool's own
options and nothing else, with what the export writes at the foot of it. Three of
the six are in it: the lift, the flat read of the same file, and the spiral that
lays the same file on its wave. The other three bring their own subject and are
not, because a formula turning on its own is a poor thing to open an app on —
nothing on screen was asked for, and the tool somebody came here to use was three
tabs along. They are still in the registry, and the command line asks for them by
name.

The spiral is the one that will also stand on nothing, which is what it has in
common with them, and it is in the window anyway. It is composed by eye — the
angle it is seen from and how thick the crowd is are the whole of the piece — and
those are questions a command line answers one rendered file at a time. The app
still opens on the lift, so it is a tab away rather than in the way.

The file is opened once and all three read it, because it is the same file: open
a drawing, see what the glyphs make of it flat, lift it, then let the drift carry
it out. Nothing is opened twice to be looked at twice.

The window opens on the lift, on a drawing that ships with it, so there is
something to turn before anything has been opened.

Drag the preview to turn a solid, scroll to zoom, double-click to face it again.
Each tool keeps its own angle, so turning the torus and then coming back to the
drawing finds it where it was left.

The spiral says those three numbers out loud as well, under View, and the two
handles move together — drag the disc and the sliders follow it. A view found by
hand is one worth keeping, and only a number can be written down and asked for
again a fortnight later.

Three schemes sit in the toolbar, and the swatch beside them recolours the object
itself to anything — the scheme keeps the paper and the window's own text, so a
green object on black cannot take the controls with it. Where the frames are
characters nothing re-renders for a colour: it costs a CSS property until an
export reads it. The spiral is the exception, and has to be drawn again — no
property on the page reaches inside a picture.

The preview does not chase the spin a frame at a time. One loop is rendered in
full, then played from memory at the rate that covers it exactly once, so what
the window shows is the animation an export writes rather than an approximation
of it that stutters when the machine is busy. A single frame is drawn first so a
tool that takes a while over a whole loop still answers immediately. A loop of
pictures is rendered smaller and from fewer frames than a loop of text, because
a frame of it crosses to the window as a hundred kilobytes of PNG rather than as
a few of text. Smaller has a floor, though: about the size the pane shows it at
and no less, since under that the frame is stretched to fill the pane and the
marks a line is drawn from fall under a pixel and read as a wash.

`ffmpeg` has to be installed for GIF and MP4; PNG and TXT need nothing.

```sh
brew install ffmpeg
```

It is looked for on the PATH first, then in the usual install prefixes — a
bundled app inherits almost no PATH from Finder, so a copy under
`/opt/homebrew/bin` is found without one. `ASCIIARY_FFMPEG` overrides both.

## The command line

The same pipeline, driven by a typed line. The whole line is one argument, quoted
— the `>` belongs to the command language rather than to the shell:

```sh
cargo run --bin asciiary -- "ascii logo.txt --depth 12 --turns 3 > out.mp4"
```

A line is a source, the filters it flows through, and where the result lands. The
extension after `>` picks the format. The language carries the `|` and the
registry behind it is wired up, but nothing is in it yet, so every line is a
source and an output for now.

Every tool takes these, whatever it draws:

| Flag | | Default |
| --- | --- | --- |
| `--duration` `--fps` | how long the file runs, and how smoothly | `4` `20` |
| `--columns` `--rows` | the grid, which is what sizes a picture as well | `160` `48` |
| `--scale` | pixels per point | `2` |
| `--ink` `--paper` | the object and what is behind it, as hex | `#e7e7e7` `#0c0c0e` |
| `--samples` | renders averaged into one written frame | `1` |
| `--shutter` | how much of the gap to the next frame they cover | `1` |

`--samples` is what makes a fast thing look fast. One render a frame catches an
instant, and several of them averaged across the gap to the next frame smear it
the way a camera would — `--samples 5 --shutter 1.2` is the setting the sketches
this borrows from record at. Only a written file pays for it: the preview is
always a single sample, which is the same split those sketches make between
watching a piece and recording it.

And its own on top:

| Tool | Flag | | Default |
| --- | --- | --- | --- |
| `loops` | `--piece` | which of the six above | `hilbert` |
| | `--order` `--depth` | how far `hilbert`, and `sierpinski` or `sliding`, recurse | `4` |
| | `--count` | discs in `sinusoids`, elements in `spherewave` | `28` `700` |
| | `--twists` | turns the short way in `toruscurve` | `3` |
| | `--seed` | which arrangement | `7` |
| `ascii` | *(first word)* | the drawing or picture to lift, or `--text` for one inline | |
| | `--depth` | how far the heaviest glyph stands out, in cell widths | `8` |
| | `--relief` | cells across a picture is read at; a drawing keeps its own | `120` |
| `scene` | `--shape` | `sphere`, `torus`, `cube` or `knot` | `torus` |
| | `--steps` | segments around the form | `64` |
| | `--thickness` | the tube's reach, against the body's | `0.42` |
| `media` | *(first word)* | the drawing, picture or animation to read, or `--text` for one inline | |
| | `--fit` | `contain` to letterbox it, `cover` to crop it | `contain` |
| `gen2d` | `--style` | `flow` for strokes, `noise` for tone | `flow` |
| | `--lines` `--steps` | how many strokes, and how far each is traced | `640` `120` |
| | `--grain` `--swirl` | how fine the field is, and how hard it turns | `1.3` `1.0` |
| | `--seed` | which field | `7` |
| `spiral` | *(first word)* | the drawing or picture to lay on the disc, or `--text` for one inline | |
| | `--spread` | how much of the disc it covers; past a whole crops it | `1` |
| | `--fit` | `contain` to stand it inside the disc, `cover` to fill it | `contain` |
| | `--open` | how far it is opened out to its own darkest and brightest | `1` |
| | `--floor` | how faint the light may get before it counts as paper | `0.04` |
| | `--windings` | turns of the line that draws it, middle to rim | `110` |
| | `--spin` | turns the line makes over a loop; `0` holds it still | `1` |
| | `--rings` | times the wave repeats between the middle and the rim | `8` |
| | `--arms` | arms it winds them into; `0` is rings, below it winds back | `1` |
| | `--bare` | the drift on its own, with nothing laid under it | off |
| | `--count` | particles in the drift | `17000` |
| | `--mesh` | quads a side the plane is cut into | `130` |
| | `--seed` | which arrangement of the drift | `7` |

Anything with a camera takes `--yaw` `--pitch` `--zoom`, and anything that moves
takes `--still` to hold it. A tool that turns over its loop takes `--turns` on
top, whole turns over one of them — the spiral has none, because what travels in
it is the wave rather than the eye. Its `--spin` is not that flag under another
name: the eye stays where it was put and the line goes round beneath it, which
is a different piece from the same view being carried around a still one.
Anything lit takes `--grade` — `shades`, `detailed` or `ink`, defaulting to
`detailed`. Anything that loops takes `--period`: how many seconds one loop
lasts, which is the whole clip unless less is asked for.

`ascii` and `scene` also take the movement:

| Flag | | Default |
| --- | --- | --- |
| `--motion` | `none`, `ripple`, `breathe` or `drift` | `none` |
| `--amount` | how hard, from nothing to all of it | `0.35` |

`loops`, `media` and `gen2d` all take the reading:

| Flag | | Default |
| --- | --- | --- |
| `--marks` | `match`, `shades`, `detailed` or `ink` | `match` |
| `--contrast` | opened about the middle rather than the floor | `1` |
| `--color` | keep the source's own colour, a cell at a time | off |
| `--invert` | swap which half of the picture is background | off |

`ascii` takes `--contrast` and `--invert` as well, and they mean the same thing
there: which end of the source is the subject and how hard the rest are pushed
apart. The lift reads the answer as a height rather than as a shade, which is the
only difference. The spiral takes those two and `--color` on top, where the same
answer decides which particles are drawn at all and what they are drawn in.

An animated export is rounded to a whole number of loops so it ends where it
began, which is what lets a GIF loop without a seam. The window shows the count
it landed on.

## Layout

```text
src/
  tools.ts           what each tool takes; the panel is built from it
  main.ts            the window
src-tauri/src/
  lib.rs             the Tauri commands the window calls
  repl/              the command language behind the typed line
  art/
    generators/      the six tools
      loops/         a file to a piece, and the two paths they take out of here
    filters/         post-processing — the seam is cut, nothing fills it yet
    motion.rs        phase instead of a clock: easing, and noise round a circle
    surface.rs       a surface cut from two parameters, and a tube round a curve
    raster.rs        triangles and dots to pixels, with a depth buffer
    read.rs          a picture read back as glyphs, shared by two tools
    canvas.rs        the character grid, and the ink ramp heights are read from
    glyphs.rs        which character a shaded cell comes out as
    paint.rs         grid to pixels
    export.rs        ffmpeg, and the numbers behind each argument
```

Adding a tool is one line in `generator::registry` and an entry in `TOOLS`; a
filter is one line in `filter::registry`.

The font is [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono), under
the SIL Open Font License — see `src-tauri/assets/OFL.txt`.
