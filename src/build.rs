//! Record-then-build: translate a `Vec<Step>` into kernel operations, one pass,
//! **prefix-stable**.
//!
//! **Prefix-stability is the ground the resolved references stand on**: a script's
//! selectors query a build of the steps recorded *so far* and write model indices into
//! later steps, so step `k`'s kernel operations must
//! be a function of steps `0..k` alone — then the prefix build's arena is literally a
//! prefix of the full build's, and every index a query reported still names the same
//! cell. That rules out a lookahead scan ("copy only before a non-final consumption"):
//! whether a consumption is final is future knowledge, and acting on it would move copy
//! insertions — and every later cell index — whenever a script line is added.
//! The only prefix-stable scheme is the eager one: **every consumption copies first**
//! (a copy must precede its consumption — kernel liveness — and "last use" cannot be
//! known), the copy becomes the value's new binding, and the consumer takes the
//! previous one. Reuse is the script's freedom; the copy per consumption is its
//! flat, future-blind price.
//!
//! Intra-step reuse (one tool cutting many pieces, the pairwise products of `common`)
//! inserts its own copies under the same before-consumption rule.
//!
//! **The fuse fold's disjointness bookkeeping is identity-guarded.** `Fuse(A, B)`
//! answers the question the kit must not judge itself: one result solid means the two
//! merged, two means the kernel *proved* them disjoint. But the two result handles of a
//! disjoint fuse are indistinguishable (supersede mints fresh handles, and the kit may
//! not tell them apart by geometry — boundary rule), and letting unknown identities
//! carry disjointness marks is unsound: a mark can end up attached to the wrong body,
//! and a piece can reach the final list untested. So before each test the inputs are
//! **guard-copied**; on a disjoint verdict the copies (whose identities are exact)
//! carry on and the fused outputs are abandoned. The abandoned solids stay live in the
//! kernel arena — bounded (two per fuse test), inert, and honestly documented; a
//! kernel-side "which side did each result come from" report would remove them and is
//! recorded as the clean future.

use std::collections::HashSet;

use crate::error::{Blame, KitError, Mark};
use crate::render::render_set;
use crate::sketch::classify;
use crate::step::{
    Anchor, CylAnchor, Dist, FaceRef, KitAxis, KitBool, Pivot, PlaneRef, PlaneSpec, Step, ValueId,
    WorldPlane,
};
use crate::value::{PlaneSrc, PlaneValue, SketchValue, SolidValue, Value};
use nacre::exact::{Angle, Axis, Isometry, Rat, Rotation};
use nacre::math::{Point2, Point3, Vector3};
use nacre::ops::{
    BoolKind, DatumDef, OpError, OpOutput, Operation, Profile2d, SketchFrame, SketchPlane, apply,
};
use nacre::store::Handle;
use nacre::topo::Surface;
use nacre::topo::{Model, Solid};

/// What a build hands back.
pub struct BuildOutput {
    pub model: Model,
    /// Per step index — `None` for steps that produce no value ([`Step::Display`]).
    pub values: Vec<Option<Value>>,
    /// What the app should draw: DAG leaves, or the explicit `Display` set if any
    /// display step exists (explicit turns automatic off).
    pub rendered: Vec<ValueId>,
    /// Step indices at which an automatic copy was inserted before a consumption —
    /// inspector information ("a copy was inserted here"), never an error.
    pub auto_copies: Vec<usize>,
    /// Per step index — what the step's kernel booleans assumed (`None` for steps that
    /// ran no boolean). A fold makes several kernel calls, so this is the **step-level
    /// aggregate**: merge/coincidence counts summed, every closest call kept.
    pub reports: Vec<Option<StepReport>>,
}

/// The kernel's `BoolReport`, translated to the script's grain — the transparency
/// surface: not a failure channel, a normal-path answer to
/// "what did this operation assume".
#[derive(Debug, Default, Clone)]
pub struct StepReport {
    /// Face merges decided on toleranced evidence (axis-aligned models: 0 — nothing
    /// was assumed).
    pub merges: usize,
    /// Judgements answered by a proved coincidence rather than a proved sign.
    pub coincidences: usize,
    /// Each kernel call's closest call, rendered — empty when nothing was assumed.
    pub closest_calls: Vec<String>,
}

/// Build the first `upto` steps (all of them when `None`). Deterministic: the same
/// slice builds the same model, bit for bit — the kit adds no nondeterminism on top of
/// kernel replay (fold pair order is index-ascending).
pub fn build(steps: &[Step], upto: Option<usize>) -> Result<BuildOutput, KitError> {
    let n = upto.map_or(steps.len(), |u| u.min(steps.len()));
    let steps = &steps[..n];

    // References must point backward, at value-producing steps.
    for (i, s) in steps.iter().enumerate() {
        for id in s.reads() {
            if id.0 as usize >= i {
                return Err(KitError::Program {
                    step: i,
                    what: format!("value {} is not defined before this step", id.0),
                });
            }
            if !steps[id.0 as usize].produces_value() {
                return Err(KitError::Program {
                    step: i,
                    what: format!("value {} refers to a step that produces no solid", id.0),
                });
            }
        }
    }

    let mut b = Builder {
        model: Model::new(),
        values: Vec::with_capacity(n),
        generations: Vec::with_capacity(n),
        auto_copies: Vec::new(),
        reports: Vec::with_capacity(n),
    };
    for (i, s) in steps.iter().enumerate() {
        debug_assert_eq!(b.reports.len(), i);
        let v = b.exec(i, s)?;
        b.generations.push(match &v {
            Some(Value::Solid(sv)) => Some(vec![sv.bodies.clone()]),
            _ => None,
        });
        b.values.push(v);
        if b.reports.len() == i {
            b.reports.push(None); // the step ran no boolean
        }
    }

    let rendered = render_set(steps, &b.values);
    Ok(BuildOutput {
        model: b.model,
        values: b.values,
        rendered,
        auto_copies: b.auto_copies,
        reports: b.reports,
    })
}

struct Builder {
    model: Model,
    values: Vec<Option<Value>>,
    /// Per step — every binding **generation** a solid value has had: the produced
    /// bodies first, then one entry per rebinding [`Builder::take`] performs. A
    /// resolved reference may date from any generation (a query runs against whatever
    /// binding its prefix had built), each generation is a structural clone of the
    /// previous, and prefix-stability makes the full build's list a superset of any
    /// prefix's — so a positional lookup over this list finds every reference a query
    /// could have reported.
    generations: Vec<Option<Vec<Vec<Handle<Solid>>>>>,
    auto_copies: Vec<usize>,
    reports: Vec<Option<StepReport>>,
}

/// One body with a stable local identity and a human origin label — the fuse fold's
/// bookkeeping unit.
struct Piece {
    id: u32,
    h: Handle<Solid>,
    origin: ValueId,
    label: String,
}

impl Builder {
    // ---- kernel doors -----------------------------------------------------------

