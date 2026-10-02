//! What the kit reports when a build cannot finish — kernel rejections translated to
//! *where and why*, with pair-level blame for n-ary booleans.

use crate::step::ValueId;
pub use nacre::ops::RejectClass;
use nacre::ops::{OpError, RejectWhere};

/// Which pair of a fold failed — the n-ary boolean promise.
#[derive(Debug, Clone)]
pub struct Blame {
    /// The two script values whose combination the kernel refused.
    pub a: ValueId,
    pub b: ValueId,
    /// Which piece of each — honest labels: an original body index reads `body k`, a
    /// derived intermediate (a cut can split pieces mid-fold) reads `intermediate k`
    /// and does not pretend to be an original index.
    pub detail: String,
}

/// Where a rejection was looking — the kernel's witness location, converted to plain world
/// coordinates for anything downstream (a viewer drawing a marker). Owned values only, no
/// handles: this is safe to hold after the model is gone.
///
/// The coordinates are the kernel's diagnostic realization — a witness (one of possibly
/// several offenders), approximate by nature. Draw with them; never judge with them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mark {
    /// A single point — e.g. the pinch vertex of a non-manifold result.
    Point([f64; 3]),
    /// A segment — e.g. the edge along which a result touches itself.
    Segment([[f64; 3]; 2]),
}

impl From<RejectWhere> for Mark {
    fn from(w: RejectWhere) -> Self {
        match w {
            RejectWhere::Point(p) => Mark::Point([p[0], p[1], p[2]]),
            RejectWhere::Segment([a, b]) => Mark::Segment([[a[0], a[1], a[2]], [b[0], b[1], b[2]]]),
        }
    }
}

/// A build failure. `step` is always the index of the script step being built — read it
/// with [`KitError::step`].
///
/// **The step is a field, not prose.** `Display` does not repeat it: a caller printing
/// an error has the value in hand, and one carrying it across a boundary sends the index
/// alongside the sentence. Writing it into the sentence as well made a consumer state the
/// same fact twice — the app showed `Where line 2 — extrude` beside `Message step 1: …`,
/// and doubled it outright on the path where it added a prefix of its own.
#[derive(Debug)]
pub enum KitError {
    /// The program itself is malformed — a reference to a step that produces no value,
    /// a forward reference, a non-positive size. Caught before the kernel is asked.
    Program { step: usize, what: String },
    /// The kernel refused, honestly — carried with its own words plus where.
    Kernel {
        step: usize,
        /// The kernel's stable identifier for a boolean rejection (`RejectReason`'s
        /// `Display`), or the operation error's debug form for everything else.
        what: String,
        /// **What kind of answer the rejection is** — the kernel's own classification
        /// ([`RejectClass`]: not-supported-yet / impossible / suspected-defect), re-exported
        /// rather than mirrored so the two vocabularies cannot drift. `None` for the
        /// operation-level errors (`OpError`) that carry no classification yet.
        ///
        /// The *reason* itself is deliberately not carried: `RejectReason` holds model-lifetime
        /// handles (`TraceDeclined { face }`) that mean nothing outside the model that minted
        /// them, and its stable identifier already travels in `what`. The location does travel —
        /// [`Mark`] holds owned world coordinates, no handles.
        class: Option<RejectClass>,
        blame: Option<Blame>,
        /// Where the kernel was looking when it refused, when it said ([`Mark`]).
        mark: Option<Mark>,
    },
    /// The kit's own invariant broke: a liveness rejection (`SolidNotLive`) reached
    /// the surface. The value-semantics layer exists to make that impossible, so this
    /// is a kit bug — never a user error, and the
    /// test suite pins that it stays unreachable.
    Internal { step: usize, what: String },
}

impl KitError {
    /// Which step refused. Every variant carries one, so this never has to be dug out by
    /// a match — and a variant added later cannot silently skip it.
    pub fn step(&self) -> usize {
        match self {
            KitError::Program { step, .. }
            | KitError::Kernel { step, .. }
            | KitError::Internal { step, .. } => *step,
        }
    }
}

