//! Roidery: `cargo xtask icns <out.icns> <png>...`: pack square PNGs into a macOS `.icns` without
//! `iconutil`, so the icon set can be made on any OS. Used by `packaging/roidery-icons.sh`.
//!
//! Every entry is PNG data. Each PNG fills every icon type of its size: a 64 px PNG is both the
//! 64 px icon and the 32 px icon at 2×.

use std::path::Path;

/// The icon types that hold PNG data, by pixel size.
const TYPES: [(u32, &[u8; 4]); 11] = [
    (16, b"icp4"),
    (32, b"icp5"),
    (32, b"ic11"),
    (64, b"icp6"),
    (64, b"ic12"),
    (128, b"ic07"),
    (256, b"ic08"),
    (256, b"ic13"),
    (512, b"ic09"),
    (512, b"ic14"),
    (1024, b"ic10"),
];

/// Build the `.icns` bytes from PNG images.
pub fn pack(pngs: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    for png in pngs {
        let (w, h) = crate::ico::png_size(png)?;
        if w != h {
            return Err(format!("{w}x{h} is not square"));
        }
        let types: Vec<_> = TYPES.iter().filter(|t| t.0 == w).collect();
        if types.is_empty() {
            return Err(format!("{w} px is not an icon size (16, 32, 64, 128, 256, 512 or 1024)"));
        }
        let len = u32::try_from(png.len() + 8).map_err(|_| "image too large")?;
        for (_, t) in types {
            body.extend_from_slice(*t);
            body.extend_from_slice(&len.to_be_bytes());
            body.extend_from_slice(png);
        }
    }
    let len = u32::try_from(body.len() + 8).map_err(|_| "icon set too large")?;
    let mut out = b"icns".to_vec();
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn run(args: &[&str]) -> Result<(), String> {
    let [out, inputs @ ..] = args else {
        return Err("usage: cargo xtask icns <out.icns> <png>...".into());
    };
    if inputs.is_empty() {
        return Err("usage: cargo xtask icns <out.icns> <png>...".into());
    }
    let pngs = inputs.iter().map(|p| std::fs::read(p).map_err(|e| format!("{p}: {e}"))).collect::<Result<Vec<_>, _>>()?;
    std::fs::write(Path::new(out), pack(&pngs)?).map_err(|e| format!("{out}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(size: u32) -> Vec<u8> {
        let mut p = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        p.extend_from_slice(&size.to_be_bytes());
        p.extend_from_slice(&size.to_be_bytes());
        p.extend_from_slice(&[8, 6, 0, 0, 0]);
        p
    }

    #[test]
    fn each_png_fills_every_type_of_its_size() {
        let out = pack(&[png(16), png(64)]).unwrap();
        assert_eq!(&out[..4], b"icns");
        assert_eq!(u32::from_be_bytes([out[4], out[5], out[6], out[7]]) as usize, out.len());
        let entry = 8 + png(16).len();
        let types: Vec<&[u8]> = [8, 8 + entry, 8 + 2 * entry].iter().map(|&i| &out[i..i + 4]).collect();
        assert_eq!(types, [&b"icp4"[..], b"icp6", b"ic12"]);
    }

    #[test]
    fn odd_sizes_are_refused() {
        assert!(pack(&[png(48)]).is_err());
    }
}
