//! CMYK images: the ink amounts of image blobs that hold CMYK samples.
//!
//! Two encodings hold them: a four-component JPEG, whose samples are stored inverted with Adobe's
//! marker (the convention JPEG readers and PDF writers take a CMYK JPEG to follow), and a CMYK
//! TIFF (8 or 16 bits, chunky, ink amounts as they are). [`ImageBlob::cmyk_tiff`] writes the
//! latter, Deflate-compressed. Every other image decodes to RGB(A) only.

use std::io::Cursor;

use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

use crate::ImageBlob;

/// Most pixels a CMYK image is decoded with (256 MB of samples): larger ones decode as RGB.
pub const MAX_CMYK_PIXELS: u64 = 1 << 26;

/// The ink amounts of a CMYK image: C, M, Y and K bytes per pixel, 0 is no ink.
#[derive(Clone, Debug, PartialEq)]
pub struct Inks {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Inks {
    /// `data` as a `width` × `height` image; `None` when it doesn't hold that many pixels, the
    /// image is empty or it has more than [`MAX_CMYK_PIXELS`].
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        let n = pixels(width, height)?;
        (data.len() as u64 == n * 4).then_some(Self { width, height, data })
    }
}

/// `width` × `height` when it is a CMYK image's size: not empty, at most [`MAX_CMYK_PIXELS`].
fn pixels(width: u32, height: u32) -> Option<u64> {
    let n = width as u64 * height as u64;
    (n > 0 && n <= MAX_CMYK_PIXELS).then_some(n)
}

/// The number of colour components of JPEG `bytes` (its frame header); `None` when they aren't a
/// JPEG.
pub fn jpeg_components(bytes: &[u8]) -> Option<usize> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut d = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), DecoderOptions::default());
    d.decode_headers().ok()?;
    Some(d.info()?.components as usize)
}

fn is_tiff(bytes: &[u8]) -> bool {
    bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*")
}

/// The colour type of TIFF `bytes` and its decoder.
fn tiff(bytes: &[u8]) -> Option<(tiff::ColorType, tiff::decoder::Decoder<Cursor<&[u8]>>)> {
    if !is_tiff(bytes) {
        return None;
    }
    let mut d = tiff::decoder::Decoder::new(Cursor::new(bytes)).ok()?;
    Some((d.colortype().ok()?, d))
}

impl ImageBlob {
    /// A CMYK TIFF of ink amounts `inks` (Deflate-compressed); `None` when it can't be written.
    pub fn cmyk_tiff(inks: &Inks) -> Option<Self> {
        use tiff::encoder::{Compression, TiffEncoder, colortype::CMYK8, compression::DeflateLevel};
        let mut out = Cursor::new(Vec::new());
        TiffEncoder::new(&mut out)
            .ok()?
            .with_compression(Compression::Deflate(DeflateLevel::Balanced))
            .write_image::<CMYK8>(inks.width, inks.height, &inks.data)
            .ok()?;
        Some(Self::new("image/tiff", out.into_inner()))
    }

    /// Does the image hold CMYK samples (a four-component JPEG or a CMYK TIFF)? Reads only its
    /// header.
    pub fn is_cmyk(&self) -> bool {
        jpeg_components(&self.bytes) == Some(4) || tiff(&self.bytes).is_some_and(|(t, _)| matches!(t, tiff::ColorType::CMYK(8 | 16)))
    }

    /// The ink amounts of a CMYK image; `None` for other images, and ones that can't be decoded
    /// or are larger than [`MAX_CMYK_PIXELS`].
    pub fn cmyk(&self) -> Option<Inks> {
        let bytes = self.bytes.as_slice();
        if bytes.starts_with(&[0xFF, 0xD8]) { jpeg_inks(bytes) } else { tiff_inks(bytes) }
    }
}

