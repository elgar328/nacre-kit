//! What should be drawn — a **script-layer** notion, not the kernel's live set
//! (after `translate(base, …)` the value `base` is still usable, but drawing everything
//! live would show the original and the moved one on top of each other).
//!
//! **Default: every value no later step made something out of.** Handing a value to a
//! step that builds from it hands the screen over to what was built — that is what
//! stops a chain of transforms from leaving a ghost at every stage.
//!
//! `Copy` is the exception, and stating it is what makes the rule one sentence:
//! a copy makes a *second* thing rather than a derived one, so the original stays.
//! Reuse copies by itself here, so writing `.copy()` says one thing only: *I want a
//! second one*.
//!
//! **Sketches obey the same rule** — they are not excluded by kind. A sketch you
//! have drawn and not yet used is a result worth seeing; extruding it hands the screen
//! to the solid. Note that "consumed" would be the wrong test for them: an extrude
//! does not consume its sketch (sketches are pure data, reused freely), so a sketch
//! would linger forever after being used. "Made something out of it" is the question,
//! and reading is how a sketch gets used.
//!
//! An explicit [`Step::Display`] anywhere turns the default off and the display
//! targets are the whole set.

use crate::step::{Step, ValueId};
use crate::value::Value;

pub(crate) fn render_set(steps: &[Step], values: &[Option<Value>]) -> Vec<ValueId> {
    let displayed: Vec<ValueId> = steps
        .iter()
        .filter_map(|s| match s {
            Step::Display { targets, .. } => Some(targets.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    if !displayed.is_empty() {
        let mut seen = std::collections::HashSet::new();
        return displayed.into_iter().filter(|v| seen.insert(*v)).collect();
    }
    let mut used = vec![false; steps.len()];
    for s in steps {
        if matches!(s, Step::Copy { .. }) {
            continue; // a copy builds nothing out of its source
        }
        for id in s.reads() {
            used[id.0 as usize] = true;
        }
    }
    // Solids and sketches; a plane has nothing to draw.
    (0..steps.len())
        .filter(|&i| {
            let drawable = matches!(&values[i], Some(Value::Solid(_)) | Some(Value::Sketch(_)));
            drawable && !used[i]
        })
        .map(|i| ValueId(i as u32))
        .collect()
}
