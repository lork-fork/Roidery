//! Data the program reads: from itself (`currentfile`) through decoding filters, and images
//! sampled from it, from strings or from procedures.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{ImageBlob, ImageObject, Node, NodeId, NodeKind};
use vectorcraft_geom::Affine;

use super::graphics::{Space, process};
use super::interp::{Interp, matrix_of};
use super::lex::{find, hex_decode};
use super::obj::{DictRef, Key, Obj, Op, PsError, Res, Shared, ps_err};
use crate::ps;

/// Most bytes one filter or image reads.
const MAX_DATA: usize = 1 << 28;
/// Most pixels an image may have.
const MAX_PIXELS: usize = 1 << 26;
/// Most distinct colours an image converts through a tint transform.
const MAX_TINTS: usize = 1 << 16;

/// A file object: the program itself, or data a filter decoded.
#[derive(Debug)]
pub(crate) enum Stream {
    Current,
    /// Filters over the program's own data, decoded when first read.
    Pending(Vec<Filter>),
    Buf {
        data: Vec<u8>,
        pos: usize,
        /// The samples are 8-bit RGB (or grey) whatever the image's colour space: a decoded JPEG.
        rgb: Option<u8>,
    },
}

fn io<T>(at: &str) -> Res<T> {
    ps_err("ioerror", at)
}

/// The bytes `RunLengthDecode` makes of `raw`, and how many it read.
fn run_length(raw: &[u8]) -> Res<(Vec<u8>, usize)> {
    let mut out = vec![];
    let mut i = 0;
    while let Some(&n) = raw.get(i) {
        i += 1;
        match n {
            128 => break,
            0..=127 => {
                let len = usize::from(n) + 1;
                out.extend_from_slice(raw.get(i..i + len).ok_or(PsError::Ps("ioerror", "RunLengthDecode".into()))?);
                i += len;
            }
            _ => {
                let b = *raw.get(i).ok_or(PsError::Ps("ioerror", "RunLengthDecode".into()))?;
                i += 1;
                out.extend(std::iter::repeat_n(b, 257 - usize::from(n)));
            }
        }
        if out.len() > MAX_DATA {
            return Err(PsError::Limit("decoded data is too large"));
        }
    }
    Ok((out, i))
}

/// zlib data inflated, and how many bytes of `raw` it took.
fn flate(raw: &[u8]) -> Res<(Vec<u8>, usize)> {
    let mut z = flate2::Decompress::new(true);
    let mut out = Vec::with_capacity(raw.len().saturating_mul(3).min(MAX_DATA));
    loop {
        let before = (z.total_in(), z.total_out());
        if out.capacity() - out.len() < 64 * 1024 {
            out.reserve(256 * 1024);
        }
        let input = raw.get(z.total_in() as usize..).unwrap_or_default();
        let status = z.decompress_vec(input, &mut out, flate2::FlushDecompress::None).map_err(|_| PsError::Ps("ioerror", "FlateDecode".into()))?;
        if out.len() > MAX_DATA {
            return Err(PsError::Limit("decoded data is too large"));
        }
        match status {
            flate2::Status::StreamEnd => break,
            _ if (z.total_in(), z.total_out()) == before => break,
            _ => {}
        }
    }
    Ok((out, z.total_in() as usize))
}

/// LZW data (`EarlyChange` 1) decoded, and how many bytes of `raw` it took.
fn lzw(raw: &[u8]) -> Res<(Vec<u8>, usize)> {
    let mut table: Vec<Vec<u8>> = (0..=255u8).map(|b| vec![b]).collect();
    table.push(vec![]);
    table.push(vec![]);
    let (mut out, mut prev): (Vec<u8>, Option<Vec<u8>>) = (vec![], None);
    let (mut acc, mut bits, mut width, mut i) = (0u32, 0u32, 9u32, 0usize);
    loop {
        while bits < width {
            let Some(&b) = raw.get(i) else { return Ok((out, i)) };
            i += 1;
            acc = (acc << 8) | u32::from(b);
            bits += 8;
        }
        let code = ((acc >> (bits - width)) & ((1 << width) - 1)) as usize;
        bits -= width;
        match code {
            256 => {
                table.truncate(258);
                width = 9;
                prev = None;
            }
            257 => return Ok((out, i)),
            _ => {
                let entry = match (table.get(code), &prev) {
                    (Some(e), _) if !e.is_empty() => e.clone(),
                    (_, Some(p)) if code == table.len() => {
                        let mut e = p.clone();
                        e.extend(p.first());
                        e
                    }
                    _ => return io("LZWDecode"),
                };
                out.extend_from_slice(&entry);
                if let Some(mut p) = prev.take()
                    && table.len() < 4096
                {
                    p.extend(entry.first());
                    table.push(p);
                }
                width = match table.len() + 1 {
                    n if n >= 2048 => 12,
                    n if n >= 1024 => 11,
                    n if n >= 512 => 10,
                    _ => 9,
                };
                prev = Some(entry);
                if out.len() > MAX_DATA {
                    return Err(PsError::Limit("decoded data is too large"));
                }
            }
        }
    }
}

