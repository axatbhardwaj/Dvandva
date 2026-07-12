//! `reverify` — the fail-closed delta re-verification decision core (chunk A1).
//!
//! Option B (delta-reverification-2): the ONLY carry-eligible unit is a
//! mechanical `test_creation` `subagent_track`; cross-review / deep-review /
//! risk-angle tracks and every `verification_matrix` row are unbounded and
//! never carry, and the terminal `done`-gate is unchanged. [`decide`] returns
//! [`Decision::Carry`] only when a bounded, engine-derivable closure is provably
//! untouched since a real on-cycle origin pass; every uncertainty returns
//! [`Decision::ReRun`] (the fail-closed default).
//!
//! CLOSURE COMPLETENESS IS NOT PROVEN (RR-8). [`derive_covered_closure`] is a
//! best-effort transitive walk over the *declared* `work_split` paths; it can
//! under- or over-approximate the true input footprint of a unit, and this
//! module makes NO claim that the derived closure is complete (the sound-RTS
//! ceiling). Correctness does NOT rest on closure completeness. It rests on two
//! guarantees this module never weakens: (a) reviews never carry, so every
//! adversarial gate re-runs at full depth on every pass; and (b) the terminal
//! `done`-gate is unchanged and re-verifies every `verification_matrix` row
//! fresh against `implementation_family_anchor`. Carry is a best-effort
//! INTERMEDIATE optimization honored at exactly one gate
//! (`test_creation_to_cross_review_ok`); it can only skip the mechanical replay
//! of a unit whose declared closure is bounded, on-cycle, and shows no git diff
//! from a real origin pass — never a review, never a terminal approval, never a
//! global / `covers:["*"]` / non-derivable unit.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;

use crate::provenance;
use crate::util;

/// The re-verification decision for one downstream unit.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// The unit's declared inputs are provably untouched — skip mechanical
    /// replay (best-effort intermediate; never a review, never terminal).
    Carry,
    /// Re-run the unit at full depth (the fail-closed default).
    ReRun,
}