    /// `apply`, with kernel errors translated. `SolidNotLive` is *never* a user error
    /// — the value layer exists to make it unreachable — so it comes back as
    /// [`KitError::Internal`].
    fn kapply(
        &mut self,
        step: usize,
        op: &Operation,
        blame: Option<Blame>,
    ) -> Result<OpOutput, KitError> {
        apply(&mut self.model, op).map_err(|e| match e {
            OpError::SolidNotLive => KitError::Internal {
                step,
                what: "a consumed solid was used again (the automatic-copy scan missed it)"
                    .to_string(),
            },
            // A boolean rejection carries the kernel's classification; an operation-level
            // error still carries none — that taxonomy is a later step, and it is a
            // separate question from the words below (`PocketNotBlind` is the operation's
            // premise breaking, which none of the three classes describes).
            OpError::Boolean(nacre::ops::BoolError::Rejected { reason, at }) => KitError::Kernel {
                step,
                what: reason.to_string(),
                class: Some(reason.class()),
                blame,
                mark: at.map(Mark::from),
            },
            // Everything else is an operation-level refusal. `op_words` decides what an
            // author is told — a sentence, never the bare Rust variant name (a reader
            // meeting `PocketNotBlind` learns nothing). The words live in that table, so no
            // variant needs an arm of its own here just to carry them.
            other => KitError::Kernel {
                step,
                what: crate::error::op_words(&other),
                class: None,
                blame,
                mark: None,
            },
        })
    }

    fn kcopy(&mut self, step: usize, h: Handle<Solid>) -> Result<Handle<Solid>, KitError> {
        match self.kapply(step, &Operation::Copy { solid: h }, None)? {
            OpOutput::Copy { solid } => Ok(solid),
            other => Err(KitError::Internal {
                step,
                what: format!("copy answered {other:?}"),
            }),
        }
    }

    /// One kernel boolean, through the reporting entry (`boolean` is its thin
    /// wrapper), with the call's report folded into the step's aggregate.
    fn kbool(
        &mut self,
        step: usize,
        kind: BoolKind,
        a: Handle<Solid>,
        b: Handle<Solid>,
        blame: Blame,
    ) -> Result<Vec<Handle<Solid>>, KitError> {
        let (solids, report) = nacre::ops::boolean_with_report(&mut self.model, kind, a, b)
            .map_err(|e| match e {
                // Liveness is the kit's own invariant, never the user's problem —
                // the boolean spells it `InputNotLive`.
                nacre::ops::BoolError::InputNotLive => KitError::Internal {
                    step,
                    what: "a consumed solid was used again (the automatic-copy scan missed it)"
                        .to_string(),
                },
                nacre::ops::BoolError::Rejected { reason, at } => KitError::Kernel {
                    step,
                    what: reason.to_string(),
                    class: Some(reason.class()),
                    blame: Some(blame),
                    mark: at.map(Mark::from),
                },
            })?;
        while self.reports.len() < step {
            self.reports.push(None);
        }
        if self.reports.len() == step {
            self.reports.push(Some(StepReport::default()));
        }
        let agg = self.reports[step].get_or_insert_with(StepReport::default);
        agg.merges += report.merges.len();
        agg.coincidences += report.coincidences;
        if let Some(l) = &report.loosest {
            agg.closest_calls.push(format!("{l:?}"));
        }
        Ok(solids)
    }

    /// A value's bodies, taken **for consumption** — eagerly copied first, always.
    /// The copy becomes the value's new binding (a new generation) and the previous
    /// binding goes to the consumer. Unconditional, because "is this the last use" is
    /// future knowledge and acting on it would break prefix-stability (module doc):
    /// the copy must be inserted here or never.
    fn take(&mut self, step: usize, id: ValueId) -> Result<Vec<Handle<Solid>>, KitError> {
        let bodies = self.solid_of(step, id)?.bodies.clone();
        if !bodies.is_empty() {
            let mut keep = Vec::with_capacity(bodies.len());
            for &h in &bodies {
                keep.push(self.kcopy(step, h)?);
            }
            self.values[id.0 as usize] = Some(Value::Solid(SolidValue {
                bodies: keep.clone(),
            }));
            self.generations[id.0 as usize]
                .as_mut()
                .expect("a solid value has a generation list")
                .push(keep);
            self.auto_copies.push(step);
        }
        Ok(bodies)
    }

    // ---- rational lifts ---------------------------------------------------------

    fn dec(&self, step: usize, x: f64, what: &str) -> Result<Rat, KitError> {
        Rat::from_decimal(x).ok_or_else(|| KitError::Kernel {
            step,
            what: format!("{what} ({x}) is outside the exact decimal window"),
            class: None,
            blame: None,
            mark: None,
        })
    }

    fn dec3(&self, step: usize, p: [f64; 3], what: &str) -> Result<[Rat; 3], KitError> {
        Ok([
            self.dec(step, p[0], what)?,
            self.dec(step, p[1], what)?,
            self.dec(step, p[2], what)?,
        ])
    }

    // ---- step execution ---------------------------------------------------------

