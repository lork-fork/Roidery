//! Fabric presets ("auto fabric"): stitch settings tuned for common materials.

use serde::{Deserialize, Serialize};

use crate::model::{Design, ObjectKind};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fabric {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Satin spacing (mm).
    pub satin_spacing: f64,
    /// Fill row spacing (mm).
    pub fill_spacing: f64,
    /// Fill stitch length (mm).
    pub fill_stitch_length: f64,
    /// Pull compensation (mm).
    pub pull_comp: f64,
    /// Tatami underlay under fills.
    pub fill_underlay: bool,
    /// Underlay row spacing under fills (mm).
    pub underlay_spacing: f64,
    /// Recommend a water-soluble topping (pile fabrics).
    pub topping: bool,
    /// The stabiliser (backing) recommended under it.
    pub stabilizer: Stabilizer,
    /// Push compensation (mm): satin columns and fill rows stop short of the edges along them.
    pub push_comp: f64,
    /// The underlay under satin.
    pub satin_underlay: SatinUnderlayRecipe,
    /// An edge run under fills (round the shape, just inside it).
    pub fill_edge_run: bool,
    /// Sewn on clothing, worn and washed (`Some`): Check Design's long-stitch limit follows.
    pub garment: Option<bool>,
}

/// The underlay a fabric puts under satin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SatinUnderlayRecipe {
    /// Chosen from each column's width.
    #[default]
    Auto,
    CenterRun,
    EdgeRun,
    Zigzag,
    /// An edge run, then a zigzag over it: the most support, for knits and pile.
    EdgeZigzag,
}

impl SatinUnderlayRecipe {
    pub const ALL: [SatinUnderlayRecipe; 5] = [
        SatinUnderlayRecipe::Auto,
        SatinUnderlayRecipe::CenterRun,
        SatinUnderlayRecipe::EdgeRun,
        SatinUnderlayRecipe::Zigzag,
        SatinUnderlayRecipe::EdgeZigzag,
    ];

    pub fn id(self) -> &'static str {
        match self {
            SatinUnderlayRecipe::Auto => "auto",
            SatinUnderlayRecipe::CenterRun => "center-run",
            SatinUnderlayRecipe::EdgeRun => "edge-run",
            SatinUnderlayRecipe::Zigzag => "zigzag",
            SatinUnderlayRecipe::EdgeZigzag => "edge-zigzag",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.id() == id)
    }

    /// Sets `u` to this recipe (Auto leaves it to each column's width).
    pub fn apply(self, u: &mut crate::model::SatinUnderlay) {
        let (center, edge, zigzag) = match self {
            SatinUnderlayRecipe::Auto => {
                u.auto = true;
                return;
            }
            SatinUnderlayRecipe::CenterRun => (true, false, false),
            SatinUnderlayRecipe::EdgeRun => (false, true, false),
            SatinUnderlayRecipe::Zigzag => (false, false, true),
            SatinUnderlayRecipe::EdgeZigzag => (false, true, true),
        };
        u.auto = false;
        u.center_run = center;
        u.edge_run = edge;
        u.zigzag = zigzag;
    }
}

/// A stabiliser (backing) hooped under the fabric.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stabilizer {
    /// Torn away after sewing: stable wovens, caps, towels.
    #[default]
    TearAway,
    /// Cut away round the design, the rest left in: knits and anything that stretches.
    CutAway,
    /// A light cut-away that doesn't show through: thin knits and light colours.
    NoShowMesh,
    /// Dissolved in water: sheer fabrics, lace and freestanding work.
    WashAway,
}

impl Stabilizer {
    pub const ALL: [Stabilizer; 4] = [Stabilizer::TearAway, Stabilizer::CutAway, Stabilizer::NoShowMesh, Stabilizer::WashAway];