/// A JPEG's pixels (grey or RGB, 8 bits) with its channel count, and how many bytes of `raw` it
/// took.
fn dct(raw: &[u8]) -> Res<(Vec<u8>, usize, u8)> {
    let end = find(raw, &[0xFF, 0xD9]).map_or(raw.len(), |e| e + 2);
    let img = image::load_from_memory_with_format(raw.get(..end).unwrap_or_default(), image::ImageFormat::Jpeg)
        .map_err(|_| PsError::Ps("ioerror", "DCTDecode".into()))?;
    if img.color().channel_count() == 1 { Ok((img.to_luma8().into_raw(), end, 1)) } else { Ok((img.to_rgb8().into_raw(), end, 3)) }
}

/// A decoding filter and its parameters.
#[derive(Clone, Debug)]
pub(crate) struct Filter {
    name: Rc<str>,
    /// `SubFileDecode`'s count and end-of-data string.
    sub: (usize, Vec<u8>),
}

/// The bytes `f` decodes from `raw`, how many it read, and the channels of a decoded JPEG.
fn decode(f: &Filter, raw: &[u8]) -> Res<(Vec<u8>, usize, Option<u8>)> {
    let (data, used) = match &*f.name {
        "ASCIIHexDecode" => {
            let end = raw.iter().position(|b| *b == b'>');
            let body = raw.get(..end.unwrap_or(raw.len())).unwrap_or_default();
            (hex_decode(body).ok_or(PsError::Ps("ioerror", "ASCIIHexDecode".into()))?, end.map_or(raw.len(), |e| e + 1))
        }
        "ASCII85Decode" => {
            let end = find(raw, b"~>");
            let mut text = String::from_utf8_lossy(raw.get(..end.unwrap_or(raw.len())).unwrap_or_default()).into_owned();
            text.push_str("~>");
            (ps::ascii85_decode(&text).ok_or(PsError::Ps("ioerror", "ASCII85Decode".into()))?, end.map_or(raw.len(), |e| e + 2))
        }
        "RunLengthDecode" => run_length(raw)?,
        "FlateDecode" => flate(raw)?,
        "LZWDecode" => lzw(raw)?,
        "DCTDecode" => {
            let (d, used, channels) = dct(raw)?;
            return Ok((d, used, Some(channels)));
        }
        "SubFileDecode" => {
            let (count, eod) = &f.sub;
            if eod.is_empty() {
                // A count of 0 without an end string sets no end: the data passes through whole.
                let n = if *count == 0 { raw.len() } else { (*count).min(raw.len()) };
                (raw.get(..n).unwrap_or_default().to_vec(), n)
            } else {
                let (mut at, mut seen) = (0, 0);
                loop {
                    match find(raw.get(at..).unwrap_or_default(), eod) {
                        Some(i) if seen < *count => {
                            seen += 1;
                            at += i + eod.len();
                        }
                        Some(i) => break (raw.get(..at + i).unwrap_or_default().to_vec(), at + i + eod.len()),
                        None => break (raw.to_vec(), raw.len()),
                    }
                }
            }
        }
        "NullEncode" => (raw.to_vec(), raw.len()),
        other => return ps_err("undefined", other),
    };
    Ok((data, used, None))
}

/// What a source of image data gives.
enum Source {
    File(Rc<RefCell<Stream>>),
    Str(Vec<u8>),
    Proc(Obj),
}