    fn exec(&mut self, step: usize, s: &Step) -> Result<Option<Value>, KitError> {
        match s {
            Step::Cuboid { size, at } => self.cuboid(step, *size, *at).map(solid),
            Step::Cylinder {
                radius,
                height,
                at,
                axis,
            } => self.cylinder(step, *radius, *height, *at, *axis).map(solid),
            Step::Boolean { kind, args } => self.boolean(step, *kind, args).map(solid),
            Step::Translate { src, offset } => {
                let off = self.dec3(step, *offset, "translate offset")?;
                let iso = Isometry::translation(off);
                self.transform_each(step, *src, &iso).map(solid)
            }
            Step::Rotate {
                src,
                axis,
                deg,
                pivot,
            } => {
                let pivot_rat = self.pivot_point(step, *src, *pivot)?;
                let angle =
                    Angle::from_deg(self.dec(step, *deg, "rotation angle")?).ok_or_else(|| {
                        KitError::Kernel {
                            step,
                            what: format!("rotation angle {deg} is not representable"),
                            class: None,
                            blame: None,
                            mark: None,
                        }
                    })?;
                let iso = Isometry::rotation(Rotation {
                    axis: kaxis(*axis),
                    pivot: pivot_rat,
                    angle,
                });
                self.transform_each(step, *src, &iso).map(solid)
            }
            Step::Mirror { src, plane } => {
                let offset = self.dec(step, plane.offset, "mirror offset")?;
                let axis = kaxis(plane.normal);
                let bodies = self.take(step, *src)?;
                let mut out = Vec::with_capacity(bodies.len());
                for h in bodies {
                    match self.kapply(
                        step,
                        &Operation::Mirror {
                            solid: h,
                            axis,
                            offset,
                        },
                        None,
                    )? {
                        OpOutput::Mirror { solid } => out.push(solid),
                        other => {
                            return Err(KitError::Internal {
                                step,
                                what: format!("mirror answered {other:?}"),
                            });
                        }
                    }
                }
                Ok(solid(SolidValue { bodies: out }))
            }
            Step::Copy { src } => {
                // Reads without consuming — the kernel Copy leaves the source live.
                let bodies = self.solid_of(step, *src)?.bodies.clone();
                let mut out = Vec::with_capacity(bodies.len());
                for h in bodies {
                    out.push(self.kcopy(step, h)?);
                }
                Ok(solid(SolidValue { bodies: out }))
            }
            Step::Body { src, index } => {
                // Reads without consuming, and **copies**: `take` consumes the originals
                // and leaves the source holding fresh ones, so a body sharing its
                // source's handle would retire that handle the first time it was used.
                //
                // The copy is deliberately *not* recorded in `auto_copies`. That list
                // means "a copy was inserted before a consumption" — inspector
                // information about what value semantics cost. This copy is the
                // operation itself, and counting it would inflate a number whose whole
                // meaning is "copies you did not ask for".
                let bodies = &self.solid_of(step, *src)?.bodies;
                let h = *bodies.get(*index).ok_or_else(|| KitError::Program {
                    step,
                    what: format!(
                        "value {} has {} bod{}, so there is no body {}",
                        src.0,
                        bodies.len(),
                        if bodies.len() == 1 { "y" } else { "ies" },
                        index
                    ),
                })?;
                let one = self.kcopy(step, h)?;
                Ok(solid(SolidValue { bodies: vec![one] }))
            }
            Step::Plane { spec } => self
                .plane_value(step, spec)
                .map(|p| Some(Value::Plane(Box::new(p)))),
            Step::Sketch { plane, paths } => {
                if let PlaneRef::Value(id) = plane {
                    // Kind-checked now, so a sketch never stores a reference to a
                    // solid where its plane should be.
                    self.plane_of(step, *id)?;
                }
                // The court sits once, now — an invalid sketch is never stored, and
                // the error arrives early, with coordinates. What is stored is the
                // statement; every consumer lowers it again (cheap, deterministic).
                classify(step, paths, false)?;
                Ok(Some(Value::Sketch(SketchValue {
                    plane: *plane,
                    paths: paths.clone(),
                })))
            }
            Step::Extrude { sketch, dist } => self.extrude(step, *sketch, *dist).map(solid),
            Step::Pad { face, sketch, dist } => self
                .pad_pocket(step, false, *face, *sketch, *dist)
                .map(solid),
            Step::Pocket { face, sketch, dist } => self
                .pad_pocket(step, true, *face, *sketch, *dist)
                .map(solid),
            Step::Display { targets, .. } => {
                for id in targets {
                    // Only solids are drawn — sketches and planes are ingredients:
                    // A plane has nothing to draw — no extent of its own, only a
                    // statement about where things sit. Solids and sketches do.
                    let v = self.values[id.0 as usize].as_ref();
                    if v.is_some_and(|v| v.as_plane().is_some()) {
                        return Err(KitError::Program {
                            step,
                            what: format!(
                                "value {} is a plane — a plane has no extent to draw; \
                                 display what you drew on it",
                                id.0
                            ),
                        });
                    }
                }
                Ok(None)
            }
        }
    }

    /// The value as a solid, or the kind-mismatch program error.
    fn solid_of(&self, step: usize, id: ValueId) -> Result<&SolidValue, KitError> {
        let v = self.values[id.0 as usize]
            .as_ref()
            .expect("validated: references point at value steps");
        v.as_solid().ok_or_else(|| KitError::Program {
            step,
            what: format!("value {} is {} — this step needs a solid", id.0, v.kind()),
        })
    }

    /// The value as a plane, or the kind-mismatch program error.
    fn plane_of(&self, step: usize, id: ValueId) -> Result<&PlaneValue, KitError> {
        let v = self.values[id.0 as usize]
            .as_ref()
            .expect("validated: references point at value steps");
        v.as_plane().ok_or_else(|| KitError::Program {
            step,
            what: format!("value {} is {} — this step needs a plane", id.0, v.kind()),
        })
    }

    /// The kernel's datum door: state `def`, get back the interned plane handle and
    /// the frame the statement fixes on it.
    fn kdatum(
        &mut self,
        step: usize,
        def: DatumDef,
    ) -> Result<(Handle<Surface>, SketchFrame), KitError> {
        match self.kapply(step, &Operation::DatumPlane { def }, None)? {
            OpOutput::DatumPlane { plane, frame } => Ok((plane, frame)),
            other => Err(KitError::Internal {
                step,
                what: format!("datum answered {other:?}"),
            }),
        }
    }

    /// Build a plane value: translate the spec into a kernel datum statement, keep the
    /// statement itself as [`PlaneSrc`] (the raw material for the flipped re-statement
    /// a negative extrude needs).
    fn plane_value(&mut self, step: usize, spec: &PlaneSpec) -> Result<PlaneValue, KitError> {
        let (def, src) = match spec {
            PlaneSpec::World(wp) => (
                DatumDef::Stated(world_plane(*wp)),
                PlaneSrc::WorldAxes {
                    base: *wp,
                    origin: [0.0; 3],
                },
            ),
            PlaneSpec::WorldAt { base, origin } => (
                DatumDef::Stated(world_plane(*base).with_origin(Point3::from_array(*origin))),
                PlaneSrc::WorldAxes {
                    base: *base,
                    origin: *origin,
                },
            ),
            PlaneSpec::Points {
                origin,
                x_point,
                y_hint,
            } => {
                // The written points ARE the definition (the kernel keeps them as its
                // `PlaneDef`) — no kit-side arithmetic at all. `None` is a degenerate
                // statement: coincident or collinear points direct no plane.
                let sp = SketchPlane::through_points(
                    Point3::from_array(*origin),
                    Point3::from_array(*x_point),
                    Point3::from_array(*y_hint),
                )
                .ok_or_else(|| KitError::Program {
                    step,
                    what: format!(
                        "the points {origin:?}, {x_point:?}, {y_hint:?} are coincident \
                         or collinear — they name no plane"
                    ),
                })?;
                (
                    DatumDef::Stated(sp),
                    PlaneSrc::Points {
                        origin: *origin,
                        x_point: *x_point,
                        y_hint: *y_hint,
                    },
                )
            }
            PlaneSpec::Offset { base, dist } => {
                // Not the kernel's `DatumDef::Offset`: that door normalizes to the
                // plane's **canonical** frame, whose direction is deliberately not a
                // contract — a sketch on it could extrude either way. The kit instead
                // re-states the base's own axes at the shifted origin (exact: one
                // decimal added to one coordinate), which pins the direction to the
                // base statement *and* still interns onto the same plane handle (the
                // canonical name is derived from the stated points).
                let bv = self.plane_of(step, *base)?.clone();
                let PlaneSrc::WorldAxes { base: wp, origin } = bv.src else {
                    return Err(KitError::Program {
                        step,
                        what: "an offset over a three-point plane has an irrational \
                               origin — no exact re-statement exists to pin its frame; \
                               state the tilted plane where you need it with its own \
                               three points"
                            .into(),
                    });
                };
                let k = match wp {
                    WorldPlane::XY => 2,
                    WorldPlane::YZ => 0,
                    WorldPlane::ZX => 1,
                };
                let shifted = self
                    .dec(step, origin[k], "offset base origin")?
                    .checked_add(self.dec(step, *dist, "offset distance")?)
                    .ok_or_else(|| KitError::Program {
                        step,
                        what: "the offset overflows the exact window".into(),
                    })?;
                let mut o = origin;
                o[k] = shifted.to_f64();
                (
                    DatumDef::Stated(world_plane(wp).with_origin(Point3::from_array(o))),
                    PlaneSrc::WorldAxes {
                        base: wp,
                        origin: o,
                    },
                )
            }
            PlaneSpec::Through { vertices } => {
                let handles = self.through_handles(step, vertices)?;
                (
                    DatumDef::ThroughVertices(handles),
                    PlaneSrc::Through {
                        vertices: vertices.map(|r| r.vertex),
                    },
                )
            }
        };
        let (plane, frame) = self.kdatum(step, def)?;
        Ok(PlaneValue { plane, frame, src })
    }