    pub fn id(self) -> &'static str {
        match self {
            Stabilizer::TearAway => "tear-away",
            Stabilizer::CutAway => "cut-away",
            Stabilizer::NoShowMesh => "no-show-mesh",
            Stabilizer::WashAway => "wash-away",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Stabilizer::TearAway => "Tear-away",
            Stabilizer::CutAway => "Cut-away",
            Stabilizer::NoShowMesh => "No-show mesh",
            Stabilizer::WashAway => "Wash-away",
        }
    }

    pub fn from_id(id: &str) -> Option<Stabilizer> {
        Stabilizer::ALL.into_iter().find(|s| s.id() == id)
    }
}

pub fn fabrics() -> Vec<Fabric> {
    vec![
        Fabric {
            name: "Woven (twill, poplin)".into(),
            description: "Stable woven fabric – standard settings.".into(),
            satin_spacing: 0.40,
            fill_spacing: 0.40,
            fill_stitch_length: 4.0,
            pull_comp: 0.15,
            fill_underlay: true,
            underlay_spacing: 2.5,
            topping: false,
            stabilizer: Stabilizer::TearAway,
            push_comp: 0.0,
            satin_underlay: SatinUnderlayRecipe::Auto,
            fill_edge_run: true,
            garment: None,
        },
        Fabric {
            name: "Pique knit (polo shirt)".into(),
            description: "Textured knit – more underlay and pull compensation.".into(),
            satin_spacing: 0.38,
            fill_spacing: 0.40,
            fill_stitch_length: 3.5,
            pull_comp: 0.30,
            fill_underlay: true,
            underlay_spacing: 2.0,
            topping: true,
            stabilizer: Stabilizer::CutAway,
            push_comp: 0.2,
            satin_underlay: SatinUnderlayRecipe::EdgeZigzag,
            fill_edge_run: true,
            garment: Some(true),
        },
        Fabric {
            name: "Jersey knit (T-shirt)".into(),
            description: "Stretchy knit – lighter density, strong underlay.".into(),
            satin_spacing: 0.45,
            fill_spacing: 0.45,
            fill_stitch_length: 3.5,
            pull_comp: 0.30,
            fill_underlay: true,
            underlay_spacing: 2.0,
            topping: false,
            stabilizer: Stabilizer::NoShowMesh,
            push_comp: 0.2,
            satin_underlay: SatinUnderlayRecipe::EdgeZigzag,
            fill_edge_run: true,
            garment: Some(true),
        },
        Fabric {
            name: "Fleece / sweatshirt".into(),
            description: "Thick pile – denser stitching to stay on top, use topping.".into(),
            satin_spacing: 0.35,
            fill_spacing: 0.38,
            fill_stitch_length: 4.0,
            pull_comp: 0.35,
            fill_underlay: true,
            underlay_spacing: 1.8,
            topping: true,
            stabilizer: Stabilizer::CutAway,
            push_comp: 0.25,
            satin_underlay: SatinUnderlayRecipe::EdgeZigzag,
            fill_edge_run: true,
            garment: Some(true),
        },
        Fabric {
            name: "Towel / terry".into(),
            description: "Loops need dense coverage and topping.".into(),
            satin_spacing: 0.33,
            fill_spacing: 0.36,
            fill_stitch_length: 3.5,
            pull_comp: 0.35,
            fill_underlay: true,
            underlay_spacing: 1.5,
            topping: true,
            stabilizer: Stabilizer::TearAway,
            push_comp: 0.25,
            satin_underlay: SatinUnderlayRecipe::EdgeZigzag,
            fill_edge_run: true,
            garment: Some(false),
        },
        Fabric {
            name: "Denim / canvas".into(),
            description: "Heavy stable fabric – can take open density.".into(),
            satin_spacing: 0.42,
            fill_spacing: 0.45,
            fill_stitch_length: 4.5,
            pull_comp: 0.10,
            fill_underlay: true,
            underlay_spacing: 3.0,
            topping: false,
            stabilizer: Stabilizer::TearAway,
            push_comp: 0.1,
            satin_underlay: SatinUnderlayRecipe::Auto,
            fill_edge_run: true,
            garment: None,
        },
        Fabric {
            name: "Leather / vinyl".into(),
            description: "Perforates easily – open density, longer stitches, little underlay.".into(),
            satin_spacing: 0.50,
            fill_spacing: 0.55,
            fill_stitch_length: 5.0,
            pull_comp: 0.05,
            fill_underlay: false,
            underlay_spacing: 3.5,
            topping: false,
            stabilizer: Stabilizer::TearAway,
            push_comp: 0.0,
            satin_underlay: SatinUnderlayRecipe::CenterRun,
            fill_edge_run: false,
            garment: Some(false),
        },
        Fabric {
            name: "Structured cap".into(),
            description: "Firm buckram front – moderate density, centre-out sequencing.".into(),
            satin_spacing: 0.40,
            fill_spacing: 0.42,
            fill_stitch_length: 3.5,
            pull_comp: 0.25,
            fill_underlay: true,
            underlay_spacing: 2.5,
            topping: false,
            stabilizer: Stabilizer::TearAway,
            push_comp: 0.15,
            satin_underlay: SatinUnderlayRecipe::EdgeRun,
            fill_edge_run: true,
            garment: None,
        },
        Fabric {
            name: "Silk / lightweight".into(),
            description: "Delicate fabric – light density to avoid puckering.".into(),
            satin_spacing: 0.48,
            fill_spacing: 0.50,
            fill_stitch_length: 3.5,
            pull_comp: 0.10,
            fill_underlay: false,
            underlay_spacing: 3.0,
            topping: false,
            stabilizer: Stabilizer::NoShowMesh,
            push_comp: 0.1,
            satin_underlay: SatinUnderlayRecipe::CenterRun,
            fill_edge_run: true,
            garment: Some(true),
        },
    ]
}

