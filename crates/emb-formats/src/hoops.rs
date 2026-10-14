//! Hoops by machine brand: the sewing field each hoop allows, from the manufacturers' published
//! hoop sizes.
//!
//! A design is sewn from the hoop's centre (where the needle starts), so it fits when its size,
//! centred, stays inside the field. Commercial round hoops are listed by inside diameter; their
//! sewing field is taken as that circle less 10 mm all round, the clearance the presser foot
//! needs.

/// One hoop's sewing field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hoop {
    /// Stable id, `brand-size`.
    pub id: &'static str,
    pub brand: &'static str,
    /// The hoop's name as the brand gives it, or its size.
    pub name: &'static str,
    /// Sewing field, millimetres (for a round hoop, its diameter both ways).
    pub width: f64,
    pub height: f64,
    pub round: bool,
}

const fn h(id: &'static str, brand: &'static str, name: &'static str, width: f64, height: f64) -> Hoop {
    Hoop { id, brand, name, width, height, round: false }
}

/// A commercial round hoop of `inside` mm: the field is 10 mm in from the frame.
const fn round(id: &'static str, name: &'static str, inside: f64) -> Hoop {
    Hoop { id, brand: COMMERCIAL, name, width: inside - 20.0, height: inside - 20.0, round: true }
}

const COMMERCIAL: &str = "Commercial";

/// Every hoop, grouped by brand in [`brands`] order, smallest first.
pub const HOOPS: &[Hoop] = &[
    h("brother-4x4", "Brother", "4 × 4 in", 100.0, 100.0),
    h("brother-5x7", "Brother", "5 × 7 in", 130.0, 180.0),
    h("brother-6x10", "Brother", "6 × 10 in", 160.0, 260.0),
    h("brother-8x8", "Brother", "8 × 8 in", 200.0, 200.0),
    h("brother-8x12", "Brother", "8 × 12 in", 200.0, 300.0),
    h("brother-9.5x14", "Brother", "9.5 × 14 in", 240.0, 360.0),
    h("babylock-4x4", "Baby Lock", "4 × 4 in", 100.0, 100.0),
    h("babylock-5x7", "Baby Lock", "5 × 7 in", 130.0, 180.0),
    h("babylock-6x10", "Baby Lock", "6 × 10 in", 160.0, 260.0),
    h("babylock-8x12", "Baby Lock", "8 × 12 in", 200.0, 300.0),
    h("babylock-9.5x14", "Baby Lock", "9.5 × 14 in", 240.0, 360.0),
    h("janome-sq14", "Janome", "SQ14", 140.0, 140.0),
    h("janome-re20", "Janome", "RE20", 170.0, 200.0),
    h("janome-sq20", "Janome", "SQ20", 200.0, 200.0),
    h("janome-re28", "Janome", "RE28", 200.0, 280.0),
    h("janome-sq23", "Janome", "SQ23", 230.0, 230.0),
    h("janome-re36", "Janome", "RE36", 200.0, 360.0),
    h("bernina-medium", "Bernina", "Medium", 100.0, 130.0),
    h("bernina-large-oval", "Bernina", "Large Oval", 145.0, 255.0),
    h("bernina-midi", "Bernina", "Midi", 265.0, 165.0),
    h("bernina-maxi", "Bernina", "Maxi", 210.0, 400.0),
    h("bernina-jumbo", "Bernina", "Jumbo", 260.0, 400.0),
    h("husqvarna-100x100", "Husqvarna Viking", "100 × 100 mm", 100.0, 100.0),
    h("husqvarna-120x120", "Husqvarna Viking", "120 × 120 mm", 120.0, 120.0),
    h("husqvarna-240x150", "Husqvarna Viking", "240 × 150 mm", 240.0, 150.0),
    h("husqvarna-260x200", "Husqvarna Viking", "260 × 200 mm", 260.0, 200.0),
    h("husqvarna-360x200", "Husqvarna Viking", "360 × 200 mm", 360.0, 200.0),
    h("husqvarna-360x350", "Husqvarna Viking", "360 × 350 mm", 360.0, 350.0),
    h("pfaff-120x115", "Pfaff", "120 × 115 mm", 120.0, 115.0),
    h("pfaff-240x150", "Pfaff", "240 × 150 mm", 240.0, 150.0),
    h("pfaff-260x200", "Pfaff", "260 × 200 mm", 260.0, 200.0),
    h("pfaff-360x200", "Pfaff", "360 × 200 mm", 360.0, 200.0),
    h("pfaff-360x350", "Pfaff", "360 × 350 mm", 360.0, 350.0),
    h("singer-4x4", "Singer", "4 × 4 in", 100.0, 100.0),
    h("singer-10x6", "Singer", "10 × 6 in", 260.0, 150.0),
    round("commercial-12", "12 cm round", 120.0),
    round("commercial-15", "15 cm round", 150.0),
    round("commercial-18", "18 cm round", 180.0),
    round("commercial-21", "21 cm round", 210.0),
    round("commercial-24", "24 cm round", 240.0),
    round("commercial-30", "30 cm round", 300.0),
    h("commercial-cap", COMMERCIAL, "Cap frame", 130.0, 60.0),
];