    /// Restore a `through` statement's vertex handles from their resolved references —
    /// with membership checked against **every generation** the named value has had
    /// (a query may have reported any of them; the arena is append-only, so the walks
    /// stand even after a supersede). An in-range index naming some *other* solid's
    /// vertex is a program error, not a plane.
    ///
    /// These reads are deliberately absent from `Step::reads()` — naming a solid's
    /// vertices does not visually replace the solid (the render-set comment there) —
    /// so the backward-reference check happens here instead.
    fn through_handles(
        &self,
        step: usize,
        refs: &[crate::step::VertexRef; 3],
    ) -> Result<[Handle<nacre::topo::Vertex>; 3], KitError> {
        let mut out = [None; 3];
        for (slot, r) in out.iter_mut().zip(refs) {
            if r.of.0 as usize >= self.values.len() {
                return Err(KitError::Program {
                    step,
                    what: format!("value {} is not defined before this step", r.of.0),
                });
            }
            self.solid_of(step, r.of)?; // the kind error, before the reference error
            let named = self.generations[r.of.0 as usize]
                .as_ref()
                .expect("a solid value has a generation list")
                .iter()
                .any(|generation| {
                    crate::query::solid_vertex_indices(&self.model, generation).contains(&r.vertex)
                });
            if !named {
                return Err(KitError::Program {
                    step,
                    what: format!(
                        "vertex {} is not a vertex of value {} — pick it from a query \
                         against that value",
                        r.vertex, r.of.0
                    ),
                });
            }
            *slot = Some(
                self.model
                    .vertex_handle_at(r.vertex)
                    .expect("membership-checked index"),
            );
        }
        Ok(out.map(|s| s.expect("filled")))
    }

    /// The frame an extrude sweeps through, and whether the sketch's segments must be
    /// mirrored (`(x, y) → (x, −y)`) to keep the caller's footprint — true exactly on
    /// the flipped (backward) road.
    fn extrude_frame(
        &mut self,
        step: usize,
        plane: &PlaneRef,
        forward: bool,
    ) -> Result<(SketchFrame, bool), KitError> {
        if forward {
            let frame = match plane {
                PlaneRef::World(wp) => SketchFrame::world(&self.model, world_axis(*wp)),
                PlaneRef::Value(id) => self.plane_of(step, *id)?.frame,
            };
            return Ok((frame, false));
        }
        let frame = match plane {
            PlaneRef::World(wp) => self.flipped_world_frame(step, *wp, [0.0; 3])?,
            PlaneRef::Value(id) => {
                let src = self.plane_of(step, *id)?.src.clone();
                self.flipped_frame(step, &src)?
            }
        };
        Ok((frame, true))
    }

    /// The flipped re-statement road (K1, origin-generalized): the same plane stated
    /// with the opposite normal — axes pinned (`u′ = +u, v′ = −v`) — is the same
    /// interned handle with the opposite frame, and the mirrored segments keep the
    /// caller's world footprint exactly. The axes are world unit vectors, so the
    /// statement is exact for any decimal origin.
    fn flipped_world_frame(
        &mut self,
        step: usize,
        wp: WorldPlane,
        origin: [f64; 3],
    ) -> Result<SketchFrame, KitError> {
        self.world_frame_at(step, wp, origin, true)
    }

    /// **This world plane, at this origin, this way round** — the one statement both the flipped
    /// re-statement and a ranged extrude's start plane are made of.
    ///
    /// The origin is why this exists as its own sentence. A ranged extrude does **not** move a
    /// solid afterwards; it states the sketch's plane where the range *starts* and sweeps the
    /// range's length. Moving the solid would leave a motion on it (`transform` only skips the
    /// node for a plane the motion fixes, never for a cylinder), so the same shape would come out
    /// with a different character — the same volume down a different road.
    fn world_frame_at(
        &mut self,
        step: usize,
        wp: WorldPlane,
        origin: [f64; 3],
        flipped: bool,
    ) -> Result<SketchFrame, KitError> {
        let (u, v) = match wp {
            WorldPlane::XY => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            WorldPlane::YZ => ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            WorldPlane::ZX => ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        };
        let v = if flipped { [-v[0], -v[1], -v[2]] } else { v };
        let sp = SketchPlane::from_axes(
            Point3::from_array(origin),
            Vector3::from_array(u),
            Vector3::from_array(v),
        );
        Ok(self.kdatum(step, DatumDef::Stated(sp))?.1)
    }

    /// `origin` moved along `wp`'s own normal by `d` — where a ranged extrude starts.
    fn along_normal(origin: [f64; 3], wp: WorldPlane, d: f64) -> [f64; 3] {
        let mut o = origin;
        match wp {
            WorldPlane::XY => o[2] += d,
            WorldPlane::YZ => o[0] += d,
            WorldPlane::ZX => o[1] += d,
        }
        o
    }

    /// A statement of the same plane with the opposite frame — where the statement's
    /// shape allows one. A `Points` plane's opposite frame is the **user's** statement
    /// to make: the opposite frame's axes are canonically derived, not the statement's,
    /// so no footprint mapping is the kit's to invent (the order of the points is the
    /// direction).
    fn flipped_frame(&mut self, step: usize, src: &PlaneSrc) -> Result<SketchFrame, KitError> {
        match src {
            PlaneSrc::WorldAxes { base, origin } => self.flipped_world_frame(step, *base, *origin),
            PlaneSrc::Points { .. } => Err(KitError::Program {
                step,
                what: "extruding backwards off a three-point plane is not supported — \
                       state the plane with xPoint and yHint swapped and extrude along \
                       its normal"
                    .into(),
            }),
            PlaneSrc::Through { .. } => Err(KitError::Program {
                step,
                what: "extruding backwards off a through-plane is not supported — \
                       state the plane with the opposite vertex order (swap two of the \
                       three) and extrude along its normal"
                    .into(),
            }),
        }
    }

