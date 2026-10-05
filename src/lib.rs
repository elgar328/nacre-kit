//! # nacre-kit
//!
//! The convenience layer over the [`nacre`] kernel — what a code-CAD script actually
//! speaks. The kernel is exact and minimal (two-solid booleans, consuming semantics,
//! no sketch sugar); this crate provides the model a script wants — **values that can
//! be freely reused, n-ary booleans, multi-body solids** — and translates it into
//! kernel operations without ever making a geometric judgment of its own.
//!
//! The boundary rule (from the kernel's `docs/overview.md`):
//!
//! > The convenience layer only *composes* kernel ops. The moment something needs a
//! > new exact predicate decision (in/out, coincidence, direction), it belongs to the
//! > kernel.
//!
//! The public surface is deliberately small:
//! - [`Step`] — one recorded script action; a program is `Vec<Step>`. This is the
//!   log a script's front end serializes, and what [`build`] replays.
//! - [`build`] — translate the steps into kernel operations, copying a value before
//!   each consumption so it stays reusable.
//! - [`BuildOutput`] — the kernel model, the value table, and what should be rendered.
//! - [`KitError`] — kernel rejections translated to where-and-why, with pair-level
//!   blame for n-ary booleans.
//! - [`BuildOutput::export_step`] and [`BuildOutput::rendered_bodies`] — files out of a build:
//!   what is shown is what is written.

mod build;
mod error;
mod export;
mod query;
mod render;
mod sketch;
mod step;
mod value;

pub use build::{BuildOutput, StepReport, build};
pub use error::{Blame, KitError, Mark, RejectClass};
pub use export::StepExport;
pub use query::{FaceInfo, VertexInfo};
pub use step::{
    Anchor, CircleSize, Corner, CylAnchor, Dist, EdgeStyle, Edges, FaceRef, KitAxis, KitBool,
    MirrorPlane, Path, PenPath, Pivot, PlaneRef, PlaneSpec, SketchSeg, Step, Style, ValueId,
    VertexRef, WorldPlane,
};
pub use value::{PlaneSrc, PlaneValue, SketchValue, SolidValue, Value};