/// The brands, in the order [`HOOPS`] lists them.
pub fn brands() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = vec![];
    for hoop in HOOPS {
        if !out.contains(&hoop.brand) {
            out.push(hoop.brand);
        }
    }
    out
}

/// The hoop with `id`, ignoring case.
pub fn find(id: &str) -> Option<&'static Hoop> {
    HOOPS.iter().find(|h| h.id.eq_ignore_ascii_case(id))
}

/// How a design sits in a hoop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fit {
    Fits,
    /// Only turned a quarter turn.
    Rotated,
    /// By how much it is too wide and too tall, mm (0 for a side that fits).
    TooLarge {
        over_w: f64,
        over_h: f64,
    },
}

/// How a design `w` × `h` mm, centred, sits in a sewing field `fw` × `fh` mm (a circle of
/// diameter `fw` when `round`).
pub fn fit(w: f64, h: f64, fw: f64, fh: f64, round: bool) -> Fit {
    // Rounding in the machine file can add a tenth of a millimetre.
    const SLACK: f64 = 0.15;
    if round {
        let over = (w.hypot(h) - fw).max(0.0);
        return if over <= SLACK { Fit::Fits } else { Fit::TooLarge { over_w: over, over_h: over } };
    }
    if w <= fw + SLACK && h <= fh + SLACK {
        Fit::Fits
    } else if h <= fw + SLACK && w <= fh + SLACK {
        Fit::Rotated
    } else {
        Fit::TooLarge { over_w: (w - fw).max(0.0), over_h: (h - fh).max(0.0) }
    }
}

impl Hoop {
    pub fn fit(&self, w: f64, h: f64) -> Fit {
        fit(w, h, self.width, self.height, self.round)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_brands_grouped() {
        for (i, a) in HOOPS.iter().enumerate() {
            assert!(HOOPS.iter().skip(i + 1).all(|b| b.id != a.id), "{}", a.id);
            assert!(a.width > 0.0 && a.height > 0.0);
        }
        let b = brands();
        assert_eq!(b.first(), Some(&"Brother"));
        assert_eq!(b.last(), Some(&COMMERCIAL));
        // Each brand's hoops come together.
        let order: Vec<usize> = HOOPS.iter().map(|h| b.iter().position(|x| *x == h.brand).unwrap()).collect();
        assert!(order.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(find("JANOME-RE20").map(|h| h.height), Some(200.0));
    }

    #[test]
    fn fits_turned_and_too_large() {
        let five_by_seven = find("brother-5x7").unwrap();
        assert_eq!(five_by_seven.fit(120.0, 170.0), Fit::Fits);
        assert_eq!(five_by_seven.fit(170.0, 120.0), Fit::Rotated);
        assert_eq!(five_by_seven.fit(140.0, 190.0), Fit::TooLarge { over_w: 10.0, over_h: 10.0 });
        // Round: the corners count. A 70 mm square has a 99 mm diagonal.
        let round = find("commercial-12").unwrap();
        assert_eq!(round.fit(70.0, 70.0), Fit::Fits);
        assert!(matches!(round.fit(80.0, 80.0), Fit::TooLarge { .. }));
    }
}