    /// A cuboid by the extrude road: the profile's corners are computed in rationals
    /// (decimal ÷ 2 stays decimal, so the one f64 descent round-trips exactly), the
    /// height is the raw literal, and the z-anchor is an exact rational translation.
    fn cuboid(&mut self, step: usize, size: [f64; 3], at: Anchor) -> Result<SolidValue, KitError> {
        if !size.iter().all(|d| *d > 0.0) {
            return Err(KitError::Program {
                step,
                what: format!("cuboid size must be positive, got {size:?}"),
            });
        }
        let s = self.dec3(step, size, "cuboid size")?;
        let corner: [Rat; 3] = match at {
            Anchor::Corner(c) => self.dec3(step, c, "cuboid corner")?,
            Anchor::Center(c) => {
                let c = self.dec3(step, c, "cuboid center")?;
                let half = Rat::new(1, 2).expect("1/2");
                let mut out = [Rat::from_int(0); 3];
                for k in 0..3 {
                    out[k] = c[k]
                        .checked_sub(s[k].checked_mul(half).ok_or_else(|| KitError::Program {
                            step,
                            what: "cuboid size overflows the exact window".into(),
                        })?)
                        .ok_or_else(|| KitError::Program {
                            step,
                            what: "cuboid center overflows the exact window".into(),
                        })?;
                }
                out
            }
        };
        let (x0, y0) = (corner[0], corner[1]);
        let x1 = x0.checked_add(s[0]).ok_or_else(|| KitError::Program {
            step,
            what: "cuboid extent overflows the exact window".into(),
        })?;
        let y1 = y0.checked_add(s[1]).ok_or_else(|| KitError::Program {
            step,
            what: "cuboid extent overflows the exact window".into(),
        })?;
        let p2 = |x: Rat, y: Rat| Point2::from_array([x.to_f64(), y.to_f64()]);
        let profile = Profile2d::polygon(vec![p2(x0, y0), p2(x1, y0), p2(x1, y1), p2(x0, y1)])
            .map_err(|e| KitError::Kernel {
                step,
                what: format!("{e:?}"),
                class: None,
                blame: None,
                mark: None,
            })?;
        let frame = SketchFrame::world(&self.model, Axis::Z);
        let out = self.kapply(
            step,
            &Operation::Extrude {
                frame,
                profile,
                dist: size[2],
            },
            None,
        )?;
        let OpOutput::Extrude { solid, .. } = out else {
            return Err(KitError::Internal {
                step,
                what: format!("extrude answered {out:?}"),
            });
        };
        // Anchor in z, exactly.
        let z = corner[2];
        let solid = if z == Rat::from_int(0) {
            solid
        } else {
            let iso = Isometry::translation([Rat::from_int(0), Rat::from_int(0), z]);
            match self.kapply(
                step,
                &Operation::Transform {
                    solid,
                    isometry: iso,
                },
                None,
            )? {
                OpOutput::Transform { solid } => solid,
                other => {
                    return Err(KitError::Internal {
                        step,
                        what: format!("transform answered {other:?}"),
                    });
                }
            }
        };
        self.model.rebuild_adjacency();
        Ok(SolidValue {
            bodies: vec![solid],
        })
    }

    /// Stand a cylinder on the world XY plane and lift it to its anchor — **sugar**: a
    /// one-circle sketch extruded `height`, the road a user's `circle` sketch takes
    /// (the kernel locks this against its retired primitive).
    ///
    /// **No computed number goes through the f64 door.** The three the kernel is
    /// handed — the circle's `x`/`y`, the radius, the height — are the caller's own
    /// written decimals, which `Rat::from_decimal` recovers exactly. The one value this
    /// function *computes*, the base `z` (`z − h/2` for a centre anchor), never becomes
    /// an f64: it rides `Isometry::translation`, which takes rationals.
    ///
    /// That is stricter than [`Builder::cuboid`], whose computed corner has to reach the
    /// kernel as profile coordinates — a round trip that is exact only while the decimal
    /// stays short. A cylinder's two *in-plane* coordinates need no arithmetic under
    /// either anchor, so the round trip is not merely short here, it is absent.
    /// **The axis names the plane, and every "z" in this body is really "along the
    /// axis".** `Z` sketches on XY, `X` on YZ, `Y` on ZX — and those three frames run
    /// `(u, v) = (x, y)`, `(y, z)`, `(z, x)`, which is the **cyclic** shift
    /// `u = (axis + 1) % 3`, `v = (axis + 2) % 3` (read off `SketchPlane::world_yz`'s and
    /// `world_zx`'s stated axes, not guessed). So the circle's centre is the anchor's two
    /// in-plane components and the final move runs along the axis.
    ///
    /// **Picking components is sound only because all three world frames sit at the
    /// origin** and `SketchFrame::world` leaves `flip` false, so `+w` realizes to `+axis`.
    /// Only the second of those is stated by that function's doc (where it is marked a
    /// measurement, not a derivation); the origin is **not** written down anywhere, which
    /// is why the assertion below measures it here rather than citing it. A frame with an
    /// offset origin would need the projection written out, and that assertion is what
    /// would say so.
    fn cylinder(
        &mut self,
        step: usize,
        radius: f64,
        height: f64,
        at: CylAnchor,
        axis: KitAxis,
    ) -> Result<SolidValue, KitError> {
        // Written as `all(> 0)` rather than `<= 0` so a NaN is refused rather than
        // slipping past a negated comparison — the `cuboid` spelling.
        if ![radius, height].iter().all(|d| *d > 0.0) {
            return Err(KitError::Program {
                step,
                what: format!(
                    "cylinder radius and height must be positive, got radius {radius}, height {height}"
                ),
            });
        }
        // Lifted for the window check even though the kernel is handed the caller's own
        // f64: refusing here names the number in the user's words, and leaves the
        // sketch court's own window errors as the kernel API's net rather than
        // something a script can provoke.
        self.dec(step, radius, "cylinder radius")?;
        let h = self.dec(step, height, "cylinder height")?;
        // The axis' own index, and the two the sketch plane runs on.
        let w = match axis {
            KitAxis::X => 0,
            KitAxis::Y => 1,
            KitAxis::Z => 2,
        };
        let (iu, iv) = ((w + 1) % 3, (w + 2) % 3);
        // The labels follow the axis rather than saying "x"/"y"/"z": a message that names
        // the wrong coordinate is a sentence that is quietly false, which costs more than
        // the three words it saves.
        let name = |i: usize| ["x", "y", "z"][i];
        let (centre, base_w) = match at {
            CylAnchor::Base(p) => (
                p,
                self.dec(step, p[w], &format!("cylinder base {}", name(w)))?,
            ),
            CylAnchor::Center(p) => {
                let c = self.dec(step, p[w], &format!("cylinder centre {}", name(w)))?;
                let half = Rat::new(1, 2).expect("1/2");
                let down = h.checked_mul(half).ok_or_else(|| KitError::Program {
                    step,
                    what: "cylinder height overflows the exact window".into(),
                })?;
                (
                    p,
                    c.checked_sub(down).ok_or_else(|| KitError::Program {
                        step,
                        what: "cylinder centre overflows the exact window".into(),
                    })?,
                )
            }
        };
        // The window check for the two in-plane coordinates happens here even though the
        // values pass through untouched: a coordinate the kernel cannot state exactly is
        // the caller's mistake to hear about now, in the kit's own words.
        self.dec(step, centre[iu], &format!("cylinder centre {}", name(iu)))?;
        self.dec(step, centre[iv], &format!("cylinder centre {}", name(iv)))?;

        let circle = crate::step::Path::Circle {
            center: [centre[iu], centre[iv]],
            size: crate::step::CircleSize::Radius(radius),
        };
        let mut profiles = classify(step, &[circle], false)?;
        let Some(profile) = profiles.pop().filter(|_| profiles.is_empty()) else {
            return Err(KitError::Internal {
                step,
                what: "one circle is one profile".into(),
            });
        };
        let frame = SketchFrame::world(&self.model, kaxis(axis));
        // The two facts the component pick rests on, checked where it is made: the frame
        // sits at the origin, and its `+u`/`+v` are the world axes the cyclic shift names.
        debug_assert!(
            {
                let p = nacre::ops::frame_plane(&self.model, &frame).expect("a world frame");
                let unit = |v: nacre::math::Vector3, k: usize| {
                    (0..3).all(|i| (v.as_array()[i] - f64::from(u8::from(i == k))).abs() < 1e-12)
                };
                p.origin().as_array().iter().all(|c| c.abs() < 1e-12)
                    && unit(p.x_axis(), iu)
                    && unit(p.y_axis(), iv)
            },
            "a world sketch frame is at the origin with (u, v) the cyclic world axes"
        );
        let out = self.kapply(
            step,
            &Operation::Extrude {
                frame,
                profile,
                dist: height,
            },
            None,
        )?;
        let OpOutput::Extrude { solid, .. } = out else {
            return Err(KitError::Internal {
                step,
                what: format!("extrude answered {out:?}"),
            });
        };
        // Anchor along the axis, exactly — the base cap stands on the frame's plane until this.
        let solid = if base_w == Rat::from_int(0) {
            solid
        } else {
            let mut offset = [Rat::from_int(0); 3];
            offset[w] = base_w;
            let iso = Isometry::translation(offset);
            match self.kapply(
                step,
                &Operation::Transform {
                    solid,
                    isometry: iso,
                },
                None,
            )? {
                OpOutput::Transform { solid } => solid,
                other => {
                    return Err(KitError::Internal {
                        step,
                        what: format!("transform answered {other:?}"),
                    });
                }
            }
        };
        self.model.rebuild_adjacency();
        Ok(SolidValue {
            bodies: vec![solid],
        })
    }

