//! Machine profiles: the file format a machine reads and the design settings that suit it, as a
//! starting point for a design (longest stitch and frame move, speed and trim time for time
//! estimates, and optionally the shortest stitch, trims and ties it wants).
//!
//! The standard profiles, one per format, ship with the app and are named by their format
//! (`"dst"`); their values are deliberately generic. The user can change them (the changes are
//! kept apart, so a standard profile can go back to how it ships) and make profiles of their own
//! for their machines, each writing one of the formats.

use crate::model::{DesignSettings, TieStyle};
use serde::{Deserialize, Serialize};

/// Commercial (multi-needle) or home machines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MachineKind {
    Commercial,
    #[default]
    Home,
}

impl MachineKind {
    pub fn label(self) -> &'static str {
        match self {
            MachineKind::Commercial => "commercial machines",
            MachineKind::Home => "home machines",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            MachineKind::Commercial => "commercial",
            MachineKind::Home => "home",
        }
    }
}

/// The design settings a profile can carry besides its stitch limits, speed and trim time: the
/// shortest stitch, when connectors are trimmed, the ties and the colour change time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MachineAdvanced {
    /// Stitches shorter than this are merged away (mm).
    pub min_stitch: f64,
    /// Connectors longer than this (mm) are trimmed instead of jumped.
    pub trim_threshold: f64,
    pub tie_in: bool,
    pub tie_off: bool,
    /// Size of tie-in and tie-off stitches (mm).
    pub tie_length: f64,
    pub tie_style: TieStyle,
    /// Seconds per colour change, for time estimates.
    pub color_change_seconds: f64,
}

impl Default for MachineAdvanced {
    fn default() -> Self {
        Self::of(&DesignSettings::default())
    }
}

impl MachineAdvanced {
    /// The advanced values of the design settings `s`.
    pub fn of(s: &DesignSettings) -> Self {
        MachineAdvanced {
            min_stitch: s.min_stitch,
            trim_threshold: s.trim_threshold,
            tie_in: s.tie_in,
            tie_off: s.tie_off,
            tie_length: s.tie_length,
            tie_style: s.tie_style,
            color_change_seconds: s.color_change_seconds,
        }
    }

    fn apply(&self, s: &mut DesignSettings) {
        s.min_stitch = self.min_stitch;
        s.trim_threshold = self.trim_threshold;
        s.tie_in = self.tie_in;
        s.tie_off = self.tie_off;
        s.tie_length = self.tie_length;
        s.tie_style = self.tie_style;
        s.color_change_seconds = self.color_change_seconds;
    }
}

/// A machine, or a family of machines, the file format it reads and settings that suit it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MachineProfile {
    /// A standard profile's format (`"pes"`); the user's name for one of their own.
    pub name: String,
    /// File extension of the format the machine reads (`"dst"`, `"pes"`, ...).
    pub format: String,
    pub kind: MachineKind,
    /// Longest stitch (mm); longer ones are split.
    pub max_stitch: f64,
    /// Longest single frame move (mm); longer jumps are split.
    pub max_jump: f64,
    /// Typical sewing speed (stitches per minute), for time estimates.
    pub speed: f64,
    /// Seconds a trim takes, for time estimates.
    pub trim_seconds: f64,
    /// The other design settings the machine wants; none keeps the design's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advanced: Option<MachineAdvanced>,
}

impl Default for MachineProfile {
    fn default() -> Self {
        standard_row(STANDARD[0])
    }
}

use MachineKind::{Commercial, Home};

/// The standard profiles: format, kind, longest stitch and frame move (mm) and speed (stitches
/// per minute).
const STANDARD: [(&str, MachineKind, f64, f64); 7] = [
    // DST moves at most 12.1 mm per record
    ("dst", Commercial, 12.1, 1000.0),
    ("exp", Commercial, 12.0, 1000.0),
    ("u01", Commercial, 12.0, 1000.0),
    ("pes", Home, 12.0, 650.0),
    ("jef", Home, 12.0, 800.0),
    ("vp3", Home, 12.0, 800.0),
    ("xxx", Home, 12.0, 650.0),
];

fn standard_row((format, kind, max, speed): (&str, MachineKind, f64, f64)) -> MachineProfile {
    let trim_seconds = match kind {
        Commercial => 4.0,
        Home => 6.0,
    };
    MachineProfile { name: format.to_string(), format: format.to_string(), kind, max_stitch: max, max_jump: max, speed, trim_seconds, advanced: None }
}

/// The standard profiles as they ship, one per format.
pub fn standard_profiles() -> Vec<MachineProfile> {
    STANDARD.into_iter().map(standard_row).collect()
}

