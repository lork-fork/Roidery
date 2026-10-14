# Roidery app icon

**Picture:** an R sewn in satin, framed in a wooden embroidery hoop with its clasp at the top, on a
deep teal tile. The satin stitches run across each column of the R, as a machine sews them, and a
running stitch circles the cloth.

**Palette:**

| Colour | Hex | Used for |
|---|---|---|
| Teal | `#1d5c63` | the tile |
| Wood | `#c98b4e`, `#a96d36`, `#d9a466`, `#b7793f`, `#8f5a2b` | the hoop and its clasp |
| Cloth | `#fbf6ec` to `#e9dfcc` | the fabric in the hoop |
| Thread | `#c8343a`, with `#ef6a5e` sheen and `#8e1f26` stitch lines | the R and the running stitch |

**Tile:** `viewBox="0 0 512 512"`, a rounded square with `rx=112`, clipped, like VectorCraft's
icon, so the files drop into the same places. The macOS files add Apple's transparent margin (tile
= 824/1024 of the canvas); Windows and Linux use the tile edge to edge.

`roidery.svg` is the canonical artwork, written by hand. Licence: see `LICENSE.txt`.

## Files

| File | What | Used by |
|---|---|---|
| `roidery.svg` | master vector | source for everything below |
| `roidery-1024.png` | full tile, 1024 px | docs, store listings |
| `roidery-macos-512.png` | tile with Apple margin, 512 px | runtime Dock icon (`apps/vectorcraft/src/main.rs`) |
| `roidery.icns` | macOS icon set, 16 to 1024 px | the macOS app bundle |
| `roidery.ico` | Windows icon, 16 to 256 px | `apps/vectorcraft/build.rs` (embedded in the `.exe`) |
| `favicon.ico` | 16, 32 and 48 px | the browser version's tab (`apps/vectorcraft-web/index.html`) |
| `apple-touch-icon.png` | 180 px, square and opaque | the browser version saved to a phone's home screen |
| `hicolor/<size>/apps/app.roidery.Roidery.png` | Linux theme icons, 16 to 512 px; the 256 one is also the runtime icon on Windows and Linux, the 128 one the brand mark in the app bar and About box (`crates/ui-egui/src/brand.rs`) | Linux packages |
| `hicolor/scalable/apps/app.roidery.Roidery.svg` | Linux scalable icon | as above |

## Regenerate

Edit `roidery.svg`, then run `packaging/roidery-icons.sh` (needs `resvg`). It writes every PNG, the
`.ico` files (via `cargo xtask ico`) and the `.icns` (with `iconutil` on macOS, else `cargo xtask
icns`).