/// The ink amounts of a four-component JPEG (its samples inverted back; YCCK converted to CMYK
/// first, as JPEG readers do).
fn jpeg_inks(bytes: &[u8]) -> Option<Inks> {
    let mut d = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), DecoderOptions::default());
    d.decode_headers().ok()?;
    let space = d.input_colorspace()?;
    if d.info()?.components != 4 || !matches!(space, ColorSpace::CMYK | ColorSpace::YCCK) {
        return None;
    }
    let (w, h) = d.dimensions()?;
    let (w, h) = (u32::try_from(w).ok()?, u32::try_from(h).ok()?);
    pixels(w, h)?;
    d.set_options(DecoderOptions::default().jpeg_set_out_colorspace(space));
    let mut data = d.decode().ok()?;
    for p in data.as_chunks_mut::<4>().0 {
        if space == ColorSpace::YCCK {
            let [y, cb, cr] = [p[0], p[1], p[2]].map(f32::from);
            // libjpeg's YCCK → CMYK: the complement of the YCbCr colour's RGB.
            p[0] = (434.456 - y - 1.402 * cr).clamp(0.0, 255.0) as u8;
            p[1] = (119.541 - y + 0.344 * cb + 0.714 * cr).clamp(0.0, 255.0) as u8;
            p[2] = (481.816 - y - 1.772 * cb).clamp(0.0, 255.0) as u8;
        }
        for v in p.iter_mut() {
            *v = 255 - *v;
        }
    }
    Inks::new(w, h, data)
}

/// The ink amounts of a chunky CMYK TIFF (16-bit samples to 8 bits).
fn tiff_inks(bytes: &[u8]) -> Option<Inks> {
    let (kind, mut d) = tiff(bytes)?;
    let (w, h) = d.dimensions().ok()?;
    // Planar samples (one plane per ink) would read as interleaved.
    let planar = d.find_tag_unsigned::<u16>(tiff::tags::Tag::PlanarConfiguration).ok().flatten().is_some_and(|p| p != 1);
    if !matches!(kind, tiff::ColorType::CMYK(8 | 16)) || planar || pixels(w, h).is_none() {
        return None;
    }
    let data = match d.read_image().ok()? {
        tiff::decoder::DecodingResult::U8(v) => v,
        tiff::decoder::DecodingResult::U16(v) => v.iter().map(|s| (s >> 8) as u8).collect(),
        _ => return None,
    };
    Inks::new(w, h, data)
}

