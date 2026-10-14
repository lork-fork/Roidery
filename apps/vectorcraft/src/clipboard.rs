//! The system clipboard with every format Copy offers and Paste reads
//! (`Services::system_clipboard`).
//!
//! - Windows: text, SVG, PDF, PNG and an opaque bitmap (for apps that don't read PNG) in one go;
//!   Paste reads each of them.
//! - macOS and Linux: one format at a time, the text (a type-only copy's text or the SVG markup),
//!   else the PNG as a bitmap; Paste reads text and bitmaps. On Wayland, arboard's data-control
//!   backend reads the compositor's clipboard (X11 through XWayland only sees what X11 apps
//!   copied), falling back to X11 where the compositor lacks the protocol.
//! - Everywhere: files copied in a file manager paste as the first one that is art (SVG, PDF, a
//!   metafile or a bitmap), before the clipboard's other formats (the files' paths as text).

use std::io::{Cursor, Read};
use std::path::PathBuf;

use vectorcraft_engine::cmd::clipboard::{BITMAP, FILE_HEAD, Flavour, PNG, TEXT, file_flavour};
use vectorcraft_ui_egui::SystemClipboard;

/// This platform's system clipboard.
pub fn system_clipboard() -> Box<dyn SystemClipboard> {
    #[cfg(windows)]
    let cb = Box::<win::Clipboard>::default();
    #[cfg(not(windows))]
    let cb = Box::<portable::Clipboard>::default();
    cb
}

/// The text flavour among `flavours`.
fn text_of(flavours: &[Flavour]) -> Option<String> {
    flavours.iter().find(|f| f.mime == TEXT).map(|f| String::from_utf8_lossy(&f.data).into_owned())
}

fn decode_png(png: &[u8]) -> Option<image::RgbaImage> {
    Some(image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.to_rgba8())
}