    /// Extrude a sketch: per-direction re-classification (deterministic, cheap —
    /// and it makes "two directions, two different results" unrepresentable), one
    /// prism per island, all islands one value.
    fn extrude(
        &mut self,
        step: usize,
        sketch: ValueId,
        dist: Dist,
    ) -> Result<SolidValue, KitError> {
        match dist {
            Dist::One(d) => {
                let bodies = self.extrude_one(step, sketch, d)?;
                self.model.rebuild_adjacency();
                Ok(SolidValue { bodies })
            }
            // **A range says where the sketch starts, not that a solid moves.** It used
            // to sweep **twice** — once each way from the plane — and fuse the halves. That cost a
            // boolean per island and, worse, stated one cylinder twice: a fillet swept down and
            // the same fillet swept up differ only in the axis's **sign**, which is not part of
            // what the surface is, and the two statements met as a coincident pair the kernel
            // refuses. So a filleted profile could not be extruded across its own plane at all.
            //
            // One sweep, from a plane stated at the range's start. No fuse, no flipped frame, no
            // mirrored rings — and no motion on the result, which a "sweep then translate" would
            // have left behind.
            Dist::Both(lo, hi) => {
                // NaN is not ordered either, and this spelling says so — the same door
                // `pad_pocket` uses for its own positive-depth demand.
                if lo.partial_cmp(&hi) != Some(std::cmp::Ordering::Less) {
                    return Err(KitError::Program {
                        step,
                        what: format!(
                            "an extrude range runs from the first distance to the second, so \
                             they must be ordered — got ({lo}, {hi})"
                        ),
                    });
                }
                let bodies = self.extrude_span(step, sketch, lo, hi - lo)?;
                self.model.rebuild_adjacency();
                Ok(SolidValue { bodies })
            }
        }
    }

    /// One direction of an extrude: the island prisms, unassembled.
    fn extrude_one(
        &mut self,
        step: usize,
        sketch: ValueId,
        dist: f64,
    ) -> Result<Vec<Handle<Solid>>, KitError> {
        if dist == 0.0 {
            return Err(KitError::Program {
                step,
                what: "extrude distance must be nonzero".into(),
            });
        }
        let sv = self.sketch_of(step, sketch)?;
        let (frame, mirrored) = self.extrude_frame(step, &sv.plane, dist > 0.0)?;
        self.extrude_profiles(step, &sv, frame, mirrored, dist.abs())
    }

    /// **One sweep of `len`, from the sketch's plane stated `start` along its own normal** — what
    /// a ranged extrude is. `start == 0` is the plane the sketch already names, so that case takes
    /// the very road a one-sided extrude does and cannot drift from it.
    fn extrude_span(
        &mut self,
        step: usize,
        sketch: ValueId,
        start: f64,
        len: f64,
    ) -> Result<Vec<Handle<Solid>>, KitError> {
        let sv = self.sketch_of(step, sketch)?;
        let frame = if start == 0.0 {
            self.extrude_frame(step, &sv.plane, true)?.0
        } else {
            self.offset_frame(step, &sv.plane, start)?
        };
        // The frame is the plane's own, never the flipped re-statement, so the rings are read as
        // drawn.
        self.extrude_profiles(step, &sv, frame, false, len)
    }

    /// The sketch's plane, restated `d` along its normal — the start plane of a ranged extrude.
    ///
    /// A plane written as three points or three vertices declines, exactly as
    /// [`Self::flipped_frame`] does and for the same reason: its parallel restatement's axes are
    /// the **user's** to choose, not the kit's to invent.
    fn offset_frame(
        &mut self,
        step: usize,
        plane: &PlaneRef,
        d: f64,
    ) -> Result<SketchFrame, KitError> {
        let (base, origin) = match plane {
            PlaneRef::World(wp) => (*wp, [0.0; 3]),
            PlaneRef::Value(id) => match self.plane_of(step, *id)?.src.clone() {
                PlaneSrc::WorldAxes { base, origin } => (base, origin),
                PlaneSrc::Points { .. } | PlaneSrc::Through { .. } => {
                    return Err(KitError::Program {
                        step,
                        what: "an extrude range off a three-point plane is not supported — \
                               state the plane where the range starts and extrude along its \
                               normal"
                            .into(),
                    });
                }
            },
        };
        self.world_frame_at(step, base, Self::along_normal(origin, base, d), false)
    }

    /// The sketch a value holds, cloned out of the value table.
    fn sketch_of(&self, step: usize, sketch: ValueId) -> Result<SketchValue, KitError> {
        let v = self.values[sketch.0 as usize]
            .as_ref()
            .expect("validated: references point at value steps");
        Ok(v.as_sketch()
            .ok_or_else(|| KitError::Program {
                step,
                what: format!(
                    "value {} is {} — extrude needs a sketch",
                    sketch.0,
                    v.kind()
                ),
            })?
            .clone())
    }

    /// One prism per island, on a frame already chosen — the half both extrude roads share.
    fn extrude_profiles(
        &mut self,
        step: usize,
        sv: &SketchValue,
        frame: SketchFrame,
        mirrored: bool,
        dist: f64,
    ) -> Result<Vec<Handle<Solid>>, KitError> {
        let profiles = classify(step, &sv.paths, mirrored)?;
        let mut bodies = Vec::with_capacity(profiles.len());
        for profile in profiles {
            let out = self.kapply(
                step,
                &Operation::Extrude {
                    frame,
                    profile,
                    dist,
                },
                None,
            )?;
            let OpOutput::Extrude { solid, .. } = out else {
                return Err(KitError::Internal {
                    step,
                    what: format!("extrude answered {out:?}"),
                });
            };
            bodies.push(solid);
        }
        Ok(bodies)
    }