/// A TIFF whose pixels have samples after their colour, which the image decoder doesn't read at
/// all, as RGBA pixels: CMYK with an alpha channel (5 samples), each colour converted by `rgb` (ink
/// amounts 0..1 → sRGB 0..1; the inks can't be kept with the transparency, so they show as RGB),
/// and grayscale with extra samples, of which the first is the alpha when the file says so (other
/// channels are left out). Associated (premultiplied) alpha is divided back out. `None` for other
/// images, planar ones and ones larger than [`MAX_CMYK_PIXELS`].
pub fn extra_samples_tiff_rgba(bytes: &[u8], rgb: impl Fn([f32; 4]) -> [f32; 3]) -> Option<image::RgbaImage> {
    use tiff::tags::Tag;
    let (kind, mut d) = tiff(bytes)?;
    let (w, h) = d.dimensions().ok()?;
    let planar = d.find_tag_unsigned::<u16>(Tag::PlanarConfiguration).ok().flatten().is_some_and(|p| p != 1);
    // 0: white is zero, 1: black is zero (the two grayscales).
    let photometric = d.find_tag_unsigned::<u16>(Tag::PhotometricInterpretation).ok().flatten();
    // ExtraSamples 1: associated alpha (premultiplied colour), 2: unassociated alpha, 0: not alpha.
    let extra = d.find_tag_unsigned_vec::<u16>(Tag::ExtraSamples).ok().flatten().and_then(|v| v.first().copied());
    // (samples in a pixel, how many of them are its colour, is the next one alpha?)
    let (samples, colour, alpha) = match kind {
        tiff::ColorType::CMYKA(8 | 16) => (5, 4, true),
        tiff::ColorType::Multiband { bit_depth: 8 | 16, num_samples: n @ 2.. } if matches!(photometric, Some(0 | 1)) => {
            (usize::from(n), 1, matches!(extra, Some(1 | 2)))
        }
        _ => return None,
    };
    if planar {
        return None;
    }
    let n = usize::try_from(pixels(w, h)?).ok()?;
    let data: Vec<u8> = match d.read_image().ok()? {
        tiff::decoder::DecodingResult::U8(v) => v,
        tiff::decoder::DecodingResult::U16(v) => v.iter().map(|s| (s >> 8) as u8).collect(),
        _ => return None,
    };
    if Some(data.len()) != n.checked_mul(samples) {
        return None;
    }
    let mut out = Vec::with_capacity(n * 4);
    for p in data.chunks_exact(samples) {
        let opacity = if alpha { p.get(colour).copied().unwrap_or(u8::MAX) } else { u8::MAX };
        let a = f32::from(opacity) / 255.0;
        let level = |v: u8| {
            let v = f32::from(v) / 255.0;
            if extra == Some(1) && a > 0.0 { (v / a).min(1.0) } else { v }
        };
        let [r, g, b] = match *p {
            [c, m, y, k, ..] if colour == 4 => rgb([level(c), level(m), level(y), level(k)]),
            [v, ..] if photometric == Some(0) => [1.0 - level(v); 3],
            [v, ..] => [level(v); 3],
            [] => continue,
        };
        out.extend([r, g, b].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        out.push(opacity);
    }
    image::RgbaImage::from_raw(w, h, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cmyk_tiff_with_alpha_reads_as_rgba() {
        use tiff::encoder::{TiffEncoder, colortype::CMYKA8};
        // 2 × 1: full cyan opaque, then 100% K at half alpha (premultiplied: K 128, alpha 128).
        let mut file = Cursor::new(Vec::new());
        let mut enc = TiffEncoder::new(&mut file).unwrap();
        let mut img = enc.new_image::<CMYKA8>(2, 1).unwrap();
        img.encoder().write_tag(tiff::tags::Tag::ExtraSamples, &[1u16][..]).unwrap();
        img.write_data(&[255, 0, 0, 0, 255, 0, 0, 0, 128, 128]).unwrap();
        let bytes = file.into_inner();
        let naive = |[c, m, y, k]: [f32; 4]| [(1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k)];
        let px = extra_samples_tiff_rgba(&bytes, naive).expect("CMYKA reads");
        assert_eq!(px.dimensions(), (2, 1));
        assert_eq!(px.get_pixel(0, 0).0, [0, 255, 255, 255], "cyan, opaque");
        assert_eq!(px.get_pixel(1, 0).0, [0, 0, 0, 128], "the premultiplied K divided back out: black at half alpha");
        // Not CMYKA: not this reader's.
        assert!(extra_samples_tiff_rgba(&ImageBlob::cmyk_tiff(&inks()).unwrap().bytes, naive).is_none());
    }

    /// Two 8-bit samples a pixel: gray and one extra sample, which the file's `ExtraSamples` names.
    struct GrayExtra8;
    impl tiff::encoder::colortype::ColorType for GrayExtra8 {
        type Inner = u8;
        const TIFF_VALUE: tiff::tags::PhotometricInterpretation = tiff::tags::PhotometricInterpretation::BlackIsZero;
        const BITS_PER_SAMPLE: &'static [u16] = &[8, 8];
        const SAMPLE_FORMAT: &'static [tiff::tags::SampleFormat] = &[tiff::tags::SampleFormat::Uint; 2];
        fn horizontal_predict(row: &[u8], result: &mut Vec<u8>) {
            result.extend_from_slice(row);
        }
    }

    /// A grayscale TIFF with an alpha channel, or with a channel that isn't one (#1466): the image
    /// decoder reads neither.
    #[test]
    fn a_grayscale_tiff_with_an_extra_channel_reads_as_rgba() {
        let gray = |extra: u16| {
            let mut file = Cursor::new(Vec::new());
            let mut enc = tiff::encoder::TiffEncoder::new(&mut file).unwrap();
            let mut img = enc.new_image::<GrayExtra8>(3, 1).unwrap();
            img.encoder().write_tag(tiff::tags::Tag::ExtraSamples, &[extra][..]).unwrap();
            // White opaque, a gray of 64 at 80% alpha (premultiplied: 51), black with no alpha.
            img.write_data(&[255, 255, 51, 204, 0, 0]).unwrap();
            let bytes = file.into_inner();
            assert!(image::load_from_memory(&bytes).is_err(), "the image decoder reads it now: this reader may go");
            extra_samples_tiff_rgba(&bytes, |_| [0.0; 3]).expect("gray with an extra sample reads").into_raw()
        };
        assert_eq!(gray(1), [255, 255, 255, 255, 64, 64, 64, 204, 0, 0, 0, 0], "associated alpha, divided back out");
        assert_eq!(gray(2), [255, 255, 255, 255, 51, 51, 51, 204, 0, 0, 0, 0], "unassociated alpha");
        assert_eq!(gray(0), [255, 255, 255, 255, 51, 51, 51, 255, 0, 0, 0, 255], "a channel that isn't alpha: opaque");
    }

    fn inks() -> Inks {
        Inks::new(3, 2, vec![0, 40, 100, 0, 255, 0, 0, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 0, 0, 255, 12, 34, 56, 78]).unwrap()
    }

    #[test]
    fn a_cmyk_tiff_keeps_its_ink_amounts_and_shows_as_rgb() {
        let b = ImageBlob::cmyk_tiff(&inks()).unwrap();
        assert_eq!(b.mime, "image/tiff");
        assert!(b.is_cmyk());
        assert_eq!(b.cmyk(), Some(inks()), "lossless");
        // Every image reader sees an RGB image: cyan is (0, 255, 255).
        let rgb = image::load_from_memory(&b.bytes).unwrap().to_rgb8();
        assert_eq!(rgb.get_pixel(1, 0).0, [0, 255, 255]);
    }

    #[test]
    fn a_cmyk_jpeg_reads_back_as_ink_amounts() {
        // A flat CMYK JPEG: Adobe's inverted samples with its marker.
        let flat = [10u8, 40, 100, 20];
        let px: Vec<u8> = (0..16 * 16).flat_map(|_| flat).collect();
        let mut jpg = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpg, 100).encode(&px, 16, 16, jpeg_encoder::ColorType::Cmyk).unwrap();
        let b = ImageBlob::new("image/jpeg", jpg);
        assert_eq!(jpeg_components(&b.bytes), Some(4));
        assert!(b.is_cmyk());
        let back = b.cmyk().unwrap();
        assert_eq!((back.width, back.height), (16, 16));
        for p in back.data.as_chunks::<4>().0 {
            assert!(p.iter().zip(flat).all(|(a, b)| a.abs_diff(b) <= 2), "{p:?}");
        }
    }

    #[test]
    fn other_images_are_not_cmyk() {
        let mut png = Vec::new();
        image::RgbaImage::new(2, 2).write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let mut jpg = Vec::new();
        image::RgbImage::new(8, 8).write_to(&mut Cursor::new(&mut jpg), image::ImageFormat::Jpeg).unwrap();
        for b in [ImageBlob::png(png), ImageBlob::new("image/jpeg", jpg), ImageBlob::new("image/tiff", b"II*\0junk".to_vec())] {
            assert!(!b.is_cmyk() && b.cmyk().is_none(), "{}", b.mime);
        }
        assert_eq!(Inks::new(2, 2, vec![0; 15]), None, "too few samples");
        assert_eq!(Inks::new(0, 2, vec![]), None, "empty");
        assert_eq!(Inks::new(1 << 14, 1 << 13, vec![]), None, "too large");
    }
}