/// The standard profile for machines reading `format` (`"pes"`, `"PES"`), as it ships.
pub fn standard(format: &str) -> Option<MachineProfile> {
    STANDARD.into_iter().find(|r| r.0.eq_ignore_ascii_case(format.trim())).map(standard_row)
}

/// The formats a machine profile can write, by extension.
pub fn formats() -> impl Iterator<Item = &'static str> {
    STANDARD.iter().map(|r| r.0)
}

impl MachineProfile {
    /// Whether this is one of the standard profiles (they are named by their format).
    pub fn is_standard(&self) -> bool {
        standard(&self.name).is_some()
    }

    /// "PES: home machines" for a standard profile; the name of one of the user's.
    pub fn label(&self) -> String {
        if self.is_standard() { format!("{}: {}", self.format.to_uppercase(), self.kind.label()) } else { self.name.clone() }
    }

    /// The design settings with this machine's values.
    pub fn apply(&self, s: &DesignSettings) -> DesignSettings {
        let mut out = DesignSettings {
            max_stitch: self.max_stitch,
            max_jump: self.max_jump,
            machine_speed: self.speed,
            trim_seconds: self.trim_seconds,
            ..s.clone()
        };
        if let Some(a) = &self.advanced {
            a.apply(&mut out);
        }
        out
    }

    /// This profile with the machine values of the design settings `s`, advanced ones too (its
    /// name, format and kind stay).
    pub fn with_settings(&self, s: &DesignSettings) -> MachineProfile {
        MachineProfile {
            max_stitch: s.max_stitch,
            max_jump: s.max_jump,
            speed: s.machine_speed,
            trim_seconds: s.trim_seconds,
            advanced: Some(MachineAdvanced::of(s)),
            ..self.clone()
        }
    }
}

/// The saved copy of the standard profile `std`, kept to its name, format and kind.
fn saved_standard(saved: &[MachineProfile], std: &MachineProfile) -> Option<MachineProfile> {
    saved.iter().find(|m| m.name.eq_ignore_ascii_case(&std.name)).map(|m| MachineProfile {
        name: std.name.clone(),
        format: std.format.clone(),
        kind: std.kind,
        ..m.clone()
    })
}

/// Every profile: the standard ones (as `saved` changes them) in format order, then the user's
/// own. `saved` is what the user keeps: changed standard profiles and their own ones; a profile
/// of their own without a name or a format the app writes is left out.
pub fn profiles(saved: &[MachineProfile]) -> Vec<MachineProfile> {
    let mut v: Vec<MachineProfile> = standard_profiles().into_iter().map(|m| saved_standard(saved, &m).unwrap_or(m)).collect();
    v.extend(saved.iter().filter(|m| !m.name.trim().is_empty() && !m.is_standard() && standard(&m.format).is_some()).map(|m| {
        let mut m = m.clone();
        m.format.make_ascii_lowercase();
        m
    }));
    v
}

/// The profile named `name` (any case), standard or the user's.
pub fn find(saved: &[MachineProfile], name: &str) -> Option<MachineProfile> {
    profiles(saved).into_iter().find(|m| m.name.eq_ignore_ascii_case(name.trim()))
}

