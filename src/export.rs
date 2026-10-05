//! Files out of a build — **what is shown is what is written.**
//!
//! A build's model holds more live solids than it draws: a copy made before a consumption, a value
//! no later step reads but the display set leaves out. Writing the model whole would put those in
//! the file (in 61 of this crate's 140 test builds the model held more solids than were drawn), so
//! [`BuildOutput::rendered_bodies`] names the bodies once, and every format writes those:
//! [`BuildOutput::export_step`] here, and an OBJ through `nacre::tess::Tessellation::to_obj_solids`
//! from whoever holds the mesh.
//!
//! "STEP" in this module is the file format (ISO 10303), never a program [`Step`](crate::Step).

use crate::build::BuildOutput;
use nacre::ops::RefineReport;
use nacre::step::StepError;
use nacre::store::Handle;
use nacre::topo::Solid;

/// A STEP file and what the export door did to the model's caches on the way.
#[derive(Debug)]
pub struct StepExport {
    /// The file's text.
    pub text: String,
    /// What `nacre::ops::refine_caches` raised before writing, and what it left — a value left
    /// undecided or unrealized is written at the figure its construction gave.
    pub refine: RefineReport,
}

impl BuildOutput {
    /// **The bodies the build shows** — every body of each solid value in [`BuildOutput::rendered`],
    /// in that order (sketches and planes have none). This is the one answer to "what does an
    /// export write"; each format reads it.
    pub fn rendered_bodies(&self) -> Vec<Handle<Solid>> {
        self.rendered
            .iter()
            .filter_map(|id| self.values.get(id.0 as usize)?.as_ref()?.as_solid())
            .flat_map(|v| v.bodies.iter().copied())
            .collect()
    }

    /// **The shown solids as a STEP (ISO 10303, AP242) file** — [`BuildOutput::rendered_bodies`],
    /// one part each, with `timestamp` written verbatim into the header (the kernel reads no
    /// clock; `""` leaves the field blank).
    ///
    /// First pays `nacre::ops::refine_caches`, the kernel's door before an export: a history longer
    /// than the caches pay for at build time leaves construction figures behind, and the door
    /// raises them to the nearest `f64` of the exact geometry. It takes the model mutably, and its
    /// own rule is to call it on a copy when the model will be edited further — a build's output is
    /// the end of its log, so nothing is built on it afterwards. A raised cache is a closer `f64`
    /// of the same truth; the queries that read caches answer with it from then on.
    pub fn export_step(&mut self, timestamp: &str) -> Result<StepExport, StepError> {
        let refine = nacre::ops::refine_caches(&mut self.model);
        let text = nacre::step::to_step_solids(&self.model, &self.rendered_bodies(), timestamp)?;
        Ok(StepExport { text, refine })
    }
}