    /// Pad or pocket on a face named by a resolved reference.
    ///
    /// **The face is found by position, not by index alone.** The reference's index
    /// dates from whatever binding generation the query ran against, and `take` may
    /// hand this consumption a *different* generation (an automatic copy). A kernel
    /// `Copy` is a structural clone — outer shell then cavities, each shell's faces in
    /// source order (`transform_solid` spells the order out) — so `(body, shell, face
    /// position)` names the same face in every generation: locate the index in the
    /// current binding or the produced bodies, then read that position off the taken
    /// bodies. Pure handle-walking; no geometry is judged (boundary rule).
    fn pad_pocket(
        &mut self,
        step: usize,
        pocket: bool,
        face: FaceRef,
        sketch: ValueId,
        dist: f64,
    ) -> Result<SolidValue, KitError> {
        let name = if pocket { "pocket" } else { "pad" };
        if dist.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return Err(KitError::Program {
                step,
                what: format!(
                    "a {name} needs a positive depth, got {dist} — the direction is \
                     the face's own (outward for pad, inward for pocket)"
                ),
            });
        }
        // The sketch's profile, in the face's frame (its `plane` field is not
        // consulted here), and exactly one island.
        let sv = {
            let v = self.values[sketch.0 as usize]
                .as_ref()
                .expect("validated: references point at value steps");
            v.as_sketch()
                .ok_or_else(|| KitError::Program {
                    step,
                    what: format!("value {} is {} — {name} needs a sketch", sketch.0, v.kind()),
                })?
                .clone()
        };
        let mut profiles = classify(step, &sv.paths, false)?;
        if profiles.len() != 1 {
            return Err(KitError::Program {
                step,
                what: format!(
                    "one island per {name} — this sketch has {}; {name} each island \
                     separately",
                    profiles.len()
                ),
            });
        }
        let profile = profiles.remove(0);

        // Locate the reference across every generation the value has had (a query may
        // have reported any of them; positions are generation-invariant), take the
        // value, and read the position off the taken bodies.
        self.solid_of(step, face.of)?; // the kind error, before the reference error
        let (bi, si, fi) = self.generations[face.of.0 as usize]
            .as_ref()
            .expect("a solid value has a generation list")
            .iter()
            .find_map(|generation| locate_face(&self.model, generation, face.face))
            .ok_or_else(|| KitError::Program {
                step,
                what: format!(
                    "face {} is not a face of value {} — pick it from a query against \
                     that value",
                    face.face, face.of.0
                ),
            })?;
        let bodies = self.take(step, face.of)?;
        let target = face_at(&self.model, bodies[bi], si, fi);