/// Whether the standard profile `name` differs from how it ships.
pub fn is_edited(saved: &[MachineProfile], name: &str) -> bool {
    match (standard(name), find(saved, name)) {
        (Some(std), Some(cur)) => std != cur,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_set_the_machine_values_and_keep_the_rest() {
        let pes = standard("PES").expect("pes");
        assert_eq!(pes.label(), "PES: home machines");
        assert!(pes.is_standard());
        let base = DesignSettings { tie_length: 1.1, ..DesignSettings::default() };
        let s = pes.apply(&base);
        assert_eq!((s.max_stitch, s.machine_speed, s.tie_length), (12.0, 650.0, 1.1));
        assert!(standard("abc").is_none());
        // one profile per format
        let all = standard_profiles();
        for (i, m) in all.iter().enumerate() {
            assert!(all.iter().skip(i + 1).all(|o| o.format != m.format));
        }
        assert_eq!(formats().count(), all.len());
        assert_eq!(profiles(&[]), all);
    }

    #[test]
    fn edited_and_custom_profiles() {
        let design = DesignSettings { machine_speed: 700.0, tie_length: 1.4, ..DesignSettings::default() };
        let dst = standard("dst").expect("dst").with_settings(&design);
        let diamond = MachineProfile { name: "Diamond deLuxe".into(), ..standard("vp3").expect("vp3") };
        let saved = vec![dst.clone(), diamond.clone()];
        let all = profiles(&saved);
        assert_eq!(all.len(), 8, "seven standard, then the user's");
        assert_eq!(all[0], dst);
        assert_eq!(all[7].label(), "Diamond deLuxe");
        assert!(!all[7].is_standard());
        assert_eq!(find(&saved, "diamond DELUXE").map(|m| m.format), Some("vp3".to_string()));
        assert!(is_edited(&saved, "DST") && !is_edited(&saved, "pes") && !is_edited(&saved, "Diamond deLuxe"));
        // an edited standard profile with advanced settings sets them too
        let s = find(&saved, "dst").expect("dst").apply(&DesignSettings::default());
        assert_eq!((s.machine_speed, s.tie_length), (700.0, 1.4));
        // a saved copy keeps the standard profile's format and kind, whatever it says
        let odd = MachineProfile { name: "DST".into(), format: "pes".into(), kind: Home, speed: 900.0, ..standard("dst").expect("dst") };
        let m = find(&[odd], "dst").expect("dst");
        assert_eq!((m.name.as_str(), m.format.as_str(), m.kind, m.speed), ("dst", "dst", Commercial, 900.0));
        assert!(!is_edited(&[MachineProfile { name: "DST".into(), ..standard("dst").expect("dst") }], "dst"), "the same values in another case");
        // one of the user's without a format the app writes is left out
        assert_eq!(profiles(&[MachineProfile { format: "emb".into(), ..diamond }]).len(), 7);
    }

    #[test]
    fn profiles_read_back_from_their_saved_form() {
        let m = standard("jef").expect("jef").with_settings(&DesignSettings { tie_style: TieStyle::Triangle, ..DesignSettings::default() });
        let json = serde_json::to_value(&m).expect("json");
        assert_eq!(json["maxStitch"], 12.0);
        assert_eq!(json["advanced"]["tieStyle"], "triangle");
        assert_eq!(serde_json::from_value::<MachineProfile>(json).expect("back"), m);
        // a profile saved before advanced settings existed reads without them
        let old: MachineProfile = serde_json::from_str(r#"{"name": "Shop", "format": "dst", "kind": "commercial", "speed": 900}"#).expect("old");
        assert_eq!((old.speed, old.max_stitch, old.advanced), (900.0, 12.1, None));
    }
}

/// A home embroidery machine someone can pick as theirs (My Machine): the file format it reads
/// and the hoops it takes, the everyday ones first (names from [`crate::hoops::standard_hoops`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HomeMachine {
    pub name: &'static str,
    /// The machine's own format, the default for saving.
    pub format: &'static str,
    /// Every format the machine reads, the native one first.
    pub reads: &'static [&'static str],
    pub hoops: &'static [&'static str],
}

/// The Designer Diamond deLuxe's hoops: the three it comes with first (Crown 260 × 200, Royal
/// 360 × 200 and Splendid Square 120 × 120, as Husqvarna Viking's feature sheet lists them), then
/// the optional ones it takes.
const DIAMOND_HOOPS: &[&str] = &[
    "Husqvarna 260 × 200",
    "Husqvarna 360 × 200",
    "Husqvarna 120 × 120",
    "Husqvarna 240 × 150",
    "Husqvarna 360 × 350",
    "Husqvarna Quilter's 200 × 200",
    "Husqvarna Mega Endless 260 × 150",
    "Husqvarna Small Square 80 × 80",
    "Husqvarna 100 × 100",
];
const HUSQVARNA_HOOPS: &[&str] = &[
    "Husqvarna 240 × 150",
    "Husqvarna 260 × 200",
    "Husqvarna 120 × 120",
    "Husqvarna 360 × 200",
    "Husqvarna 360 × 350",
    "Husqvarna Imperial 360 × 260",
    "Husqvarna Quilter's 200 × 200",
    "Husqvarna Mega Endless 260 × 150",
    "Husqvarna Small Square 80 × 80",
    "Husqvarna 100 × 100",
];
const PFAFF_HOOPS: &[&str] = &[
    "Pfaff creative Grand Metal 240 × 150",
    "Pfaff creative 120 × 115",
    "Pfaff creative Elite 260 × 200",
    "Pfaff creative Deluxe 360 × 200",
    "Pfaff creative Grand Dream 360 × 350",
    "Pfaff creative Quilters 200 × 200",
    "Pfaff creative Grand Quilters 260 × 260",
    "Pfaff creative Grand Endless 260 × 150",
    "Pfaff creative Petite Square 80 × 80",
];
const BROTHER_HOOPS: &[&str] = &[
    "Brother 130 × 180 (5×7)",
    "Brother 100 × 100 (4×4)",
    "Brother 160 × 260 (6×10)",
    "Brother 200 × 200 (8×8)",
    "Brother 200 × 300 (8×12)",
    "Brother 240 × 240 (9.5×9.5)",
    "Brother 240 × 360 (9.5×14)",
];
const BROTHER_MULTI_HOOPS: &[&str] = &[
    "Brother 200 × 300 (8×12)",
    "Brother 130 × 180 (5×7)",
    "Brother 100 × 100 (4×4)",
    "Brother 200 × 200 (8×8)",
    "Cap frame 130 × 60",
    "Wide cap frame 160 × 60",
    "Sleeve 80 × 300",
];
const COMMERCIAL_HOOPS: &[&str] = &[
    "Tajima Round 15 cm",
    "Tajima Round 12 cm",
    "Tajima Round 18 cm",
    "Tajima Round 21 cm",
    "Tajima Round 24 cm",
    "Tajima Square 30 × 30 cm",
    "Cap frame 130 × 60",
    "Wide cap frame 160 × 60",
    "Jacket back 360 × 360",
    "Sleeve 80 × 300",
    "Magnetic frame 130 × 130",
    "Magnetic pocket frame 100 × 70",
];