/// The first of `paths` (files copied in a file manager) that pastes as one of `mimes`, read
/// whole. Only its first bytes are read to tell, so other files cost little.
fn copied_file(paths: Vec<PathBuf>, mimes: &[&'static str]) -> Option<Flavour> {
    paths.into_iter().find_map(|path| {
        // text/uri-list lines end in CRLF (RFC 2483), but arboard splits them on LF only.
        let path = match path.to_str() {
            Some(p) if p.ends_with('\r') => PathBuf::from(p.trim_end_matches('\r')),
            _ => path,
        };
        let mut head = Vec::with_capacity(FILE_HEAD);
        std::fs::File::open(&path).ok()?.take(FILE_HEAD as u64).read_to_end(&mut head).ok()?;
        let mime = file_flavour(&path.to_string_lossy(), &head, mimes)?;
        let data = std::fs::read(&path).ok()?;
        (!data.is_empty()).then_some(Flavour { mime, data })
    })
}

#[cfg(windows)]
mod win {
    use std::num::NonZeroU32;

    use clipboard_win::{formats, options::NoClear, raw};
    use vectorcraft_engine::cmd::clipboard::{PDF, SVG};

    use super::*;

    /// The registered clipboard formats a flavour is written under (and read from, first held
    /// first).
    fn names(mime: &str) -> &'static [&'static str] {
        match mime {
            SVG => &["image/svg+xml"],
            PDF => &["Portable Document Format", "application/pdf"],
            PNG => &["PNG"],
            _ => &[],
        }
    }

    fn formats_of(mime: &str) -> impl Iterator<Item = u32> {
        names(mime).iter().filter_map(|n| raw::register_format(n)).map(NonZeroU32::get)
    }

    fn err(e: impl std::fmt::Display) -> String {
        format!("{e}")
    }

    /// `png` over white, as a BMP file (what the bitmap format takes).
    fn opaque_bmp(png: &[u8]) -> Option<Vec<u8>> {
        let img = decode_png(png)?;
        let over = |c: u8, a: u8| ((u32::from(c) * u32::from(a) + 255 * (255 - u32::from(a)) + 127) / 255) as u8;
        let mut rgb = Vec::with_capacity(img.as_raw().len() / 4 * 3);
        for p in img.pixels() {
            let [r, g, b, a] = p.0;
            rgb.extend([over(r, a), over(g, a), over(b, a)]);
        }
        let rgb = image::RgbImage::from_raw(img.width(), img.height(), rgb)?;
        let mut bmp = vec![];
        rgb.write_to(&mut Cursor::new(&mut bmp), image::ImageFormat::Bmp).ok()?;
        Some(bmp)
    }

    #[derive(Default)]
    pub struct Clipboard {
        /// The clipboard's sequence number right after our last write.
        seq: Option<NonZeroU32>,
        /// The text of our last write.
        text: Option<String>,
    }

    impl SystemClipboard for Clipboard {
        /// Writes every flavour it can; the first failure is reported.
        fn write(&mut self, flavours: &[Flavour]) -> Result<(), String> {
            let mut result = Ok(());
            {
                let _open = clipboard_win::Clipboard::new_attempts(10).map_err(err)?;
                raw::empty().map_err(err)?;
                let mut set = |r: clipboard_win::SysResult<()>| {
                    if result.is_ok() {
                        result = r.map_err(err);
                    }
                };
                for f in flavours {
                    if f.mime == TEXT {
                        set(raw::set_string_with(&String::from_utf8_lossy(&f.data), NoClear));
                        continue;
                    }
                    for id in formats_of(f.mime) {
                        set(raw::set_without_clear(id, &f.data));
                    }
                    if f.mime == PNG
                        && let Some(bmp) = opaque_bmp(&f.data)
                    {
                        set(raw::set_bitmap_with(&bmp, NoClear));
                    }
                }
            }
            self.seq = raw::seq_num();
            self.text = text_of(flavours);
            result
        }

        fn holds_ours(&mut self) -> bool {
            if self.seq.is_some() && raw::seq_num() == self.seq {
                return true;
            }
            // A clipboard manager may have copied our data again as its own.
            let Some(ours) = self.text.as_deref() else { return false };
            let Ok(_open) = clipboard_win::Clipboard::new_attempts(10) else { return false };
            read_one(TEXT).is_some_and(|f| f.data == ours.as_bytes())
        }

        fn read(&mut self, mimes: &[&'static str]) -> Option<Flavour> {
            let mut files = vec![];
            {
                let _open = clipboard_win::Clipboard::new_attempts(10).ok()?;
                // None copied when the clipboard holds no file list.
                let _ = raw::get_file_list_path(&mut files);
            }
            // The files are read with the clipboard closed again, not keeping other apps waiting.
            copied_file(files, mimes).or_else(|| {
                let _open = clipboard_win::Clipboard::new_attempts(10).ok()?;
                mimes.iter().find_map(|m| read_one(m))
            })
        }

        fn has(&mut self, mimes: &[&'static str]) -> bool {
            raw::is_format_avail(formats::CF_HDROP)
                || mimes.iter().any(|m| match *m {
                    TEXT => raw::is_format_avail(formats::CF_UNICODETEXT),
                    BITMAP => raw::is_format_avail(formats::CF_BITMAP) || formats_of(PNG).any(raw::is_format_avail),
                    m => formats_of(m).any(raw::is_format_avail),
                })
        }
    }

    /// Read the first registered format of `mime` the clipboard holds into `data`.
    fn read_registered(mime: &str, data: &mut Vec<u8>) -> bool {
        formats_of(mime).filter(|id| raw::is_format_avail(*id)).any(|id| {
            data.clear();
            raw::get_vec(id, data).is_ok() && !data.is_empty()
        })
    }

    /// One flavour, while the clipboard is open.
    fn read_one(mime: &'static str) -> Option<Flavour> {
        let mut data = vec![];
        let mime = match mime {
            BITMAP if read_registered(PNG, &mut data) => PNG,
            BITMAP => {
                data.clear();
                raw::get_bitmap(&mut data).ok()?;
                "image/bmp"
            }
            TEXT => {
                raw::get_string(&mut data).ok()?;
                TEXT
            }
            m if read_registered(m, &mut data) => m,
            _ => return None,
        };
        // Memory blocks can be longer than the data: SVG text ends at its first NUL.
        if mime == SVG
            && let Some(end) = data.iter().position(|b| *b == 0)
        {
            data.truncate(end);
        }
        (!data.is_empty()).then_some(Flavour { mime, data })
    }
}

/// arboard: one format at a time. Built everywhere so it is checked everywhere.
#[cfg_attr(windows, allow(dead_code))]
mod portable {
    use std::borrow::Cow;

    use super::*;

    /// `img` as PNG bytes.
    fn encode_png(img: &image::RgbaImage) -> Option<Vec<u8>> {
        let mut out = vec![];
        img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
        Some(out)
    }

    /// What our last write put on the clipboard.
    enum Ours {
        Text(String),
        Image(usize, usize),
    }

    #[derive(Default)]
    pub struct Clipboard {
        /// Kept open: on Linux the copied data lives as long as the handle does.
        cb: Option<arboard::Clipboard>,
        ours: Option<Ours>,
    }

    impl Clipboard {
        fn cb(&mut self) -> Result<&mut arboard::Clipboard, String> {
            if self.cb.is_none() {
                self.cb = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
            }
            self.cb.as_mut().ok_or_else(|| "no system clipboard".to_string())
        }
    }

    impl SystemClipboard for Clipboard {
        fn write(&mut self, flavours: &[Flavour]) -> Result<(), String> {
            let text = text_of(flavours);
            let image = match &text {
                Some(_) => None,
                None => flavours.iter().find(|f| f.mime == PNG).and_then(|f| decode_png(&f.data)),
            };
            let cb = self.cb()?;
            let ours = match (text, image) {
                (Some(t), _) => {
                    cb.set_text(t.as_str()).map_err(|e| e.to_string())?;
                    Some(Ours::Text(t))
                }
                (None, Some(img)) => {
                    let (width, height) = (img.width() as usize, img.height() as usize);
                    cb.set_image(arboard::ImageData { width, height, bytes: Cow::Owned(img.into_raw()) }).map_err(|e| e.to_string())?;
                    Some(Ours::Image(width, height))
                }
                (None, None) => {
                    cb.clear().map_err(|e| e.to_string())?;
                    None
                }
            };
            self.ours = ours;
            Ok(())
        }

        fn holds_ours(&mut self) -> bool {
            let Some(cb) = self.cb.as_mut() else { return false };
            match &self.ours {
                Some(Ours::Text(t)) => cb.get_text().is_ok_and(|now| now == *t),
                Some(Ours::Image(w, h)) => cb.get_image().is_ok_and(|now| (now.width, now.height) == (*w, *h)),
                None => false,
            }
        }

        fn read(&mut self, mimes: &[&'static str]) -> Option<Flavour> {
            let cb = self.cb().ok()?;
            if let Some(f) = cb.get().file_list().ok().and_then(|paths| copied_file(paths, mimes)) {
                return Some(f);
            }
            mimes.iter().find_map(|m| match *m {
                TEXT => cb.get_text().ok().filter(|t| !t.is_empty()).map(|t| Flavour { mime: TEXT, data: t.into_bytes() }),
                BITMAP => {
                    let img = cb.get_image().ok()?;
                    let rgba = image::RgbaImage::from_raw(u32::try_from(img.width).ok()?, u32::try_from(img.height).ok()?, img.bytes.into_owned())?;
                    encode_png(&rgba).map(|data| Flavour { mime: PNG, data })
                }
                _ => None,
            })
        }

        /// Text only: reading a bitmap costs as much as pasting it.
        fn has(&mut self, mimes: &[&'static str]) -> bool {
            mimes.contains(&TEXT) && self.cb().is_ok_and(|cb| cb.get_text().is_ok_and(|t| !t.is_empty()))
        }
    }
}

#[cfg(test)]
mod tests {
    use vectorcraft_engine::cmd::clipboard::{PASTE_ORDER, SVG};

    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vectorcraft-clipboard-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = vec![];
        image::RgbaImage::new(w, h).write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn the_first_copied_file_that_is_art_pastes() {
        let d = dir("art");
        let (notes, red, art) = (d.join("notes.txt"), d.join("red image.png"), d.join("art.svg"));
        std::fs::write(&notes, "not art").unwrap();
        std::fs::write(&red, png(3, 2)).unwrap();
        std::fs::write(&art, "<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();
        // Missing files, folders and text are skipped.
        let f = copied_file(vec![d.join("gone.png"), d.clone(), notes.clone(), red.clone(), art.clone()], &PASTE_ORDER).unwrap();
        assert_eq!((f.mime, f.data), (PNG, png(3, 2)));
        assert_eq!(copied_file(vec![art.clone(), red.clone()], &PASTE_ORDER).map(|f| f.mime), Some(SVG));
        // Only what Paste asks for.
        assert_eq!(copied_file(vec![art, red.clone()], &[BITMAP]).map(|f| f.mime), Some(PNG));
        assert!(copied_file(vec![red], &[TEXT]).is_none());
        assert!(copied_file(vec![notes], &PASTE_ORDER).is_none());
        assert!(copied_file(vec![], &PASTE_ORDER).is_none());
    }

    #[test]
    fn a_crlf_left_by_the_uri_list_is_ignored() {
        let red = dir("crlf").join("red.png");
        std::fs::write(&red, png(2, 2)).unwrap();
        let with_cr = PathBuf::from(format!("{}\r", red.display()));
        assert_eq!(copied_file(vec![with_cr], &PASTE_ORDER).map(|f| f.mime), Some(PNG));
    }
}