impl Interp<'_> {
    pub fn data_op(&mut self, op: Op) -> Res {
        use Op::*;
        match op {
            CurrentFile => self.push(Obj::File { stream: Rc::new(RefCell::new(Stream::Current)), exec: false })?,
            Filter => self.filter()?,
            ReadHexString | ReadString => {
                let s = self.pop_str()?;
                let Obj::File { stream: f, .. } = self.pop()? else { return ps_err("typecheck", "") };
                let n = s.len();
                let got = if op == ReadString { self.read(&f, n)? } else { self.read_hex(&f, n)? };
                self.fill(s, &got)?;
                self.push(Obj::Bool(got.len() == n))?;
            }
            ReadLine => {
                let s = self.pop_str()?;
                let Obj::File { stream: f, .. } = self.pop()? else { return ps_err("typecheck", "") };
                let n = s.len();
                let mut line = vec![];
                let mut ended = false;
                while line.len() < n {
                    match self.read(&f, 1)?.first() {
                        Some(b'\n') | None => {
                            ended = true;
                            break;
                        }
                        Some(b'\r') => {
                            ended = true;
                            break;
                        }
                        Some(b) => line.push(*b),
                    }
                }
                self.fill(s, &line)?;
                self.push(Obj::Bool(ended))?;
            }
            FlushFile => {
                // An input file is read to its end: a filter over the program's data skips it
                // (metadata read with `flushfile`). The program's own file goes on being run.
                if let Obj::File { stream, .. } = self.pop()? {
                    self.materialize(&stream)?;
                }
            }
            CloseFile => {
                self.pop()?;
            }
            File => {
                self.pop()?;
                self.pop()?;
                return ps_err("invalidfileaccess", "file");
            }
            Eexec => self.eexec()?,
            Image | ImageMask | ColorImage => self.image(op)?,
            _ => self.text_op(op)?,
        }
        Ok(())
    }

    /// Up to `n` bytes of file `f`.
    fn read(&mut self, f: &Rc<RefCell<Stream>>, n: usize) -> Res<Vec<u8>> {
        self.materialize(f)?;
        Ok(match &mut *f.borrow_mut() {
            Stream::Current => {
                let rest = self.lex.data();
                let k = n.min(rest.len());
                let v = rest.get(..k).unwrap_or_default().to_vec();
                self.lex.advance(k);
                v
            }
            Stream::Buf { data, pos, .. } => {
                let start = (*pos).min(data.len());
                let end = start.saturating_add(n).min(data.len());
                *pos = end;
                data.get(start..end).unwrap_or_default().to_vec()
            }
            Stream::Pending(_) => vec![],
        })
    }

    /// Up to `n` bytes read as hexadecimal digits from `f` (other characters skipped).
    fn read_hex(&mut self, f: &Rc<RefCell<Stream>>, n: usize) -> Res<Vec<u8>> {
        let mut out = Vec::with_capacity(n);
        let mut high: Option<u8> = None;
        while out.len() < n {
            let Some(&b) = self.read(f, 1)?.first() else { break };
            let Some(v) = char::from(b).to_digit(16) else { continue };
            match high.take() {
                Some(h) => out.push(h << 4 | v as u8),
                None => high = Some(v as u8),
            }
        }
        Ok(out)
    }

    /// Read bytes `got` into string `s` and push the part of `s` they fill.
    fn fill(&mut self, s: Shared<u8>, got: &[u8]) -> Res {
        if !s.write(0, got) {
            return ps_err("rangecheck", "");
        }
        self.push_interval(Obj::Str(s), 0, got.len())
    }

    /// `filter`: a file that decodes its source. Over the program's own data it decodes when
    /// first read (the data follows the operator that reads it); over other data, at once.
    fn filter(&mut self) -> Res {
        let name = self.pop()?.text().ok_or(PsError::Ps("typecheck", "filter".into()))?;
        let sub = if &*name == "SubFileDecode" {
            let eod = self.pop_str()?.to_vec();
            let count = self.pop_count()?;
            (count, eod)
        } else {
            (0, vec![])
        };
        if let Some(Obj::Dict(d)) = self.stack.last().cloned() {
            self.pop()?;
            let predictor = d.borrow().get(&Key::name("Predictor")).and_then(Obj::as_num).unwrap_or(1.0);
            if predictor > 1.0 {
                return ps_err("undefined", "a filter predictor");
            }
        }
        let spec = Filter { name, sub };
        let stream = match self.pop()? {
            Obj::File { stream: f, .. } => {
                if let Stream::Buf { data, pos, .. } = &*f.borrow() {
                    self.alloc(data.len().saturating_sub(*pos))?;
                }
                let chain = match &*f.borrow() {
                    Stream::Current => Some(vec![]),
                    Stream::Pending(c) => Some(c.clone()),
                    Stream::Buf { .. } => None,
                };
                match chain {
                    Some(mut c) => {
                        // The new filter reads a pending one's data: it has none left of its own
                        // (`flushfile` on it skips nothing more).
                        if !c.is_empty() {
                            *f.borrow_mut() = Stream::Buf { data: vec![], pos: 0, rgb: None };
                        }
                        c.push(spec);
                        Stream::Pending(c)
                    }
                    None => {
                        let mut b = f.borrow_mut();
                        let Stream::Buf { data, pos, .. } = &mut *b else { return io("filter") };
                        let (out, used, rgb) = decode(&spec, data.get(*pos..).unwrap_or_default())?;
                        *pos = pos.saturating_add(used);
                        Stream::Buf { data: out, pos: 0, rgb }
                    }
                }
            }
            Obj::Str(s) => {
                self.alloc(s.borrow().len())?;
                let (data, _, rgb) = decode(&spec, &s.borrow())?;
                Stream::Buf { data, pos: 0, rgb }
            }
            p @ Obj::Array { .. } => {
                let raw = self.gather(&p, MAX_DATA)?;
                let (data, _, rgb) = decode(&spec, &raw)?;
                Stream::Buf { data, pos: 0, rgb }
            }
            _ => return ps_err("typecheck", "filter"),
        };
        self.push(Obj::File { stream: Rc::new(RefCell::new(stream)), exec: false })
    }

    /// The data an executable file runs as a program: what is left of it. `None` for the
    /// program's own file, which is being run already.
    pub(super) fn program_of(&mut self, f: &Rc<RefCell<Stream>>) -> Res<Option<Vec<u8>>> {
        if matches!(&*f.borrow(), Stream::Current) {
            return Ok(None);
        }
        self.read(f, MAX_DATA).map(Some)
    }

    /// Decode the filters a file waits to run over the program's data, from where it is now.
    fn materialize(&mut self, f: &Rc<RefCell<Stream>>) -> Res {
        let chain = match &*f.borrow() {
            Stream::Pending(c) => c.clone(),
            _ => return Ok(()),
        };
        let mut buf: Option<(Vec<u8>, Option<u8>)> = None;
        for spec in &chain {
            buf = Some(match buf.take() {
                None => {
                    let raw = self.lex.data();
                    let (data, used, rgb) = decode(spec, raw)?;
                    self.alloc(data.len())?;
                    self.lex.advance(used);
                    (data, rgb)
                }
                Some((prev, was)) => {
                    let (data, _, rgb) = decode(spec, &prev)?;
                    self.alloc(data.len())?;
                    (data, rgb.or(was))
                }
            });
        }
        let (data, rgb) = buf.unwrap_or_default();
        *f.borrow_mut() = Stream::Buf { data, pos: 0, rgb };
        Ok(())
    }

    /// The bytes a data procedure gives, called until it gives an empty string or `n` bytes.
    fn gather(&mut self, p: &Obj, n: usize) -> Res<Vec<u8>> {
        let mut out = vec![];
        while out.len() < n {
            self.call(p.clone())?;
            let s = self.pop_str()?;
            let s = s.borrow();
            if s.is_empty() {
                break;
            }
            self.alloc(s.len())?;
            out.extend_from_slice(&s);
        }
        out.truncate(n);
        Ok(out)
    }

    /// `n` bytes of image data from `src` (zeros past its end).
    fn source_bytes(&mut self, src: &Source, n: usize) -> Res<Vec<u8>> {
        let mut v = match src {
            Source::File(f) => self.read(f, n)?,
            Source::Str(s) if !s.is_empty() => s.iter().copied().cycle().take(n).collect(),
            Source::Str(_) => vec![],
            Source::Proc(p) => self.gather(p, n)?,
        };
        v.resize(n, 0);
        Ok(v)
    }

    /// `eexec`: an encrypted font program follows. It is skipped (type is drawn in the app's
    /// fonts, by name): the font dictionary it would complete is defined as it is.
    fn eexec(&mut self) -> Res {
        self.pop()?;
        if let Some(Obj::Dict(d)) = self.stack.last().cloned() {
            let name = d.borrow().get(&Key::name("FontName")).cloned();
            if let Some(k) = name.and_then(|n| n.key()) {
                self.pop()?;
                self.fonts.borrow_mut().insert(k, Obj::Dict(d));
            }
        }
        let rest = self.lex.data();
        let at = find(rest, b"cleartomark").ok_or(PsError::Ps("ioerror", "eexec".into()))?;
        self.lex.advance(at);
        self.push(Obj::Mark)
    }

    // ---------- images ----------

    fn image(&mut self, op: Op) -> Res {
        let mask = op == Op::ImageMask;
        let spec = match self.stack.last() {
            Some(Obj::Dict(_)) if op != Op::ColorImage => {
                let d = self.pop_dict()?;
                self.image_dict(&d, mask)?
            }
            _ => self.image_operands(op)?,
        };
        let Some(img) = self.decode_image(spec)? else { return Ok(()) };
        self.place_image(img)
    }

    /// The dictionary form of `image` and `imagemask`.
    fn image_dict(&mut self, d: &DictRef, mask: bool) -> Res<Spec> {
        let get = |k: &str| d.borrow().get(&Key::name(k)).cloned();
        let num = |k: &str| get(k).and_then(|o| o.as_num());
        let kind = num("ImageType").unwrap_or(1.0);
        if kind != 1.0 && kind != 4.0 {
            return ps_err("rangecheck", "ImageType");
        }
        let size = |k: &str| num(k).filter(|v| *v >= 1.0 && *v <= 65_535.0).map(|v| v as usize).ok_or(PsError::Ps("rangecheck", k.to_string()));
        let (w, h) = (size("Width")?, size("Height")?);
        let bpc = if mask { 1 } else { num("BitsPerComponent").unwrap_or(8.0) as u8 };
        let matrix =
            get("ImageMatrix").and_then(|o| o.items().and_then(|i| matrix_of(&i.borrow()))).ok_or(PsError::Ps("typecheck", "ImageMatrix".into()))?;
        let decode: Vec<f64> = get("Decode").and_then(|o| o.items().map(|i| i.borrow().iter().filter_map(Obj::as_num).collect())).unwrap_or_default();
        let space = if mask { Space::Gray } else { (*self.g.space).clone() };
        let multi = get("MultipleDataSources").is_some_and(|o| matches!(o, Obj::Bool(true)));
        let src = get("DataSource").ok_or(PsError::Ps("undefined", "DataSource".into()))?;
        let sources = match (&src, multi) {
            (Obj::Array { items, exec: false }, true) => items.borrow().iter().map(source).collect::<Res<Vec<_>>>()?,
            _ => vec![source(&src)?],
        };
        let mask_color: Option<Vec<u32>> = (kind == 4.0)
            .then(|| {
                get("MaskColor").and_then(|o| o.items().map(|i| i.borrow().iter().filter_map(|v| v.as_num().map(|v| v.max(0.0) as u32)).collect()))
            })
            .flatten();
        // An image mask paints its 0 bits with Decode [0 1], its 1 bits with [1 0].
        let stencil = mask.then(|| decode.first().is_some_and(|v| *v > 0.5));
        Ok(Spec { w, h, bpc, space, decode, matrix, sources, mask_color, stencil })
    }

    /// The operand forms: `w h bpc matrix src image`, `w h polarity matrix src imagemask`, and
    /// `w h bpc matrix src… multi ncomp colorimage`.
    fn image_operands(&mut self, op: Op) -> Res<Spec> {
        let (n, multi) = if op == Op::ColorImage {
            let n = self.pop_count()?;
            (n, self.pop_bool()?)
        } else {
            (1, false)
        };
        let space = match n {
            1 => Space::Gray,
            3 => Space::Rgb,
            4 => Space::Cmyk,
            _ => return ps_err("rangecheck", "colorimage"),
        };
        let mut sources = vec![];
        for _ in 0..if multi { n } else { 1 } {
            let o = self.pop()?;
            sources.push(source(&o)?);
        }
        sources.reverse();
        let matrix = self.pop_matrix()?;
        let (bpc, stencil) = if op == Op::ImageMask { (1, Some(self.pop_bool()?)) } else { (self.pop_int()?.clamp(1, 16) as u8, None) };
        let size = |v: i64| usize::try_from(v).ok().filter(|v| (1..=65_535).contains(v)).ok_or(PsError::Ps("rangecheck", "image".into()));
        let h = size(self.pop_int()?)?;
        let w = size(self.pop_int()?)?;
        Ok(Spec { w, h, bpc, space, decode: vec![], matrix, sources, mask_color: None, stencil })
    }

    /// The image's pixels, straight RGBA, rows from the first one read.
    fn decode_image(&mut self, s: Spec) -> Res<Option<Pixels>> {
        if !matches!(s.bpc, 1 | 2 | 4 | 8 | 12 | 16) {
            return ps_err("rangecheck", "BitsPerComponent");
        }
        if s.w.saturating_mul(s.h) > MAX_PIXELS {
            self.out.warn("images too large to read were left out");
            return Ok(None);
        }
        self.alloc(s.w * s.h * 4)?;
        let n = if s.stencil.is_some() { 1 } else { s.space.n() };
        let bpc = usize::from(s.bpc);
        for src in &s.sources {
            if let Source::File(f) = src {
                self.materialize(f)?;
            }
        }
        // A decoded JPEG gives 8-bit grey or RGB samples whatever the colour space says.
        let jpeg = s.sources.iter().find_map(|src| match src {
            Source::File(f) => match &*f.borrow() {
                Stream::Buf { rgb, .. } => *rgb,
                _ => None,
            },
            _ => None,
        });
        let (n, bpc, space) = match jpeg {
            Some(3) => (3, 8, Space::Rgb),
            Some(_) => (1, 8, Space::Gray),
            None => (n, bpc, s.space.clone()),
        };
        let multi = s.sources.len() > 1;
        let row_bits = if multi { s.w * bpc } else { s.w * n * bpc };
        let row = row_bits.div_ceil(8);
        let planes: Vec<Vec<u8>> = if multi {
            let mut v = vec![];
            for src in &s.sources {
                v.push(self.source_bytes(src, row * s.h)?);
            }
            v
        } else {
            let src = s.sources.first().ok_or(PsError::Ps("undefined", "DataSource".into()))?;
            vec![self.source_bytes(src, row * s.h)?]
        };
        let max = ((1u32 << bpc.min(16)) - 1) as f64;
        let sample = |plane: &[u8], y: usize, i: usize| -> u32 {
            let bit = y * row * 8 + i * bpc;
            let byte = bit / 8;
            match bpc {
                8 => u32::from(plane.get(byte).copied().unwrap_or(0)),
                16 => u32::from(plane.get(byte).copied().unwrap_or(0)) << 8 | u32::from(plane.get(byte + 1).copied().unwrap_or(0)),
                12 => {
                    let v = u32::from(plane.get(byte).copied().unwrap_or(0)) << 8 | u32::from(plane.get(byte + 1).copied().unwrap_or(0));
                    if bit.is_multiple_of(8) { v >> 4 } else { v & 0xfff }
                }
                _ => (u32::from(plane.get(byte).copied().unwrap_or(0)) >> (8 - bpc - bit % 8)) & ((1 << bpc) - 1),
            }
        };
        let default_decode: Vec<f64> = match &space {
            Space::Indexed { .. } => vec![0.0, max],
            _ => (0..n).flat_map(|_| [0.0, 1.0]).collect(),
        };
        let decode = if s.decode.len() >= 2 * n && jpeg.is_none() { s.decode.clone() } else { default_decode };
        let paint = self.g.paint.color().map(|c| c.to_rgba8(1.0));
        let mut rgba = Vec::with_capacity(s.w * s.h * 4);
        let mut cache: HashMap<Vec<u32>, [u8; 3]> = HashMap::new();
        let mut raw = vec![0u32; n];
        let mut comps = vec![0f64; n];
        for y in 0..s.h {
            for x in 0..s.w {
                for (c, slot) in raw.iter_mut().enumerate() {
                    *slot = match planes.as_slice() {
                        [one] => sample(one, y, x * n + c),
                        many => many.get(c).map_or(0, |p| sample(p, y, x)),
                    };
                }
                if let Some(paint_ones) = s.stencil {
                    let on = (raw.first().copied().unwrap_or(0) == 1) == paint_ones;
                    let [r, g, b, _] = paint.unwrap_or([0, 0, 0, 255]);
                    rgba.extend([r, g, b, if on { 255 } else { 0 }]);
                    continue;
                }
                let masked = s.mask_color.as_ref().is_some_and(|m| masked(m, &raw));
                for (c, slot) in comps.iter_mut().enumerate() {
                    let (d0, d1) = (decode.get(2 * c).copied().unwrap_or(0.0), decode.get(2 * c + 1).copied().unwrap_or(1.0));
                    *slot = d0 + f64::from(*raw.get(c).unwrap_or(&0)) * (d1 - d0) / max;
                }
                let rgb = match (&space, raw.as_slice()) {
                    (Space::Gray, [v]) if bpc == 8 && decode == [0.0, 1.0] => [*v as u8; 3],
                    (Space::Rgb, [r, g, b]) if bpc == 8 && decode == [0.0, 1.0, 0.0, 1.0, 0.0, 1.0] => [*r as u8, *g as u8, *b as u8],
                    _ => match cache.get(&raw) {
                        Some(c) => *c,
                        None => {
                            let c = self.sample_rgb(&space, &comps)?;
                            if cache.len() < MAX_TINTS {
                                cache.insert(raw.clone(), c);
                            }
                            c
                        }
                    },
                };
                rgba.extend([rgb[0], rgb[1], rgb[2], if masked { 0 } else { 255 }]);
            }
        }
        Ok(Some(Pixels { w: s.w as u32, h: s.h as u32, rgba, matrix: s.matrix }))
    }

    /// One pixel's colour in `space` as display RGB.
    fn sample_rgb(&mut self, space: &Space, comps: &[f64]) -> Res<[u8; 3]> {
        let color = match space {
            Space::Gray | Space::Rgb | Space::Cmyk => process(space, comps),
            _ => match self.paint_of(space, comps)? {
                Paint::Solid { color, .. } => Some(color),
                _ => None,
            },
        };
        let [r, g, b, _] = color.unwrap_or(Color::BLACK).to_rgba8(1.0);
        Ok([r, g, b])
    }

    /// Place decoded pixels in user space (its unit square through the image matrix).
    fn place_image(&mut self, img: Pixels) -> Res {
        let m = img.matrix;
        if m.determinant().abs() < 1e-12 {
            return Ok(());
        }
        let xf = self.xf() * m.inverse();
        if !xf.as_coeffs().iter().all(|v| v.is_finite()) || xf.determinant().abs() < 1e-12 {
            return Ok(());
        }
        let Some(rgba) = image::RgbaImage::from_raw(img.w, img.h, img.rgba) else { return Ok(()) };
        let mut png = vec![];
        if rgba.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).is_err() {
            self.out.warn("an image that couldn't be read was left out");
            return Ok(());
        }
        let blob = ImageBlob::new("image/png", png);
        let key = blob.content_key();
        self.out.doc.images.entry(key.clone()).or_insert(blob);
        let im = ImageObject { key, width: img.w, height: img.h, xf, link: None, placement: Default::default() };
        let clips = self.g.clips.clone();
        self.out.push(Node::new(NodeId(0), NodeKind::Image(im)), &clips);
        Ok(())
    }
}