/// Fail-closed guards (a)-(f), evaluated in order. Absent
/// `carried_from_checkpoint` ⇒ [`Decision::ReRun`] (byte-identical to today's
/// fresh/anchor behavior; no first traversal ever carries).
///
/// `kind` identifies `unit` for the kind-qualified origin lookup (SP-2) — one
/// of `"verification_matrix_row"` / `"subagent_track"`. Under Option B same-id
/// carry there is no `carried_from_id` field: the origin is resolved by the
/// candidate's OWN `id` at `carried_from_checkpoint`.
///
/// `decide` is generic over the unit kind; A2 honors a returned
/// [`Decision::Carry`] only at the `test_creation` gate.
pub fn decide(
    baton: &Value,
    dir: &Path,
    unit: &Value,
    kind: &str,
    current_ckpt: i64,
    current_phase: &str,
    repo_root: &Path,
) -> Decision {
    // first-pass / legacy: no carry claim ⇒ RE_RUN (byte-identical to today's
    // fresh/anchor behavior). No first traversal ever carries.
    let Some(origin) = field(unit, "carried_from_checkpoint").and_then(json_int) else {
        return Decision::ReRun;
    };
    // guard (a): the closure must be engine-derivable. `derive_covered_closure`
    // never returns `Some` of an EMPTY set (its own final `Some(paths)` is only
    // reached after at least one seed chunk contributed a path), so no separate
    // emptiness re-check is needed here.
    let Some(closure) = derive_covered_closure(baton, unit) else {
        return Decision::ReRun;
    };
    // guard (b): not a terminal-approval unit, not a global / unbounded unit.
    if is_terminal_approval_unit(unit) || is_global_unit(unit) {
        return Decision::ReRun;
    }
    // guard (c): no OPEN finding intersects the closure. A pathless open finding
    // is treated as global and blocks any carry (fail-closed).
    if open_finding_touches_closure(baton, &closure) {
        return Decision::ReRun;
    }
    // guard (d): provenance — a real, on-cycle, non-carried origin pass resolved
    // by (kind, id) [SP-2 kind-qualified]. Option B same-id carry: the origin is
    // located by the candidate's OWN id at carried_from_checkpoint.
    let unit_id = str_field(unit, "id");
    if unit_id.is_empty() {
        return Decision::ReRun;
    }
    // Read the origin snapshot FIRST (its absence is the fail-closed default),
    // THEN require it to sit on the current phase-cycle lineage. Ordering the
    // read before the ancestry check keeps BOTH arms reachable in coverage: a
    // missing snapshot is rejected right here, an off-lineage-but-readable
    // snapshot by the ancestry guard immediately below.
    let Some(snap) = provenance::read_origin_snapshot(dir, origin) else {
        return Decision::ReRun;
    };
    if !provenance::on_current_cycle_ancestry(dir, baton, current_ckpt, current_phase, origin) {
        return Decision::ReRun;
    }
    let Some(orig_unit) = provenance::find_unit(&snap, kind, &unit_id) else {
        return Decision::ReRun;
    };
    if !provenance::was_pass(&orig_unit) {
        return Decision::ReRun;
    }
    // no carry-of-a-carry: the origin must itself be a direct execution.
    if field(&orig_unit, "carried_from_checkpoint").is_some() {
        return Decision::ReRun;
    }
    // CR21-F3: for the honored `subagent_track` carry (the ONLY kind whose
    // Carry decision A2 acts on), the origin must be a COMPLETE qualifying
    // DIRECT test-creation execution — not merely `was_pass`. A
    // `verification_matrix_row` origin is never honored (reviews / the terminal
    // done-gate never consult `decide`), so its weaker legacy shape is left
    // untouched here.
    if kind == "subagent_track" && !origin_direct_test_creation_shape_ok(&orig_unit) {
        return Decision::ReRun;
    }
    // guard (e): closure-membership binding (SP-3) then anti-substitution
    // (SP-1). The sorted CURRENT-derived closure must equal BOTH the origin
    // snapshot's stored covered_paths AND the candidate's own covered_paths,
    // checked BEFORE any diff — this catches drift when a covers_chunks chunk's
    // depends_on / paths changed since the origin (silently expanding or
    // shrinking what should count as touched) even though the candidate's stale
    // claim still agrees with the origin's stale claim. Then the candidate's
    // stamped triple must byte-equal the ORIGIN SNAPSHOT's stored triple (never
    // the candidate's own claim, never current HEAD), and the ORIGIN SNAPSHOT
    // anchor must show no diff over the closure.
    let mut paths: Vec<String> = closure.iter().cloned().collect();
    paths.sort();
    let mut origin_paths = str_vec_field(&orig_unit, "covered_paths");
    origin_paths.sort();
    let mut cand_paths = str_vec_field(unit, "covered_paths");
    cand_paths.sort();
    if paths != origin_paths || paths != cand_paths {
        return Decision::ReRun;
    }
    let origin_digest = str_field(&orig_unit, "covered_input_digest");
    if origin_digest.is_empty()
        || str_field(unit, "covered_input_digest") != origin_digest
        || str_field(unit, "digest_algo") != str_field(&orig_unit, "digest_algo")
    {
        return Decision::ReRun;
    }
    if !provenance::commit_anchor_valid(repo_root, &origin_digest, &paths) {
        return Decision::ReRun;
    }
    // guard (f): carry_reason must be non-blank.
    if str_field(unit, "carry_reason").trim().is_empty() {
        return Decision::ReRun;
    }
    Decision::Carry
}

/// The engine-derived transitive covered closure for `unit`.
///
/// Seeds from `covers_chunks` (real `work_split` chunk ids — free-text `covers`
/// is NOT consulted here), unions each chunk's declared `write_paths` +
/// `read_paths`, and walks `depends_on` + `conflict_group` transitively.
/// Returns `None` (unbounded — never carries) on: empty/absent seeds, a dangling
/// seed id, a reached chunk with no declared paths, or an unnormalizable path
/// (absolute, containing `..`, or containing a glob metacharacter). Every
/// returned path is a validated repo-relative file path.
pub fn derive_covered_closure(baton: &Value, unit: &Value) -> Option<BTreeSet<String>> {
    let seeds = str_vec_field(unit, "covers_chunks");
    if seeds.is_empty() {
        return None; // empty / absent seeds ⇒ unbounded
    }
    let chunks = work_split_chunks(baton);
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<String> = seeds;
    let mut paths: BTreeSet<String> = BTreeSet::new();
    while let Some(id) = queue.pop() {
        if !visited.insert(id.clone()) {
            continue;
        }
        let Some(chunk) = find_chunk(&chunks, &id) else {
            return None; // dangling seed ⇒ unbounded
        };
        let declared = declared_paths(chunk);
        if declared.is_empty() {
            return None; // a reached chunk with no declared paths ⇒ unbounded
        }
        for p in declared {
            let Some(norm) = normalize_path(&p) else {
                return None; // absolute / `..` / glob ⇒ unbounded
            };
            paths.insert(norm);
        }
        for dep in str_vec_field(chunk, "depends_on") {
            queue.push(dep);
        }
        let cg = str_field(chunk, "conflict_group");
        if !cg.trim().is_empty() {
            for other in chunks.iter().copied() {
                if str_field(other, "conflict_group") == cg {
                    queue.push(str_field(other, "id"));
                }
            }
        }
    }
    // `seeds` was checked non-empty at entry, and every processed chunk either
    // returns `None` early or inserts at least one validated path, so `paths`
    // is guaranteed non-empty here — no emptiness re-check is needed.
    Some(paths)
}

