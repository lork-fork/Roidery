//! Fabric presets: the density, pull compensation and underlay that suit a fabric, as common
//! digitizing practice sets them. Stretchy and lofty fabrics pull stitches in more and need more
//! underlay to hold the fabric and keep the nap down; firm fabrics need less.

use crate::settings::{StitchSettings, Underlay};

/// One fabric preset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fabric {
    pub id: &'static str,
    pub label: &'static str,
    /// Satin density, mm between penetrations on one edge.
    pub satin_density: f64,
    /// Pull compensation for satins and fills, mm.
    pub pull_compensation: f64,
    /// Fill row spacing, mm.
    pub row_spacing: f64,
    /// Fill stitch length, mm.
    pub stitch_length: f64,
    /// Fill underlay row spacing, mm.
    pub underlay_spacing: f64,
    /// Satin underlay: a centre walk only (firm fabrics) or edge contours and a zig-zag
    /// (stretchy and lofty ones).
    pub heavy_underlay: bool,
}

/// The fabric presets; the first is the default.
pub const FABRICS: [Fabric; 8] = [
    Fabric {
        id: "woven",
        label: "Woven (Cotton, Poplin)",
        satin_density: 0.40,
        pull_compensation: 0.20,
        row_spacing: 0.40,
        stitch_length: 3.0,
        underlay_spacing: 2.5,
        heavy_underlay: false,
    },
    Fabric {
        id: "knit",
        label: "Knit (T-Shirt, Jersey)",
        satin_density: 0.42,
        pull_compensation: 0.30,
        row_spacing: 0.42,
        stitch_length: 3.0,
        underlay_spacing: 2.0,
        heavy_underlay: true,
    },
    Fabric {
        id: "pique",
        label: "Piqué (Polo Shirt)",
        satin_density: 0.40,
        pull_compensation: 0.30,
        row_spacing: 0.40,
        stitch_length: 3.0,
        underlay_spacing: 2.0,
        heavy_underlay: true,
    },
    Fabric {
        id: "terry",
        label: "Terry (Towel)",
        satin_density: 0.36,
        pull_compensation: 0.35,
        row_spacing: 0.38,
        stitch_length: 3.0,
        underlay_spacing: 1.8,
        heavy_underlay: true,
    },
    Fabric {
        id: "fleece",
        label: "Fleece",
        satin_density: 0.38,
        pull_compensation: 0.35,
        row_spacing: 0.40,
        stitch_length: 3.0,
        underlay_spacing: 2.0,
        heavy_underlay: true,
    },
    Fabric {
        id: "denim",
        label: "Denim, Canvas",
        satin_density: 0.42,
        pull_compensation: 0.15,
        row_spacing: 0.42,
        stitch_length: 3.5,
        underlay_spacing: 3.0,
        heavy_underlay: false,
    },
    Fabric {
        id: "twill",
        label: "Twill (Cap)",
        satin_density: 0.40,
        pull_compensation: 0.25,
        row_spacing: 0.40,
        stitch_length: 3.0,
        underlay_spacing: 2.5,
        heavy_underlay: false,
    },
    Fabric {
        id: "leather",
        label: "Leather, Vinyl",
        satin_density: 0.50,
        pull_compensation: 0.10,
        row_spacing: 0.50,
        stitch_length: 3.5,
        underlay_spacing: 3.5,
        heavy_underlay: false,
    },
];

impl Fabric {
    /// A preset by id or label, ignoring case; `None` for an unknown one.
    pub fn find(s: &str) -> Option<&'static Fabric> {
        FABRICS.iter().find(|f| f.id.eq_ignore_ascii_case(s) || f.label.eq_ignore_ascii_case(s))
    }

    /// The default fabric (woven).
    pub fn default_fabric() -> &'static Fabric {
        &FABRICS[0]
    }

    /// Set the fabric-dependent values of `s` (satin density, pull compensation and underlay; fill
    /// row spacing, stitch length, pull compensation and underlay). Stitch type, widths, angles
    /// and run lengths stay as they are.
    pub fn apply(&self, s: &mut StitchSettings) {
        s.satin.density = self.satin_density;
        s.satin.pull_compensation = self.pull_compensation;
        s.satin.underlays = if self.heavy_underlay {
            vec![Underlay::Contour { length: 2.5, inset: 0.4 }, Underlay::ZigZag { spacing: 2.0, inset: 0.4 }]
        } else {
            vec![Underlay::CenterWalk { length: 2.5 }]
        };
        s.fill.row_spacing = self.row_spacing;
        s.fill.stitch_length = self.stitch_length;
        s.fill.pull_compensation = self.pull_compensation;
        s.fill.underlay = true;
        s.fill.underlay_spacing = self.underlay_spacing;
    }

    /// `kind`'s settings on this fabric.
    pub fn settings(&self, kind: crate::StitchType) -> StitchSettings {
        let mut s = StitchSettings::of(kind);
        self.apply(&mut s);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StitchType;

    #[test]
    fn woven_is_the_default_and_matches_the_plain_defaults() {
        let d = Fabric::default_fabric();
        assert_eq!(d.id, "woven");
        assert_eq!(d.settings(StitchType::Tatami), StitchSettings::of(StitchType::Tatami));
    }

    #[test]
    fn stretchy_fabrics_get_more_pull_and_underlay() {
        let knit = Fabric::find("Knit").unwrap().settings(StitchType::Satin);
        assert!(knit.satin.pull_compensation > 0.2);
        assert_eq!(knit.satin.underlays.len(), 2);
        assert_eq!(knit.kind, StitchType::Satin);
        assert!(Fabric::find("Leather, Vinyl").is_some());
        assert!(Fabric::find("silk").is_none());
        for f in FABRICS {
            assert_eq!(f.settings(StitchType::Tatami).sanitized(), f.settings(StitchType::Tatami), "{} is in range", f.id);
        }
    }
}