        let op = if pocket {
            Operation::PocketOnFace {
                face: target,
                profile,
                dist,
            }
        } else {
            Operation::PadOnFace {
                face: target,
                profile,
                dist,
            }
        };
        let solid = match self.kapply(step, &op, None)? {
            OpOutput::PadOnFace { solid, .. } | OpOutput::PocketOnFace { solid, .. } => solid,
            other => {
                return Err(KitError::Internal {
                    step,
                    what: format!("{name} answered {other:?}"),
                });
            }
        };
        // Only the face's own body is superseded; the rest ride along unchanged.
        let mut out = bodies;
        out[bi] = solid;
        self.model.rebuild_adjacency();
        Ok(SolidValue { bodies: out })
    }

    fn pivot_point(
        &mut self,
        step: usize,
        src: ValueId,
        pivot: Pivot,
    ) -> Result<[Rat; 3], KitError> {
        match pivot {
            Pivot::Origin => Ok([Rat::from_int(0); 3]),
            Pivot::At(p) => self.dec3(step, p, "rotation pivot"),
            Pivot::Center => {
                // Volume-weighted centroid across bodies — a report coordinate; the
                // exact pivot is the rational it lifts to.
                let v = self.solid_of(step, src)?.clone();
                let (mut vol, mut acc) = (0.0f64, [0.0f64; 3]);
                for &h in &v.bodies {
                    let p =
                        nacre::props::mass_props(&self.model, h).map_err(|e| KitError::Kernel {
                            step,
                            what: format!("{e:?}"),
                            class: None,
                            blame: None,
                            mark: None,
                        })?;
                    let c =
                        nacre::props::centroid(&self.model, h).map_err(|e| KitError::Kernel {
                            step,
                            what: format!("{e:?}"),
                            class: None,
                            blame: None,
                            mark: None,
                        })?;
                    vol += p.volume;
                    for (a, x) in acc.iter_mut().zip(c.as_array()) {
                        *a += p.volume * x;
                    }
                }
                if vol == 0.0 {
                    return Err(KitError::Program {
                        step,
                        what: "pivot \"center\" of an empty value".into(),
                    });
                }
                self.dec3(step, acc.map(|a| a / vol), "pivot (center)")
            }
        }
    }

    fn transform_each(
        &mut self,
        step: usize,
        src: ValueId,
        iso: &Isometry,
    ) -> Result<SolidValue, KitError> {
        let bodies = self.take(step, src)?;
        let mut out = Vec::with_capacity(bodies.len());
        for h in bodies {
            match self.kapply(
                step,
                &Operation::Transform {
                    solid: h,
                    isometry: *iso,
                },
                None,
            )? {
                OpOutput::Transform { solid } => out.push(solid),
                other => {
                    return Err(KitError::Internal {
                        step,
                        what: format!("transform answered {other:?}"),
                    });
                }
            }
        }
        self.model.rebuild_adjacency();
        Ok(SolidValue { bodies: out })
    }

    // ---- the n-ary boolean folds ------------------------------------------------

    fn boolean(
        &mut self,
        step: usize,
        kind: KitBool,
        args: &[ValueId],
    ) -> Result<SolidValue, KitError> {
        if args.len() < 2 {
            return Err(KitError::Program {
                step,
                what: "a boolean needs at least two arguments".into(),
            });
        }
        // Take every argument up front (auto-copies land here, before consumption).
        let mut taken: Vec<(ValueId, Vec<Handle<Solid>>)> = Vec::with_capacity(args.len());
        for &id in args {
            let bodies = self.take(step, id)?;
            taken.push((id, bodies));
        }
        let bodies = match kind {
            KitBool::Fuse => self.fuse_fold(step, taken)?,
            KitBool::Cut => self.cut_fold(step, taken)?,
            KitBool::Common => self.common_fold(step, taken)?,
        };
        self.model.rebuild_adjacency();
        Ok(SolidValue { bodies })
    }

    /// Union of every body of every argument — pairwise closure. See the module doc
    /// for why each test is guard-copied.
    fn fuse_fold(
        &mut self,
        step: usize,
        taken: Vec<(ValueId, Vec<Handle<Solid>>)>,
    ) -> Result<Vec<Handle<Solid>>, KitError> {
        let mut next_id = 0u32;
        let mut list: Vec<Piece> = Vec::new();
        for (vid, bodies) in taken {
            for (k, h) in bodies.into_iter().enumerate() {
                list.push(Piece {
                    id: next_id,
                    h,
                    origin: vid,
                    label: format!("body {k}"),
                });
                next_id += 1;
            }
        }
        let mut proven: HashSet<(u32, u32)> = HashSet::new();
        loop {
            // The first unproven pair, in index order — determinism by construction.
            let mut pick = None;
            'find: for i in 0..list.len() {
                for j in (i + 1)..list.len() {
                    let key = (list[i].id.min(list[j].id), list[i].id.max(list[j].id));
                    if !proven.contains(&key) {
                        pick = Some((i, j, key));
                        break 'find;
                    }
                }
            }
            let Some((i, j, key)) = pick else { break };
            let blame = Blame {
                a: list[i].origin,
                b: list[j].origin,
                detail: format!("{} vs {}", list[i].label, list[j].label),
            };
            if list.len() == 2 {
                // No third party: whatever comes out *is* the value — merged, or the
                // kernel's proof that the two pieces stand apart. No identity needed.
                let r = self.kbool(step, BoolKind::Fuse, list[0].h, list[1].h, blame)?;
                return Ok(r);
            }
            // Guard copies keep exact identities for the disjoint verdict.
            let ca = self.kcopy(step, list[i].h)?;
            let cb = self.kcopy(step, list[j].h)?;
            let r = self.kbool(step, BoolKind::Fuse, list[i].h, list[j].h, blame)?;
            match r.len() {
                1 => {
                    // Merged: one new piece with a fresh identity; the guards are
                    // abandoned (inert, bounded — see module doc).
                    list[i] = Piece {
                        id: next_id,
                        h: r[0],
                        origin: list[i].origin,
                        label: format!("merged({}, {})", list[i].label, list[j].label),
                    };
                    next_id += 1;
                    list.remove(j);
                }
                2 => {
                    // Proven disjoint: the guards carry on under their exact
                    // identities; the fused outputs are abandoned.
                    proven.insert(key);
                    list[i].h = ca;
                    list[j].h = cb;
                }
                k => {
                    return Err(KitError::Internal {
                        step,
                        what: format!("a fuse of two solids answered {k} solids"),
                    });
                }
            }
        }
        Ok(list.into_iter().map(|p| p.h).collect())
    }

    /// `a` minus every tool body: each tool cuts every current piece (a cut can split
    /// a piece; the new pieces keep receiving the remaining tools).
    fn cut_fold(
        &mut self,
        step: usize,
        mut taken: Vec<(ValueId, Vec<Handle<Solid>>)>,
    ) -> Result<Vec<Handle<Solid>>, KitError> {
        let (a_id, a_bodies) = taken.remove(0);
        let mut pieces = a_bodies;
        for (tid, tbodies) in taken {
            for (bi, t) in tbodies.into_iter().enumerate() {
                let count = pieces.len();
                if count == 0 {
                    break; // nothing left to cut — the tool is simply unused
                }
                let mut next = Vec::new();
                for (pi, p) in std::mem::take(&mut pieces).into_iter().enumerate() {
                    // The tool is consumed per cut: copy for every piece but the last.
                    let t_use = if pi + 1 < count {
                        self.kcopy(step, t)?
                    } else {
                        t
                    };
                    let blame = Blame {
                        a: a_id,
                        b: tid,
                        detail: format!("intermediate {pi} vs body {bi}"),
                    };
                    next.extend(self.kbool(step, BoolKind::Cut, p, t_use, blame)?);
                }
                pieces = next;
            }
        }
        Ok(pieces)
    }

    /// Intersection of all arguments — the distributed pairwise products
    /// (`(⋃aᵢ) ∩ (⋃bⱼ) = ⋃(aᵢ ∩ bⱼ)`, and disjointness of the `aᵢ` carries into the
    /// products).
    fn common_fold(
        &mut self,
        step: usize,
        mut taken: Vec<(ValueId, Vec<Handle<Solid>>)>,
    ) -> Result<Vec<Handle<Solid>>, KitError> {
        let (mut acc_id, mut acc) = taken.remove(0);
        for (bid, bs) in taken {
            let (na, nb) = (acc.len(), bs.len());
            let mut out = Vec::new();
            for (ai, a) in acc.iter().enumerate() {
                for (bi, b) in bs.iter().enumerate() {
                    // Each `a` meets every `b` and vice versa: copy all but the last use.
                    let a_use = if bi + 1 < nb {
                        self.kcopy(step, *a)?
                    } else {
                        *a
                    };
                    let b_use = if ai + 1 < na {
                        self.kcopy(step, *b)?
                    } else {
                        *b
                    };
                    let blame = Blame {
                        a: acc_id,
                        b: bid,
                        detail: format!("intermediate {ai} vs body {bi}"),
                    };
                    out.extend(self.kbool(step, BoolKind::Common, a_use, b_use, blame)?);
                }
            }
            acc = out;
            acc_id = bid;
            if acc.is_empty() {
                // Intersection is already empty; later arguments cannot revive it,
                // but their values were already taken (consumed) — consistent with
                // "a boolean consumes its arguments".
                break;
            }
        }
        Ok(acc)
    }
}

fn solid(v: SolidValue) -> Option<Value> {
    Some(Value::Solid(v))
}

fn kaxis(a: KitAxis) -> Axis {
    match a {
        KitAxis::X => Axis::X,
        KitAxis::Y => Axis::Y,
        KitAxis::Z => Axis::Z,
    }
}

fn world_axis(wp: WorldPlane) -> Axis {
    match wp {
        WorldPlane::XY => Axis::Z,
        WorldPlane::YZ => Axis::X,
        WorldPlane::ZX => Axis::Y,
    }
}

fn world_plane(wp: WorldPlane) -> SketchPlane {
    match wp {
        WorldPlane::XY => SketchPlane::world_xy(),
        WorldPlane::YZ => SketchPlane::world_yz(),
        WorldPlane::ZX => SketchPlane::world_zx(),
    }
}

/// Where a face index sits in a body list: `(body, shell, face position)` — the
/// coordinates that survive a structural clone.
fn locate_face(
    model: &Model,
    bodies: &[Handle<Solid>],
    face: u32,
) -> Option<(usize, usize, usize)> {
    for (bi, &b) in bodies.iter().enumerate() {
        let s = model.solid(b);
        for (si, sh) in std::iter::once(s.outer)
            .chain(s.cavities.iter().copied())
            .enumerate()
        {
            for (fi, &fh) in model.shell(sh).faces.iter().enumerate() {
                if fh.index() == face {
                    return Some((bi, si, fi));
                }
            }
        }
    }
    None
}

/// The face at a structural position — [`locate_face`]'s inverse, on a (possibly
/// different) generation of the same body.
fn face_at(model: &Model, body: Handle<Solid>, si: usize, fi: usize) -> Handle<nacre::topo::Face> {
    let s = model.solid(body);
    let sh = std::iter::once(s.outer)
        .chain(s.cavities.iter().copied())
        .nth(si)
        .expect("a clone mirrors its source's shells");
    model.shell(sh).faces[fi]
}