/// An image to decode.
struct Spec {
    w: usize,
    h: usize,
    bpc: u8,
    space: Space,
    decode: Vec<f64>,
    /// User space → image space.
    matrix: Affine,
    /// One source, or one per component.
    sources: Vec<Source>,
    /// `MaskColor` (ImageType 4): samples that are transparent.
    mask_color: Option<Vec<u32>>,
    /// An image mask: whether its 1 bits paint.
    stencil: Option<bool>,
}

/// Decoded pixels.
struct Pixels {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
    matrix: Affine,
}

fn source(o: &Obj) -> Res<Source> {
    Ok(match o {
        Obj::File { stream: f, .. } => Source::File(f.clone()),
        Obj::Str(s) => Source::Str(s.to_vec()),
        Obj::Array { items, .. } => Source::Proc(Obj::Array { items: items.clone(), exec: true }),
        _ => return ps_err("typecheck", "DataSource"),
    })
}

/// Do samples `raw` match `MaskColor` `m` (one value per component, or a range)?
fn masked(m: &[u32], raw: &[u32]) -> bool {
    if m.len() == raw.len() {
        return m.iter().zip(raw).all(|(a, b)| a == b);
    }
    m.len() == raw.len() * 2
        && raw.iter().enumerate().all(|(i, v)| (m.get(2 * i).copied().unwrap_or(0)..=m.get(2 * i + 1).copied().unwrap_or(0)).contains(v))
}