impl Default for Fabric {
    /// Stable woven fabric: the standard settings, unnamed.
    fn default() -> Self {
        Fabric { name: String::new(), description: String::new(), ..fabrics().swap_remove(0) }
    }
}

/// What to hoop under fabric `f`, in words: its stabiliser, and a topping when it wants one.
pub fn stabilizer_advice(f: &Fabric) -> String {
    let s = f.stabilizer.label();
    if f.topping { format!("{s}, with a water-soluble topping") } else { s.to_string() }
}

pub fn find(name: &str) -> Option<Fabric> {
    fabrics().into_iter().find(|f| f.name == name)
}

impl Fabric {
    /// Applies the preset's density, pull compensation and underlay settings to
    /// every object in the design and records the fabric name.
    pub fn apply(&self, design: &mut Design) {
        design.fabric = self.name.to_string();
        for obj in &mut design.objects {
            match &mut obj.kind {
                ObjectKind::Satin(s) => {
                    s.params.spacing = self.satin_spacing;
                    s.params.pull_comp = self.pull_comp;
                    s.params.push_comp = self.push_comp;
                    self.satin_underlay.apply(&mut s.params.underlay);
                }
                ObjectKind::Fill(f) => {
                    f.params.spacing = self.fill_spacing;
                    f.params.stitch_length = self.fill_stitch_length;
                    f.params.pull_comp = self.pull_comp;
                    f.params.push_comp = self.push_comp;
                    f.params.underlay.tatami = self.fill_underlay;
                    f.params.underlay.edge_run = self.fill_edge_run;
                    f.params.underlay.spacing = self.underlay_spacing;
                }
                ObjectKind::Lettering(l) => {
                    l.fill.spacing = self.satin_spacing;
                    l.fill.pull_comp = self.pull_comp;
                }
                ObjectKind::Applique(a) => {
                    a.params.cover_spacing = self.satin_spacing;
                    a.params.pull_comp = self.pull_comp;
                }
                ObjectKind::Run(_) | ObjectKind::Stitches(_) | ObjectKind::Redwork(_) | ObjectKind::Decor(_) => {}
            }
        }
    }
}