// ===========================================================================
// Closure helpers
// ===========================================================================

/// A chunk's declared footprint: `write_paths` ∪ `read_paths`, order-preserving
/// and de-duplicated. A chunk declaring its footprint only via the generic
/// `paths` field (and neither `write_paths` nor `read_paths`) is treated as
/// having no declared paths — the fail-closed direction.
fn declared_paths(chunk: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in str_vec_field(chunk, "write_paths")
        .into_iter()
        .chain(str_vec_field(chunk, "read_paths"))
    {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Validate a declared path as a repo-relative regular file path: reject
/// absolute paths, any `.`/`..`/empty segment or `//` (via
/// [`util::is_safe_rel_path`]), and any glob metacharacter. `None` means the
/// closure is unnormalizable ⇒ unbounded ⇒ never carries.
fn normalize_path(p: &str) -> Option<String> {
    if !util::is_safe_rel_path(p) {
        return None;
    }
    if p.contains('*') || p.contains('?') || p.contains('[') || p.contains(']') {
        return None;
    }
    Some(p.to_string())
}

// ===========================================================================
// Guard predicates
// ===========================================================================

/// A terminal-approval unit (never carries): an explicit `terminal: true`
/// marker, or a `gate` / `phase` / `kind` / `track` / `name` field equal
/// (case-insensitive) to a terminal-approval token.
fn is_terminal_approval_unit(unit: &Value) -> bool {
    if bool_field(unit, "terminal") {
        return true;
    }
    const TERMINAL_TOKENS: [&str; 5] = [
        "done",
        "termination_review",
        "termination",
        "terminal",
        "final_approval",
    ];
    ["gate", "phase", "kind", "track", "name"]
        .iter()
        .any(|key| {
            let v = str_field(unit, key).trim().to_ascii_lowercase();
            TERMINAL_TOKENS.contains(&v.as_str())
        })
}

/// A global / unbounded unit (never carries): an explicit `global: true` marker,
/// or a `covers` / `covers_chunks` wildcard (`"*"`).
fn is_global_unit(unit: &Value) -> bool {
    if bool_field(unit, "global") {
        return true;
    }
    str_vec_field(unit, "covers").iter().any(|c| c == "*")
        || str_vec_field(unit, "covers_chunks")
            .iter()
            .any(|c| c == "*")
}

/// The fixed `digest_algo` value the engine stamps on a bounded direct-executed
/// test-creation covered-input digest (mirrors `write.rs::GIT_COVERS_DIFF_ALGO`;
/// reimplemented locally rather than promoting the private constant).
const GIT_COVERS_DIFF_ALGO: &str = "git-covers-diff-v1";

/// CR21-F3: whether `orig_unit` is a COMPLETE qualifying DIRECT test-creation
/// execution eligible to back a same-id carry — not merely `was_pass`. Requires
/// `status=completed`, a passing result, the test-creation `track` family, an
/// `owner`/`owner_role` present, non-empty `outputs` + `evidence_refs`, and the
/// exact `git-covers-diff-v1` stamp algo. (The no-carry-of-a-carry check lives
/// in `decide` so the "origin is a direct execution" invariant is shared with
/// the `verification_matrix_row` kind.) Any missing piece fails closed.
fn origin_direct_test_creation_shape_ok(orig_unit: &Value) -> bool {
    str_field(orig_unit, "status") == "completed"
        && provenance::was_pass(orig_unit)
        && str_field(orig_unit, "track") == "test-creation"
        && !str_field(orig_unit, "owner").trim().is_empty()
        && !str_field(orig_unit, "owner_role").trim().is_empty()
        && !str_vec_field(orig_unit, "outputs").is_empty()
        && !str_vec_field(orig_unit, "evidence_refs").is_empty()
        && str_field(orig_unit, "digest_algo") == GIT_COVERS_DIFF_ALGO
}

/// True iff any OPEN finding intersects `closure`. A finding is open per the
/// tolerant [`util::is_open_finding_status`] token set (fail-safe). A finding
/// with no declared `paths` array (or a bare-string finding) is PATHLESS and
/// treated as global — it blocks any carry. A finding with paths blocks only
/// when one of its paths prefix-overlaps a closure path.
fn open_finding_touches_closure(baton: &Value, closure: &BTreeSet<String>) -> bool {
    for finding in arr_field(baton, "findings") {
        let (open, fpaths) = match finding {
            Value::Object(_) => {
                let status = str_field(finding, "status");
                (
                    util::is_open_finding_status(Some(status.as_str())),
                    str_vec_field(finding, "paths"),
                )
            }
            // A bare-string finding (or any non-object) carries no status and no
            // paths: treat as an open, pathless (global) finding.
            _ => (true, Vec::new()),
        };
        if !open {
            continue;
        }
        if fpaths.is_empty() {
            return true; // pathless open finding ⇒ global block
        }
        if fpaths
            .iter()
            .any(|fp| closure.iter().any(|cp| path_overlap(fp, cp)))
        {
            return true;
        }
    }
    false
}

/// Prefix-overlap of two repo-relative paths (mirrors `write.rs::path_overlap`;
/// reimplemented locally rather than promoting the private helper).
fn path_overlap(left: &str, right: &str) -> bool {
    left == right
        || left.starts_with(&format!("{right}/"))
        || right.starts_with(&format!("{left}/"))
}

// ===========================================================================
// Field / jq helpers (local — mirror write.rs idioms; no promotion of privates)
// ===========================================================================

fn field<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.as_object()?.get(key)
}

/// jq `.field // ""` -r semantics: coalesce null/false to absent, then render.
fn str_field(v: &Value, key: &str) -> String {
    match util::coalesce(field(v, key)) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// jq `.field // false` compared to boolean `true`.
fn bool_field(v: &Value, key: &str) -> bool {
    matches!(util::coalesce(field(v, key)), Some(Value::Bool(true)))
}

/// serde integer read (tolerates the `arbitrary_precision` feature).
fn json_int(value: &Value) -> Option<i64> {
    value.as_i64()
}

/// A string array field, rendering non-string entries and dropping nulls (jq
/// `.field[]? | tostring`).
fn str_vec_field(v: &Value, key: &str) -> Vec<String> {
    match field(v, key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|it| match it {
                Value::String(s) => Some(s.clone()),
                Value::Null => None,
                other => Some(other.to_string()),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn arr_field<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    match field(v, key) {
        Some(Value::Array(items)) => items.as_slice(),
        _ => &[],
    }
}

/// The `work_split` chunks, whether stored as an array or an id-keyed object
/// (jq `.work_split[]?`).
fn work_split_chunks(baton: &Value) -> Vec<&Value> {
    match field(baton, "work_split") {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(Value::Object(map)) => map.values().collect(),
        _ => Vec::new(),
    }
}

fn find_chunk<'a>(chunks: &[&'a Value], id: &str) -> Option<&'a Value> {
    chunks.iter().copied().find(|c| str_field(c, "id") == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // A minimal baton whose work_split exercises seed resolution, declared
    // paths, depends_on, and conflict_group edges.
    fn baton_fixture() -> Value {
        json!({
            "work_split": [
                {
                    "id": "A",
                    "write_paths": ["src/a.rs"],
                    "read_paths": ["src/shared.rs"],
                    "depends_on": ["B"],
                    "conflict_group": "grp"
                },
                {
                    "id": "B",
                    "write_paths": ["src/b.rs"],
                    "read_paths": [],
                    "depends_on": [],
                    "conflict_group": "grp"
                },
                {
                    "id": "C",
                    "write_paths": ["src/c.rs"],
                    "read_paths": [],
                    "depends_on": [],
                    "conflict_group": "grp"
                }
            ],
            "findings": []
        })
    }

    fn nowhere() -> &'static Path {
        Path::new("/nonexistent-reverify-test")
    }

    // ---- decide(): early guards that never reach provenance -------------

    #[test]
    fn decide_absent_claim_is_rerun() {
        let baton = baton_fixture();
        let unit = json!({ "id": "t1", "covers_chunks": ["A"] });
        assert_eq!(
            decide(
                &baton,
                nowhere(),
                &unit,
                "subagent_track",
                42,
                "1",
                nowhere()
            ),
            Decision::ReRun
        );
    }

    #[test]
    fn terminal_gate_never_carries() {
        let baton = baton_fixture();
        let unit = json!({
            "id": "t1",
            "carried_from_checkpoint": 5,
            "covers_chunks": ["A"],
            "gate": "done"
        });
        assert_eq!(
            decide(
                &baton,
                nowhere(),
                &unit,
                "verification_matrix_row",
                42,
                "1",
                nowhere()
            ),
            Decision::ReRun
        );
    }

    #[test]
    fn global_unit_never_carries() {
        let baton = baton_fixture();
        let global_flag = json!({
            "id": "t1",
            "carried_from_checkpoint": 5,
            "covers_chunks": ["A"],
            "global": true
        });
        assert_eq!(
            decide(
                &baton,
                nowhere(),
                &global_flag,
                "subagent_track",
                42,
                "1",
                nowhere()
            ),
            Decision::ReRun
        );
        let wildcard = json!({
            "id": "t1",
            "carried_from_checkpoint": 5,
            "covers_chunks": ["A"],
            "covers": ["*"]
        });
        assert_eq!(
            decide(
                &baton,
                nowhere(),
                &wildcard,
                "subagent_track",
                42,
                "1",
                nowhere()
            ),
            Decision::ReRun
        );
    }

    #[test]
    fn open_finding_blocks_carry() {
        let mut baton = baton_fixture();
        baton["findings"] = json!([
            { "id": "f1", "status": "open", "paths": ["src/a.rs"] }
        ]);
        let unit = json!({
            "id": "t1",
            "carried_from_checkpoint": 5,
            "covers_chunks": ["A"]
        });
        assert_eq!(
            decide(
                &baton,
                nowhere(),
                &unit,
                "subagent_track",
                42,
                "1",
                nowhere()
            ),
            Decision::ReRun
        );
    }

    #[test]
    fn pathless_open_finding_blocks_carry() {
        let mut baton = baton_fixture();
        baton["findings"] = json!([{ "id": "f1", "status": "open" }]);
        let unit = json!({
            "id": "t1",
            "carried_from_checkpoint": 5,
            "covers_chunks": ["A"]
        });
        assert_eq!(
            decide(
                &baton,
                nowhere(),
                &unit,
                "subagent_track",
                42,
                "1",
                nowhere()
            ),
            Decision::ReRun
        );
    }

    #[test]
    fn underivable_closure_is_rerun() {
        // A carry claim over a dangling seed ⇒ guard (a) fails ⇒ ReRun.
        let baton = baton_fixture();
        let unit = json!({
            "id": "t1",
            "carried_from_checkpoint": 5,
            "covers_chunks": ["nope"]
        });
        assert_eq!(
            decide(
                &baton,
                nowhere(),
                &unit,
                "subagent_track",
                42,
                "1",
                nowhere()
            ),
            Decision::ReRun
        );
    }

    // ---- derive_covered_closure(): pure transitive walk -----------------

    #[test]
    fn derive_closure_none_on_empty() {
        let baton = baton_fixture();
        let unit = json!({ "id": "t1" }); // no covers_chunks
        assert_eq!(derive_covered_closure(&baton, &unit), None);
        let empty = json!({ "id": "t1", "covers_chunks": [] });
        assert_eq!(derive_covered_closure(&baton, &empty), None);
    }

    #[test]
    fn derive_closure_none_on_dangling_seed() {
        let baton = baton_fixture();
        let unit = json!({ "id": "t1", "covers_chunks": ["ghost"] });
        assert_eq!(derive_covered_closure(&baton, &unit), None);
    }

    #[test]
    fn derive_closure_none_on_no_declared_paths() {
        let baton = json!({
            "work_split": [
                { "id": "A", "write_paths": [], "read_paths": [], "paths": ["src/a.rs"] }
            ]
        });
        // Declares its footprint only via `paths`, not write/read ⇒ no declared
        // paths ⇒ unbounded ⇒ None.
        let unit = json!({ "id": "t1", "covers_chunks": ["A"] });
        assert_eq!(derive_covered_closure(&baton, &unit), None);
    }

    #[test]
    fn derive_closure_rejects_unnormalizable() {
        for bad in [
            json!(["/abs/x.rs"]),
            json!(["../up.rs"]),
            json!(["src/*.rs"]),
        ] {
            let baton = json!({
                "work_split": [{ "id": "A", "write_paths": bad, "read_paths": [] }]
            });
            let unit = json!({ "id": "t1", "covers_chunks": ["A"] });
            assert_eq!(
                derive_covered_closure(&baton, &unit),
                None,
                "expected None for unnormalizable path"
            );
        }
    }

    #[test]
    fn derive_closure_walks_depends_on_and_conflict_group() {
        let baton = baton_fixture();
        // Seed A -> depends_on B; A/B/C share conflict_group "grp" ⇒ all pulled.
        let unit = json!({ "id": "t1", "covers_chunks": ["A"] });
        let closure = derive_covered_closure(&baton, &unit).expect("bounded closure");
        let expected: BTreeSet<String> = ["src/a.rs", "src/shared.rs", "src/b.rs", "src/c.rs"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(closure, expected);
    }

    // ---- guard predicates in isolation ----------------------------------

    #[test]
    fn is_global_unit_variants() {
        assert!(is_global_unit(&json!({ "global": true })));
        assert!(is_global_unit(&json!({ "covers": ["*"] })));
        assert!(is_global_unit(&json!({ "covers_chunks": ["*"] })));
        assert!(!is_global_unit(&json!({ "covers_chunks": ["A"] })));
        assert!(!is_global_unit(&json!({ "global": false })));
    }

    #[test]
    fn is_terminal_approval_unit_variants() {
        assert!(is_terminal_approval_unit(&json!({ "terminal": true })));
        assert!(is_terminal_approval_unit(&json!({ "gate": "done" })));
        assert!(is_terminal_approval_unit(
            &json!({ "phase": "termination_review" })
        ));
        assert!(!is_terminal_approval_unit(&json!({ "phase": "1" })));
        assert!(!is_terminal_approval_unit(
            &json!({ "gate": "test_creation_to_cross_review_ok" })
        ));
    }

    #[test]
    fn open_finding_touches_closure_branches() {
        let closure: BTreeSet<String> = ["src/a.rs".to_string()].into_iter().collect();
        // closed finding ⇒ no block
        let closed =
            json!({ "findings": [{ "id": "f", "status": "resolved", "paths": ["src/a.rs"] }] });
        assert!(!open_finding_touches_closure(&closed, &closure));
        // open finding, non-overlapping path ⇒ no block
        let disjoint =
            json!({ "findings": [{ "id": "f", "status": "open", "paths": ["src/z.rs"] }] });
        assert!(!open_finding_touches_closure(&disjoint, &closure));
        // open finding, overlapping path ⇒ block
        let hit = json!({ "findings": [{ "id": "f", "status": "open", "paths": ["src/a.rs"] }] });
        assert!(open_finding_touches_closure(&hit, &closure));
        // pathless open finding ⇒ global block
        let pathless = json!({ "findings": [{ "id": "f", "status": "open" }] });
        assert!(open_finding_touches_closure(&pathless, &closure));
        // bare-string finding ⇒ open + pathless ⇒ global block
        let bare = json!({ "findings": ["something is wrong"] });
        assert!(open_finding_touches_closure(&bare, &closure));
        // no findings ⇒ no block
        assert!(!open_finding_touches_closure(
            &json!({ "findings": [] }),
            &closure
        ));
    }

    #[test]
    fn path_overlap_semantics() {
        assert!(path_overlap("src/a.rs", "src/a.rs"));
        assert!(path_overlap("src", "src/a.rs"));
        assert!(path_overlap("src/a.rs", "src"));
        assert!(!path_overlap("src/a.rs", "src/b.rs"));
        assert!(!path_overlap("srcx", "src"));
    }
}
