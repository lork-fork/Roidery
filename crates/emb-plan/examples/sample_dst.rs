//! Writes a small sample design (a run, a satin column, Satin Blocks and a tatami square with a
//! hole, in two colours) to a DST file: `cargo run -p roidery-plan --example sample_dst -- out.dst`.

use roidery_formats::{Format, Thread};
use roidery_plan::{PlanObject, PlanSettings, plan};
use roidery_stitch::{Point, Polyline, Shape, StitchSettings, StitchType, generate};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "sample.dst".into());
    let ring = |x0: f64, y0: f64, x1: f64, y1: f64| {
        Polyline::new(vec![Point::new(x0, y0), Point::new(x1, y0), Point::new(x1, y1), Point::new(x0, y1)], true)
    };
    let red = Thread { rgb: [200, 30, 40], name: "Red".into(), ..Default::default() };
    let navy = Thread { rgb: [20, 30, 90], name: "Navy".into(), ..Default::default() };
    let objects = [
        (Shape::Area(vec![ring(0.0, 0.0, 40.0, 40.0), ring(15.0, 15.0, 25.0, 25.0)]), StitchType::Tatami, red.clone()),
        (Shape::Paths(vec![Polyline::new(vec![Point::new(0.0, 50.0), Point::new(40.0, 50.0)], false)]), StitchType::Satin, navy.clone()),
        (
            Shape::Rails {
                left: vec![Point::new(0.0, 60.0), Point::new(20.0, 58.0), Point::new(40.0, 60.0)],
                right: vec![Point::new(0.0, 64.0), Point::new(20.0, 66.0), Point::new(40.0, 64.0)],
            },
            StitchType::SatinBlocks,
            navy.clone(),
        ),
        (Shape::Paths(vec![ring(-5.0, -5.0, 45.0, 70.0)]), StitchType::Triple, navy),
    ];
    let objs: Vec<PlanObject> = objects
        .iter()
        .map(|(shape, kind, thread)| PlanObject {
            stitches: generate(shape, &StitchSettings::of(*kind)),
            thread: thread.clone(),
            ..Default::default()
        })
        .collect();
    let design = plan(&objs, &PlanSettings::default(), "Sample");
    std::fs::write(&path, Format::Dst.write(&design)?)?;
    println!("{path}: {} stitches, {} trims, {} colour changes", design.stitch_count(), design.trims(), design.color_changes());
    Ok(())
}