impl std::fmt::Display for KitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KitError::Program { what, .. } => write!(f, "{what}"),
            KitError::Kernel {
                what, class, blame, ..
            } => {
                // Class verdict first, then the reason's own sentence when one exists —
                // uniform: "<verdict> — <fact> [identifier]".
                match class {
                    Some(c) => {
                        write!(f, "{}", class_sentence(*c))?;
                        if let Some(fact) = reason_sentence(what) {
                            write!(f, " — {fact}")?;
                        }
                        write!(f, " [{what}]")?
                    }
                    None => write!(f, "{what}")?,
                }
                if let Some(b) = blame {
                    write!(
                        f,
                        " (between value {} and value {}: {})",
                        b.a.0, b.b.0, b.detail
                    )?;
                }
                Ok(())
            }
            KitError::Internal { what, .. } => {
                // A fact, not a request: this variant existing at all means a kit
                // invariant broke.
                write!(f, "a kit invariant broke — {what}")
            }
        }
    }
}

impl std::error::Error for KitError {}

/// The class verdict as a statement of fact — facts only, never advice or prediction (the
/// kernel's standing rule, *a diagnosis, not a prompt*, and the user's: no smuggled tense).
/// The subjects are deliberately distinct: the kernel's capability, the input's nature, the
/// engine's own invariant.
/// What an author is told when an operation refuses — the sentence, then the identifier.
///
/// **Exhaustive on purpose.** A new [`OpError`] does not compile until someone decides
/// whether an author needs words for it, and `None` is a decision too: the identifier
/// alone, which is what every one of these said before. The sibling table below
/// (`reason_sentence`) can afford string keys because a renamed reject reason turns the
/// kernel's census gate red; nothing watches `OpError` that way, so the compiler is the
/// only thing that can ask.
///
/// The format matches the classified line overhead — `<fact> [identifier]` — because the
/// identifier is the handle a reader searches with, and a message that dropped it would be
/// prose with nothing to look up.
pub(crate) fn op_words(e: &OpError) -> String {
    let fact = match e {
        // Measured reaching an author through the app, in the order a person meets them.
        OpError::PocketNotBlind => {
            "a pocket has to stop inside the material — this depth reaches through, so there \
             is no floor"
        }
        OpError::PadMissesFace => {
            "the pad's outline does not meet the face — an outline hanging over the edge \
             still touches, this one misses it entirely"
        }
        OpError::NonPlanarFace => {
            // Two callers, not one: a pad or pocket aiming at a face, and a sketch whose
            // frame names a curved surface. Saying "pad and pocket" would be wrong for the
            // second, so this says what is true of the surface either way.
            "a sketch stands on a flat face — this surface is curved, and features on curved \
             faces are not built yet"
        }
        OpError::DuplicateVertex => {
            "the same vertex is named twice — two points do not fix a plane"
        }
        OpError::DistOutsideDecimalWindow => {
            // **Says no operation's name.** This one is raised from four places — a plane's
            // offset, a prism's depth, and a pad's or pocket's — so naming `extrude` (the
            // case it was first provoked from) would be a lie to three of them. The sketch
            // layer's own words for its sibling refusal, said the same way.
            "a distance is outside the exact decimal window, so it has no rational form to \
             build with"
        }
        OpError::ArcsMeetAtVertex => {
            "two arcs of different circles meet at a corner — the kernel has no definition \
             for a point on two cylinders yet; a straight step between the arcs builds"
        }
        // No words yet. Not measured reaching an author, so the identifier alone — which
        // is exactly what they said before this table existed. Deciding one is adding an
        // arm above; the fields some of them carry belong in the sentence when that
        // happens, the way `sketch.rs` writes its witnesses.
        OpError::DegenerateProfile
        | OpError::ProfileOutsideDecimalWindow { .. }
        | OpError::SelfIntersectingProfile { .. }
        | OpError::ZeroLengthProfileEdge { .. }
        | OpError::ProfileRingsMeet { .. }
        | OpError::HoleNotInsideOuter { .. }
        | OpError::NestedHole { .. }
        | OpError::ProfileUndecidable
        | OpError::ArcSweepNotQuarterTurn
        | OpError::NonPositiveDistance
        | OpError::PlaneWithoutExactForm
        | OpError::FrameOutsideDecimalWindow
        | OpError::OriginNotOnPlane
        | OpError::RefDirParallelToNormal
        | OpError::DegenerateGeometry
        | OpError::FaceNotInLiveSolid
        | OpError::OriginNotOnSolid
        | OpError::MirrorNotPlanar
        | OpError::ZeroOffset
        | OpError::CollinearVertices
        | OpError::VertexNotThreePlane
        | OpError::VerticesInMixedFrames
        | OpError::ThroughFrameUndecided
        | OpError::LogHandleOutOfRange { .. } => return format!("{e:?}"),
        // These never arrive here: `build.rs` answers them before the operation-level arm —
        // a boolean rejection carries the kernel's own classification, and a dead solid is
        // a kit bug rather than something an author did.
        OpError::Boolean(_) | OpError::SolidNotLive => return format!("{e:?}"),
    };
    // Every variant with words is fieldless, so `Debug` **is** the identifier. Giving
    // words to one that carries fields means writing them into the sentence (the way
    // `sketch.rs` writes its witnesses) and taking the name for the bracket some other
    // way — `Debug` would put the whole struct in there.
    format!("{fact} [{e:?}]")
}

