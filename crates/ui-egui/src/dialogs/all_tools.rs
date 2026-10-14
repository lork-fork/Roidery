//! The All Tools drawer: every tool as a button; picking one selects it and closes the drawer.

use super::DialogSpec;
use crate::VectorcraftApp;
use crate::state::Dialog;

pub(super) const SPEC: DialogSpec = DialogSpec { heading: |_| tl!("All Tools").into(), body, ok: None, ..DialogSpec::FORM };

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, _: &mut Dialog) -> bool {
    let mut picked = false;
    for g in vectorcraft_tools::TOOL_GROUPS {
        ui.horizontal_wrapped(|ui| {
            for tool in g.iter() {
                let shown = tl!(tool.label);
                let shown = if crate::i18n::current() == crate::i18n::Lang::EN { shown.trim_end_matches(" Tool") } else { shown };
                if ui.button(shown).clicked() {
                    app.select_tool(tool.id);
                    picked = true;
                }
            }
        });
    }
    picked
}
