# Roidery

Roidery is an embroidery digitizing app: draw or import artwork, give each object a stitch type,
watch it sew, and export a machine file. It is a fork of
[VectorCraft](https://github.com/storytold/vectorcraft) (MIT OR Apache-2.0), so the drawing tools,
interface and keyboard shortcuts are VectorCraft's, and embroidery is added on top.

## What Roidery adds

| Crate | Path | What it does |
| --- | --- | --- |
| `roidery-formats` | `crates/emb-formats` | Machine files: read and write Tajima DST and TAP, Brother PES (PES v1 with its PEC block and thumbnails), Janome JEF, Melco EXP, Husqvarna Viking VP3, Singer XXX and Barudan U01; read Barudan DSB, ZSK DSZ and Husqvarna Viking HUS; the hoop library (sewing fields by machine brand) and the fit check; thread charts (Brother and Janome built in, GIMP palette and CSV import) with colour matching in Lab. |
| `roidery-stitch` | `crates/emb-stitch` | Stitch generators: single and triple run, satin along a path (a Satin Column when its stroke has a width profile), Satin Blocks (two rails with rungs, steerable like Ember's tool), tatami fill with stagger and underlay, gradient fill (tatami fading from dense to open), contour fill (rows following the outline inwards, built on VectorCraft's Offset Path), circular fill (rings round a centre you can move), motif fill (rows of zigzags, waves, diamonds or scales in single or triple run), E-stitch and rope, manual stitches; lettering in Roidery Sans, an original centre-line satin font drawn for Roidery; fabric presets (density, pull compensation and underlay per fabric). |
| `roidery-plan` | `crates/emb-plan` | The stitch plan: sewing order, tie-ins and tie-offs, jumps and trims, colour changes, placing the design in the hoop (centred, or kept where it sits on the page); reviewing a design (sew time, thread density map). |
| `roidery-mockup` | `crates/emb-mockup` | Mockups: fabric textures (woven, knit, piqué, terry, fleece, denim, twill, leather), soft light, and realistic thread drawn on the CPU (the Stitch View's thread shader ported, with shadows), so a design can be shown sewn on a garment. |
| `roidery-fonts` | `crates/emb-fonts` | Digitized embroidery fonts in Ink/Stitch's font format (a folder with `font.json` and an SVG with one layer per letter): reads each letter's pieces with their stitch parameters (satin columns become Satin Blocks with paired rails, fills tatami, strokes running stitches), lays text out with the font's kerning and ligatures, and finds the font library (`library`). The format is read from its published files; no Ink/Stitch code is used. |

In VectorCraft's own crates the changes are kept small so upstream merges stay easy:

- `crates/doc`: `ObjectAttributes::stitch` holds an object's stitch settings as JSON, and
  `Document::embroidery` the design's fabric and thread chart.
- `crates/engine/src/cmd/embroidery.rs`: the commands below, registered in `cmd/mod.rs`, and
  `stitch_preview`, the stitches the canvas draws.
- `crates/tools/src/satinblocks.rs`: the Satin Blocks tool (1), in the toolbar's Embroidery group.
  Click and drag pulls curve handles out of a point, as the Pen tool does (Alt: only the outgoing
  handle), and the stitches fan round the curved rails (`satin::blocks_curved`, which walks both
  rails together into rungs 0.4 mm apart, paired by the fraction of each rail's length). With one
  Satin Blocks object selected it edits it: drag a rung's end (its handles move with it), click a
  rail to add a rung (a curved rail keeps its shape), Alt-click a rung to delete it; rung 0 is
  marked Start; Direct Selection edits the handles. Engine side:
  `crates/engine/src/cmd/embroidery/satin_edit.rs`.
- `crates/emb-stitch/src/medial.rs`: Convert to Satin (`stitch.convertToSatin`, Object ›
  Embroidery, engine side `crates/engine/src/cmd/embroidery/convert.rs`). The shape's exact
  medial axis (a Voronoi diagram of the outline, from the `centerline` crate) is drawn into a grid
  one cell wide, or, for an outline that crosses itself, the shape is drawn into the grid and
  thinned (Zhang–Suen) to its skeleton; corner spurs are pruned; the path that
  goes farthest and straightest is the spine (a loop for a ring), smoothed and carried on to the
  outline at its ends; rungs are cast square to it, fanned where a bend is tighter than the column
  is wide, sent to the outline's corners where a rail would cut one off (open ends close in on
  their tips), and thinned to the ones a straight block doesn't already give. Arms the spine leaves (a
  Y, a star) become more columns. The shape stays, hidden, under the new Satin Blocks.
  Convert to Satin (Classic) (`stitch.convertToSatinClassic`, `medial::Method::Classic`) keeps the
  first way next to it so the two can be compared by hand: always thinning, and no extra rungs to
  corners and tips (arms aren't bisected into the column they leave either).
- `crates/tools/src/fillangle.rs`: the Fill Angle tool (2), next to Satin Blocks. Dragging across
  the selection previews `stitch.setAngle` live and commits on release; Shift snaps to 15°,
  Escape puts the angle back.
- `crates/ui-egui/src/menus.rs`: Object › Embroidery (Stitch Settings, Convert to Satin (Centerline), Convert to Satin (Classic), Swap Satin Sides, Reverse
  Sewing Direction, Remove Overlaps, Sort by Color, Match to Thread Chart, Add Lettering). Remove
  Overlaps (`stitch.removeOverlaps`, `crates/engine/src/cmd/embroidery/overlaps.rs`) cuts fills
  where later fills cover them, keeping 1 mm under the edges. File › Export Machine File…,
  Export Worksheet… and Export Mockup… sit under Export Selection….
- `crates/ui-egui/src/embroidery_bar.rs`: the embroidery bar, across the top of the Embroidery
  workspace in place of the Control bar (Window › Embroidery Bar). Its left side follows the
  selection: the design with nothing selected (Add Lettering, Place Artwork, fabric, Sort by Color,
  Match to Thread Chart, Center in Hoop), a fill (fill type, Convert to Satin with Classic under
  its chevron, angle and the Fill Angle tool, density, underlay, colour, border, Remove Overlaps),
  a line (stitch, width or length, Reverse), Satin Blocks (density, pull compensation, underlay
  passes, Edit Rungs, Swap Sides, a warning over 10 mm), lettering (text, font, height amber
  outside the font's range, spacing, Put on Path with a path selected), computer-font text (font,
  size, Use an Embroidery Font…), a placed image (Image Trace: `stitch.traceImage` traces it with
  VectorCraft's Image Trace, white left out, and sews the paths as tatami; a click traces in 3
  Color, the arrow offers 2 Color, 6 Color and Detailed, which are VectorCraft's Default, 3 Colors,
  6 Colors and Flat Logo presets), groups and several objects (stitch type for all, Use as Fill
  Guide, Unite, Group, align, Sew Earlier / Later). Its right side is always there: Vector,
  Stitch and Realistic views; snapping (one button, a chevron for what it snaps to), grid,
  guides, rulers, outline; the stitch overlays; the hoop and how the design fits it (a popover
  for machine, hoop, fabric and thread chart); stitches and sew time; Sew-Out Player; Export.
  `Context` decides the left side; `stitch.centerInHoop` moves the design to the artboard's middle.
- View › Stitch Overlays (`stitchview.rs`): Jumps and Trims (`view.jumps`, dashed moves and a cut
  mark where the thread is trimmed), Needle Points (`view.needlePoints`), Underlay
  (`view.underlay`, off shows just the top stitches; `roidery_plan::plan_kinds` tags each record
  with its stitch kind and `PreviewStitch` carries `underlay` and `trim`), Current Color Only
  (`view.soloColor`, the other threads faded). The hoop is always drawn when the design has one.
- The page is the hoop (`embroidery/design.rs` `HoopField`): the first artboard is the hoop's
  sewing field and its centre is where the needle starts, so `stitch.export` keeps the design
  where it sits on the page (`PlanSettings::origin`; PES and JEF headers follow the origin).
  Picking a hoop (Stitch Settings, the Embroidery bar, New Document) sizes the page about its
  centre; resizing the page by hand makes it a custom hoop, and a page turned on its side keeps
  its hoop. `stitchview::hoop` draws the frame round the page (round hoops shade the corners),
  and stitches past the edge in red; `stitch.info` reports fit `outside` with `outsideMm`.
  Documents with no hoop are centred on the needle as before.
- Check Design (`embroidery/check.rs`, `stitch.check`; the panel `panels/check.rs`, Object ›
  Embroidery › Check Design and Window › Check Design): what will sew badly, errors first, each
  with where it is and, when one command fixes it, that command: the design past the hoop's edge
  (Center in Hoop) or too large for it; spots where objects stack past 2.6 layers of thread
  (an error past 4; Remove Overlaps when fills overlap; one object's own dense spots, such as a
  satin's corners, are left alone); fills under 1.5 mm wide on average (Convert to Satin);
  satins over 10 mm (loose, snag; Split Satin) or under 1 mm (Sew as Running Stitch); objects
  under 1.5 mm.
  Clicking a problem selects its objects and brings it into view; while the panel is open the
  canvas rings each problem.
- `crates/ui-egui/src/stitchview.rs`: View › Stitch View (Cmd+Y) cycles the vector art, the stitches
  as lines, and realistic thread; Outline moved off Cmd+Y to make room. View › Sew-Out Player
  (`view.sewOut`) sews the design stitch by stitch with play, step, scrub and speed.
- `crates/ui-egui/src/thread_gpu.rs` (feature `wgpu`, on in both apps): the realistic view on the
  GPU, each stitch an instanced, shaded, twisted thread over its shadow; `install` is called
  with the wgpu render state in `apps/vectorcraft/src/main.rs` and the web app. OpenGL builds
  keep the painter fallback in `stitchview.rs`.
- Thread lighting (`roidery-mockup`'s `shade` module, and the same sums in WGSL in
  `thread_gpu.rs`; keep them in step). The thread colour becomes a surface reflecting 4 % to 94 %
  of the light, so light and shade show on pure black and white. Two plies twist round each
  thread as a helix, each lit as its own small cylinder with a darker groove between them, and
  fine fibre streaks up close. The sheen runs along the fibres (Kajiya–Kay, as hair is shaded):
  a white lobe that shows on black and a softer one tinted by the thread. Grooves, the thread's
  sides and its ends (where it dives into the fabric) are darker, which is what gives white its
  shape, and a soft shoulder replaces clipping so white keeps a highlight. Each detail fades as
  it gets smaller than a pixel, and far out the thread is its average colour, so a colour looks
  the same at every zoom. The instance colour's alpha byte carries the thread's gloss (128 = 1,
  polyester and rayon), ready for a per-thread-type setting.
- View › Density Map (`view.densityMap`, `ViewFlags::density`): a heat map of thread layers
  over the stitch views, from `StitchPreview::density` (engine; `stitch.density` gives the same
  map to scripts). The maths is in `roidery-plan`'s `review` module, with the sew-time estimate
  `stitch.info` reports and Stitch Settings shows.
- `crates/ui-egui/src/panels/stitch_settings.rs`: Window › Stitch Settings, the selection's stitch
  type, settings and underlay (or the design's totals with nothing selected), first in the dock's
  icon column, and the Embroidery section at the top of the Properties panel. Both are the
  Embroidery workspace's: the others show the icon last and keep VectorCraft's Properties
  (`embroidery_workspace::icon_groups`). Basic and Advanced tabs (`stitch.settingsMode`,
  remembered in `UiState::stitch_advanced`).
- `crates/ui-egui/src/embroidery_workspace.rs`: the Embroidery workspace, Roidery's first-run
  default: its Basic toolbar, millimetres as the General unit, and what it trims (hidden, never
  removed: commands still run by id and every other workspace shows them all):
  - menus: 3D, graphs, perspective grid, symbols, slices, print tiling; Effect › Blur, Color
    Adjustments, Stylize's shadows, glows and feather, SVG Filters, plug-in effects, Document
    Raster Effects Settings; Blend, gradient mesh, Text Wrap, Threaded Text; transparency
    flattening; Export for Screens and Collect for Export; brush, graphic style and gradient
    swatch libraries; Select › Same appearance, blending mode, opacity and graphic style.
  - panels (dock, Window and search): Color Guide, Color Themes, Gradient, Transparency,
    Appearance, Graphic Styles, Brushes, Symbols, Paragraph, Character and Paragraph Styles,
    OpenType, Glyphs, Tabs, Asset Export, CSS Properties, Separations Preview, Flattener Preview.
    Layers, Libraries and Pattern Options leave the dock only (Window still opens them; the
    Sequencer lists the layers).
  - tools (Advanced toolbar and search): Flare, Mesh, Gradient, Blend, the Symbolism tools,
    graphs, perspective, slices, print tiling; the toolbar's Gradient paint button; the Contextual Task Bar (the floating bar under a selection) and its Window toggle; Opacity
    and the Appearance panel button in Properties; the fx menu's trimmed effects.

  Hooks: `UiState::embroidery`, `Workspace::embroidery`, `toolbar::slots`, the dock's tabs and
  icon column, `menus::visible_tree` (also the macOS menu), `palette::show`,
  `appearance::fx_menu`, Properties' appearance section, and `first_run` in the desktop and web
  apps.
- `crates/engine/src/cmd/newdoc.rs`: an Embroidery category of hoop presets, first in New
  Document, which opens on it, followed by every hoop in the library by brand; each preset sets
  the document's hoop (`DocSettings::hoop`). Home's quick-start cards are hoops.
- `crates/ui-egui/src/panels/sequencer.rs`: the Sequencer dock tab (`DockTab::Sequencer`, the
  Embroidery workspace's open tab), first sewn at the top. Objects lays the design out like
  Layers: layers (when there are several) and groups that fold, then each object with a
  thumbnail of its stitches in thread colour, its name (double-click to rename), stitch type and
  count, eye and lock; an object with a fill and a stroke has a step for each, dragged to swap
  which sews first (or ≡ › Sew Fill First / Sew Stroke First); a divider with the thread swatch
  marks each colour change; hidden objects stay listed. Colors has a row per colour block. Click
  to select, drag or Sew Earlier / Sew Later to reorder, Sort by Color. Engine side:
  `crates/engine/src/cmd/embroidery/sequence.rs`.
- `crates/ui-egui/src/panels/threads.rs`: Window › Threads (in the dock's icon column under Stitch
  Settings): the thread chart and fabric, the design's thread colours against the chart, Match to
  Chart and Import Chart… (`stitch.importChart`). Stitch Settings shows the fabric with nothing
  selected. Engine side: `crates/engine/src/cmd/embroidery/design.rs`.
- Lettering: Type › Add Lettering… (`type.addLettering`, `crates/ui-egui/src/dialogs/lettering.rs`)
  has two tabs that say what each kind of font is. **Embroidery Fonts** lists Roidery Sans and the
  digitized fonts in the font library, each with its sample, the stitches it uses and the sizes it
  sews well at (`stitch.fonts`), and adds a lettering object (`stitch.createLettering`).
  **Computer Fonts** adds a text object in any installed font, whose letters sew as fills. The
  Stitch Settings panel edits a lettering object's text, font (Change… reopens the window),
  letter height (with the font's size range, amber outside it), letter and line spacing
  (`stitch.setLettering`). Lettering in Roidery Sans is one path of satin columns; in a digitized
  font it is a group with a group per letter holding the font's pieces, each sewn as the font
  says, which can be selected inside the group and changed. Engine side:
  `crates/engine/src/cmd/embroidery/lettering.rs`.
- The embroidery font library is not in the repository. `cargo run --release -p roidery-fonts
  --example package_library -- <inkstitch-fonts>/src <out>` packages
  [inkstitch-fonts](https://github.com/inkstitch/inkstitch-fonts): the fonts whose licence lets
  Roidery ship them to everyone (OFL, CC-BY, CC-BY-SA, public domain; not non-commercial,
  no-derivatives, GPL or unclear), that need no contextual shaping and whose stitches Roidery has,
  each with its `LICENSE`, its compressed glyphs, `index.json` and `FONTS.md` (the credits; each
  font keeps its own licence). Roidery looks for it in `ROIDERY_EMBROIDERY_FONTS`, then in
  `embroidery-fonts` beside the program (where the installers put it), then `roidery-fonts`
  beside the checkout, then the user's own `embroidery-fonts` folder in Roidery's settings
  folder, where any Ink/Stitch font can be added (Look Again in the window finds it).
- `crates/engine/src/cmd/fileio/mod.rs` and `load.rs`: DST, PES, JEF, EXP, VP3, XXX, U01, TAP,
  DSB, DSZ and HUS rows in
  `FORMATS` (read only), `OPEN_EXTS` and `PLACE_EXTS`, and an arm in `load_with`, so File › Open
  opens machine files for editing: each run of stitches a Manual path in its thread's colour,
  grouped by colour block. Engine side: `crates/engine/src/cmd/embroidery/machine.rs`.
- `crates/ui-egui/src/machine_export.rs`: File › Export Machine File… (`file.exportMachine`), which
  saves DST, PES, JEF, EXP, VP3, XXX, U01 or TAP and remembers the format used last.
- `crates/ui-egui/src/worksheet_export.rs`: File › Export Worksheet… (`file.exportWorksheet`),
  which writes `stitch.worksheet`'s production sheet as PDF, PNG or SVG in the interface language.
- `crates/ui-egui/src/mockup_export.rs` and `dialogs/export_mockup.rs`: File › Export Mockup…
  (`file.exportMockup`), a dialog with a preview (garment, placement, colour, fabric) that writes
  `stitch.mockup`'s picture as PNG or JPEG. Engine side: `crates/engine/src/cmd/embroidery/mockup.rs`.
- `crates/ui-egui/src/i18n/*.tsv`: a `# Roidery` section at the end of each catalog.
- `crates/doc/src/hit.rs`: `HitOptions::inside` (Preferences › Selection & Anchor Display ›
  Select Inside Unfilled Shapes, on by default): a click inside an unfilled closed shape, or
  between a Satin Blocks object's rails, picks it once no edge or fill is hit, and open paths take
  a click twice the tolerance away. Embroidery outlines are often stroke only.
- `crates/pathops/src/offset.rs`: a bug fix, which Roidery should offer upstream. Offset Path
  dropped an inset of a shape with a hole as "spurious" when the deep point it tested landed in
  the hole; the test now uses each contour's face with its holes. Test:
  `offset_inset_keeps_a_shape_with_a_small_hole` in `crates/pathops/tests/ops.rs`.
- `xtask/src/layers.rs`: the Roidery crates' layers (`roidery-stitch` uses pathops, so it sits at L3;
  `roidery-mockup` stands alone).

- `crates/ui-egui/examples/icon_sheet.rs`: the editable icon sheet, a developer tool.
  `cargo run -p vectorcraft-ui-egui --example icon_sheet -- make roidery-icons.svg` puts every
  interface icon on its own labelled artboard; `-- apply <edited sheet> [--dry-run]` writes the
  changed icons back to `assets/icons` (labels from `examples/icon_sheet/labels.tsv`).

## Commands

Every visible path, compound path and text object is an embroidery object, sewn bottom to top.
Text (the Type tools, any installed font) sews its glyph outlines as one fill, with overlapping
contours merged, and stays editable; Remove Overlaps cuts what text covers but not the text.
Each object starts near where the needle ended the one before (`roidery_stitch::route`): run,
triple and satin lines sew their parts nearest first from the nearer end, closed lines and
E-stitch or rope edges start at their nearest point, and tatami starts from the nearer side.
Lettering, Satin Blocks, manual stitches and width-profiled columns keep their own start.

Each object has two parts (`crates/engine/src/cmd/embroidery/parts.rs`): its fill sews first,
with a fill stitch (tatami, contour, circular, gradient, motif, guided) in the fill colour, then
its stroke, with a line stitch (run, triple, satin, E-stitch, rope) in the stroke colour, as its
border. Without settings of its own, a fill is tatami and a stroke a running stitch. Satin
Blocks, manual stitches and lettering are the whole object (no fill part). The stroke weight is
the sewn width of a satin, E-stitch or rope stroke, both ways: the Stroke panel, the Width tool
and Stitch Settings change one number. A running stitch set in Stitch Settings shows at thread
thickness (0.4 mm). `ObjectAttributes::stitch` holds `{fill?, stroke?}`, or a single setting
(older documents, and one part with the other off). A Satin Blocks object is a path with two
subpaths, the left and right rails, where anchor `i` of each forms rung `i`; the rails may curve
between rungs.

- `stitch.set {part?, type?, settings?, reset?, ids?}`: set a part's stitch type and settings (mm
  and degrees). Without `part`, a type sews the object as that one type (the other part stops)
  and settings alone go to every part that sews.
- `stitch.info {ids?}`: each object's settings and stitch count, and the design's totals.
- `stitch.sequence {thumbs?}`: every object in sewing order (one row per fill or stroke part,
  hidden objects included, with visibility, lock, enclosing layers and groups, and with `thumbs`
  a small polyline of its stitches) and the colour blocks they make.
- `stitch.partOrder {ids?, first: "fill" | "stroke"}`: whether an object's fill or stroke sews
  first (`"strokeFirst"` in its stitch settings).
- `stitch.move {ids, before? | after?}`: sew objects just before or after another one.
- `stitch.sortColors {}`: group each layer's objects by thread colour.
- `stitch.export {path?, format?, name?, trimAt?, tieIn?, tieOff?, travelUnder?}`: write the machine file
  (`dst`, `pes`, `jef`, `exp`, `vp3`, `xxx`, `u01` or `tap`). With `travelUnder` (on unless false), a move that fills
  sewn later will cover is sewn as 2 mm running stitches under them instead of a jump and trim.
- `stitch.design {}`: the fabric, the thread chart, and each thread colour with the chart thread
  it is or is nearest to.
- `stitch.setDesign {fabric?, chart?, applyToObjects?}`: choose the fabric (`woven`, `knit`,
  `pique`, `terry`, `fleece`, `denim`, `twill`, `leather`) and the thread chart (`brother`,
  `janome`, `none`, or an imported one). Machine files then name threads that are in the chart.
- `stitch.matchThreads {ids?}`: change colours to their nearest chart thread.
- Satin along a path is a Satin Column: the stroke's width profile (the Width tool's) scales
  its width along the path (`roidery_stitch::Shape::Column`), whose width is the stroke weight.
- Every satin (along a path, Satin Blocks, lettering) sews short stitches and splits long ones
  (`satin.rs`, `short_stitches` and `split`). Where a tight curve crowds the inside edge's
  penetrations closer than `shortStitchDistance` (0.25 mm), every other one stops
  `shortStitchInset` (15%) of the way across, so the thread doesn't pile up there. A top stitch
  longer than `splitLength` (7 mm; 0 never) is split into shorter ones whose splits shift a third
  of a piece from one crossing to the next, a brick pattern rather than a groove. Both are in
  Stitch Settings (Short stitches, Split satin over; Short stitch distance under Advanced).
- `stitch.fonts {rescan?}`: the lettering fonts, built-in and digitized, with what each sews with
  and the sizes it sews well at.
- `stitch.createLettering {text, height?, spacing?, lineSpacing?, font?, x?, y?}`: text as satin
  columns (one path) in Roidery Sans, or a group of letters in a digitized font; its text stays
  editable.
- `stitch.mapLettering {ids?, path?, keep?}`: lettering follows an open path (each letter upright
  on it, turned with it); the path is stored with the lettering and removed unless `keep`.
- `stitch.setLettering {text?, height?, spacing?, lineSpacing?, font?, path?, ids?}`: change lettering
  and lay it out again where it is.
- `stitch.createSatinBlocks {left, right, settings?}`: a Satin Blocks object from its rails (the
  tool previews and commits through it); a rail point may be `{p, in?, out?}` with curve handles.
- `stitch.convertToSatin {ids?}`: closed shapes to Satin Blocks along their spines (above),
  → `{ids, skipped, widestMm, warning?}` (a column over 10 mm wide warns; the app shows it in the
  status bar). `stitch.convertToSatinClassic {ids?}` does the same the classic way (above).
- `stitch.moveRung {id, index, left?, right?}`, `stitch.insertRung {id, index, t?}`,
  `stitch.deleteRung {id, index}`: edit a Satin Blocks object's rungs (the tool drives them).
- `stitch.setGuide {ids?, guide?, keep?}`: make fills guided fills whose rows are parallel
  offsets of an open path (the one selected with them), stored with each fill as fractions of
  its bounds so it moves and scales with it; the path is removed unless `keep`.
- `stitch.setAngle {angle, ids?}`: turn fills' rows to `angle` degrees (a motif fill's motif
  angle); lines are left as they are. The Fill Angle tool drives it.
- `stitch.removeOverlaps {ids?, overlap?}`: cut fills where later fills cover them, keeping
  `overlap` mm (default 1) under the covering edges.
- `stitch.testSwatches {type?, densities?, pulls?}` (Object › Embroidery › Test Swatches ›
  Satin or Fill, `swatches.rs`): a grouped grid centred on the first artboard of 14 mm satin rings
  or tatami squares, rows at each density (labelled D0.3…), columns at each pull compensation
  (P0…), the labels in Roidery Sans sewn in triple run, to sew once on the machine and fabric at hand and pick the
  settings from. One undo step.
- `stitch.swapSides {ids?}`: swap the rails, so each stitch starts from the other side.
  Reversing the sewing direction is VectorCraft's `path.reverse`.
- `stitch.worksheet {path?, format?, paper?, ppi?, labels?}`: the one-page production sheet
  (`pdf`, `png` or `svg`; `a4` or `letter`); `labels` translates its words, keyed by their English.
- `stitch.mockup {garment?, placement?, color?, fabric?, ppm?, format?, path?}`: the design sewn
  on a `tshirt` (`center` or `leftChest`), `cap`, `tote` or `swatch`, in a `#rrggbb` garment
  colour and fabric texture, at `ppm` pixels per mm, as `png` or `jpg`.

Try it from the command line:

```sh
cargo run --release -p vectorcraft-cli -- run --in art.svg \
  --cmd select.all \
  --cmd stitch.set --params '{"type":"satin","settings":{"satin":{"width":3}}}' \
  --cmd stitch.export --params '{"path":"art.dst"}'
```

`cargo run -p roidery-plan --example sample_dst -- sample.dst` writes a sample design without the
app.

## The name

The app shows itself as Roidery (window title, app menu, About, Home, Help) and keeps its
preferences in its own folder (`Roidery` on macOS and Windows, `~/.config/roidery` on Linux), apart
from any VectorCraft on the same machine. Crates, binaries, the file format and the packaging ids
keep VectorCraft's names so upstream merges stay clean. The installers are Roidery's
(`packaging/`, see `docs/releasing.md`): `roidery-<version>-<os>-<arch>.<ext>`, the app id
`app.roidery.Roidery`, a Windows installer with its own upgrade code (it never touches a VectorCraft
install), and the binaries installed as `Roidery.exe` / `roidery` and `roidery-cli`.

The app icon is Roidery's own: an R sewn in satin in an embroidery hoop (`assets/roidery-icon/`,
see its README; regenerate with `packaging/roidery-icons.sh`). It is the window, Dock and taskbar
icon (`apps/vectorcraft/src/main.rs`), the `.exe`'s icon and version info
(`apps/vectorcraft/build.rs`), the mark in the app bar and About box
(`crates/ui-egui/src/brand.rs`), and the browser version's tab icon
(`apps/vectorcraft-web/index.html`, which also says Roidery while it loads). VectorCraft's icon stays
in `assets/app-icon/`, unused, so merges don't conflict. `cargo xtask icns` packs a macOS icon set
without `iconutil`.

## Software updates

Help › Check for Updates… (and the Roidery menu on macOS) reads the update feed, a small JSON file
a release publishes beside its installers (`cargo xtask update-feed` writes it; the format is in
`crates/emb-update`, `roidery-update`). When a newer version is out, Software Update shows what's
new and offers Download and Install, Skip This Version or Close. The download is the installer
for this computer, checked against the feed's SHA-256 before anything runs it; then the Windows
installer runs and Roidery quits, macOS opens the disk image, and an AppImage is replaced in place.
Roidery also checks once a day by itself (Preferences › General › Check for updates
automatically), quietly unless there is something new.

The feed's URL comes from `ROIDERY_UPDATE_URL` when Roidery is built (or when it runs). Builds
without one say updates aren't set up. GitHub Releases gives a stable URL for the newest release's
feed: `https://github.com/<owner>/<repo>/releases/latest/download/roidery-update.json`. The
browser version is always current and has no updates.

Code: `crates/emb-update` (the feed, versions, picking the download, SHA-256, the HTTPS fetch),
`crates/ui-egui/src/updates.rs` (the commands, the dialog, the daily check),
`apps/vectorcraft/src/update_install.rs` (handing the download to the system).

## Keeping up with VectorCraft

`upstream` is `https://github.com/storytold/vectorcraft`. Merge it into `roidery` regularly, then
run `cargo xtask ci`:

```sh
git remote add upstream https://github.com/storytold/vectorcraft.git   # once
git fetch upstream main
git merge upstream/main
```

VectorCraft's features that don't matter for embroidery stay in the code and are hidden by the
Embroidery workspace rather than deleted, so merges don't conflict. What a merge needs to know:

- The translation catalogs (`crates/ui-egui/src/i18n/*.tsv`) and `ASSETS.md` merge with git's
  `union` driver (`.gitattributes`): both sides' new lines are kept, so they rarely conflict. A
  string both sides translated appears twice; keep VectorCraft's line.
- Roidery's own files win where VectorCraft changed its copy of them: `README.md`,
  `docs/releasing.md` and the packaging (`packaging/`, `.github/workflows/packaging-lint.yml`),
  which are Roidery's names and installers. `packaging/windows/vectorcraft.wxs` stays deleted
  (Roidery's is `roidery.wxs`).
- `Cargo.lock`: take VectorCraft's and let the next build add Roidery's crates back.
- The rest are small hooks into VectorCraft's code, listed above. Where VectorCraft rewrote the
  code around one, it is put back into the new code; where VectorCraft now does the same thing
  itself (its scrubby zoom, its Direct3D 12 choice on Windows), Roidery's version is dropped.

The merge of 2026-10-14 brought in 1,874 commits of VectorCraft (from `e2a41733` to `b076025a`):
its own scrubby zoom (which replaced Roidery's), Image Trace's Flat Logo mode, floating panel
groups, Edit Toolbar, Affinity files and many fixes.

Roidery's history keeps VectorCraft's: build from a clone of the Roidery repository and `git pull`
to update, rather than applying patches, so merges from VectorCraft arrive as they are.

## Licences

Roidery's own code is MIT OR Apache-2.0 like VectorCraft. The embroidery algorithms follow the
published behaviour of stitchjs (MIT) and the format notes of libembroidery (zlib) and
pyembroidery (MIT), which are also the test oracles for the machine files (outside the repo). The
PES and JEF thread charts list the manufacturers' published colour numbers, names and RGB values;
the Janome chart's catalogue numbers are as pyembroidery lists them. No GPL code (Ink/Stitch and the like) is used.
The embroidery font library (packaged outside the repository, above) holds fonts from the
Ink/Stitch community under their own licences (SIL OFL 1.1, CC-BY 4.0, CC-BY-SA 4.0, public
domain), each with its `LICENSE` and credited in the library's `FONTS.md`. VectorCraft's ArtCraft
trademarks are removed, as its brand licence requires of forks.