fn class_sentence(c: RejectClass) -> &'static str {
    match c {
        RejectClass::NotSupported => "the kernel does not build this",
        RejectClass::Impossible => "no valid solid exists for this input",
        RejectClass::SuspectedDefect => "an engine invariant broke",
    }
}

/// A human sentence for a rejection reason — **the guard's own proposition, nothing more.**
///
/// Each sentence states only what the raising guard actually verified, so it stays true for any
/// input population that ever trips the guard; it never guesses the input's nature (the class
/// stays in [`class_sentence`], where it belongs). So a reason gets a row only where one sentence
/// is true of **every** site that raises it; the rest fall back, keyed on the stable identifier
/// that travels in `what`. A `SuspectedDefect` reason (`cylinder_stages_disagree`, the planar
/// machine's own invariants) has none: the class sentence is the whole fact, and what broke is
/// for the bug report, not the author.
///
/// Unknown identifiers return `None` and the display falls back to verdict + identifier: when
/// the kernel renames or splits a reason, this table goes *less rich*, never *wrong* — and the
/// kernel-side rule is that the commit updating the census baseline on a
/// rename also revisits this table.
fn reason_sentence(what: &str) -> Option<&'static str> {
    Some(match what {
        "self_touching_result" => {
            "the result's surface touches itself, leaving material of no thickness"
        }
        "non_manifold_vertex" => "the result pinches at a single vertex",
        "non_manifold_result_edge" => {
            "the result pinches along an edge — more than two faces share it"
        }
        "oblique_cylinder_cut" => {
            "a flat face meets the cylinder at a slant — the elliptical crossing is not built yet"
        }
        "cylinder_pair_contact" => {
            "two cylinders touch or overlap — cylinder-with-cylinder booleans are not built yet"
        }
        "cylinder_gate_undecided" => {
            "the cylinder's placement could not be checked exactly against the other body"
        }
        "ruling_bound_not_yet" => {
            "the cylinder's side is divided along its length in a way that is not built yet"
        }
        "arc_bound_not_yet" => {
            "a boundary on the cylinder's side — around its seam or along a rim — is not built yet"
        }
        "no_clear_ray" => {
            "inside-or-outside could not be decided — every probe ray from the region's corners grazes a boundary"
        }
        "witness_not_rational" => {
            "an exact value this step needed does not fit the kernel's exact numbers — the shapes' coordinates or planes carry too many digits to combine exactly"
        }
        "vertex_names_absent_surface" => {
            "a corner of the result is defined by a surface the result keeps no face on — the corner cannot be re-derived from the solid itself"
        }
        "precision_budget" => {
            "the motion history needs more precision than the judging budget holds"
        }
        _ => return None,
    })
}