/// The machines My Machine offers.
pub const HOME_MACHINES: &[HomeMachine] = &[
    HomeMachine { name: "Husqvarna Viking Designer Diamond deLuxe", format: "vp3", reads: &["vp3", "dst"], hoops: DIAMOND_HOOPS },
    HomeMachine { name: "Husqvarna Viking (other models)", format: "vp3", reads: &["vp3", "dst"], hoops: HUSQVARNA_HOOPS },
    HomeMachine { name: "Pfaff creative", format: "vp3", reads: &["vp3", "dst"], hoops: PFAFF_HOOPS },
    HomeMachine { name: "Brother or Baby Lock", format: "pes", reads: &["pes", "dst"], hoops: BROTHER_HOOPS },
    HomeMachine { name: "Brother or Baby Lock multi-needle", format: "pes", reads: &["pes", "dst"], hoops: BROTHER_MULTI_HOOPS },
    HomeMachine {
        name: "Janome or Elna",
        format: "jef",
        reads: &["jef", "dst"],
        hoops: &[
            "Janome RE20b 140 × 200",
            "Janome SQ14 140 × 140",
            "Janome SQ20b 200 × 200",
            "Janome RE28b 200 × 280",
            "Janome SQ23 230 × 230",
            "Janome GR 230 × 300",
            "Janome RE36b 200 × 360",
        ],
    },
    HomeMachine {
        name: "Bernina",
        format: "exp",
        reads: &["exp", "dst"],
        hoops: &[
            "Bernina Medium 130 × 100",
            "Bernina Large Oval 145 × 255",
            "Bernina Midi 165 × 265",
            "Bernina Maxi 210 × 400",
            "Bernina Jumbo 260 × 400",
            "Bernina Mega 150 × 400",
        ],
    },
    HomeMachine { name: "Singer", format: "xxx", reads: &["xxx", "dst"], hoops: BROTHER_HOOPS },
    HomeMachine { name: "Commercial multi-needle (DST)", format: "dst", reads: &["dst"], hoops: COMMERCIAL_HOOPS },
    HomeMachine {
        name: "Another machine (DST)",
        format: "dst",
        reads: &["dst"],
        hoops: &["Magnetic frame 130 × 180", "Magnetic frame 100 × 100", "Magnetic frame 200 × 200", "Magnetic frame 200 × 300"],
    },
];

/// A machine of [`HOME_MACHINES`] by name (any case).
pub fn home_machine(name: &str) -> Option<&'static HomeMachine> {
    HOME_MACHINES.iter().find(|m| m.name.eq_ignore_ascii_case(name.trim()))
}

#[cfg(test)]
mod home_machine_tests {
    use super::*;

    /// Every home machine's hoops are built-in hoops, its format a standard profile's.
    #[test]
    fn home_machines_name_real_hoops_and_formats() {
        let hoops = crate::hoops::standard_hoops();
        let formats: Vec<String> = standard_profiles().into_iter().map(|p| p.format).collect();
        for m in HOME_MACHINES {
            assert!(!m.hoops.is_empty(), "{}", m.name);
            for h in m.hoops {
                assert!(hoops.iter().any(|x| x.name == *h), "{}: {h}", m.name);
            }
            assert!(formats.iter().any(|f| f == m.format), "{}: {}", m.name, m.format);
        }
        for m in HOME_MACHINES {
            assert_eq!(m.reads.first(), Some(&m.format), "{}: native first", m.name);
            assert!(m.reads.iter().all(|f| formats.iter().any(|x| x == f)), "{}", m.name);
        }
        assert_eq!(home_machine("husqvarna viking designer diamond deluxe").map(|m| m.format), Some("vp3"));
    }
}
