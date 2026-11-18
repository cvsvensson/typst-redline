//! Which blocks of an old and a new file correspond, as the steps that
//! write the new file with the changes marked.

use std::ops::Range;

use similar::{Algorithm, DiffOp, capture_diff_slices};

use crate::syntax::{Block, Doc, Kind, most_similar, similarity, words};

/// One step of writing the new file, by block index.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Step {
    /// A new block written as it is.
    Keep(usize),
    /// An old block and the new block it changed into.
    Pair(usize, usize),
    Del(usize),
    Ins(usize),
}

/// The steps that write `new` with the changes against `old`: blocks that
/// are the same are kept, the others go to [`region`].
pub fn align(old: &Doc, new: &Doc) -> Vec<Step> {
    let mut steps = vec![];
    for op in capture_diff_slices(Algorithm::Patience, &old.keys(), &new.keys()) {
        match op {
            DiffOp::Equal { .. } => steps.extend(op.new_range().map(Step::Keep)),
            _ => region(&old.blocks, &new.blocks, op.old_range(), op.new_range(), &mut steps),
        }
    }
    steps
}

/// A run of differing blocks. Similar blocks of the same kind, marker and
/// label are paired and diffed word by word; around each pair, deletions
/// come first. Across a heading that replaces another, blocks are paired
/// only if they are much alike: the section is likely new, unless it was
/// renamed. Pairing looks a bounded number of blocks ahead, so that its cost
/// stays linear in the length of the run.
fn region(old: &[Block], new: &[Block], or: Range<usize>, nr: Range<usize>, steps: &mut Vec<Step>) {
    let owords: Vec<Vec<String>> = old[or.clone()].iter().map(|b| words(&b.toks())).collect();
    let mut p = or.start;
    let mut pending: Vec<usize> = vec![];
    for n in nr {
        let nb = &new[n];
        let nwords = words(&nb.toks());
        let new_section = nb.kind != Kind::Heading && pending.iter().any(|&j| new[j].kind == Kind::Heading);
        let end = or.end.min(p + 32);
        let old_heading = (p..end).find(|&q| old[q].kind == Kind::Heading).unwrap_or(end);
        let best = most_similar(
            (p..end)
                .filter(|&q| old[q].kind == nb.kind && old[q].frame() == nb.frame())
                .map(|q| (q, similarity(&owords[q - or.start], &nwords)))
                .filter(|&(q, s)| !new_section || q < old_heading || s >= 0.7),
        );
        match best {
            Some(q) => {
                unpaired(old, new, p..q, &pending, steps);
                pending.clear();
                steps.push(Step::Pair(q, n));
                p = q + 1;
            }
            None => pending.push(n),
        }
    }
    unpaired(old, new, p..or.end, &pending, steps);
}

/// Deleted and inserted blocks in turn, so that a replaced block is followed
/// by its replacement; but leading setup code of the new version goes first
/// (deleted content may use its definitions), and from the first heading on
/// either side, sections take turns whole. A page or column break inserted
/// or deleted with marked content is part of that change: it is kept, or
/// not written.
fn unpaired(old: &[Block], new: &[Block], or: Range<usize>, nidx: &[usize], steps: &mut Vec<Step>) {
    let ob = &old[or.clone()];
    let ns = nidx
        .iter()
        .position(|&j| new[j].kind != Kind::Setup)
        .unwrap_or(nidx.len());
    let oh = ob.iter().position(|b| b.kind == Kind::Heading).unwrap_or(ob.len());
    let nh = nidx
        .iter()
        .position(|&j| new[j].kind == Kind::Heading)
        .unwrap_or(nidx.len());
    let old_content = ob.iter().any(|b| b.kind != Kind::Setup);
    let new_content = ns < nidx.len();
    let insert = |steps: &mut Vec<Step>, j: usize| {
        steps.push(if new[j].is_break() && new_content {
            Step::Keep(j)
        } else {
            Step::Ins(j)
        })
    };
    let delete = |steps: &mut Vec<Step>, q: usize| {
        if !(old[q].is_break() && old_content) {
            steps.push(Step::Del(q))
        }
    };
    nidx[..ns].iter().for_each(|&j| insert(steps, j));
    for k in 0..oh.max(nh - ns) {
        if k < oh {
            delete(steps, or.start + k);
        }
        if let Some(&j) = nidx[ns..nh].get(k) {
            insert(steps, j);
        }
    }
    let sections = |kinds: Vec<Kind>, mut start: usize| -> Vec<Range<usize>> {
        kinds
            .chunk_by(|_, k| *k != Kind::Heading)
            .map(|s| {
                start += s.len();
                start - s.len()..start
            })
            .collect()
    };
    let osecs = sections(ob[oh..].iter().map(|b| b.kind).collect(), or.start + oh);
    let nsecs = sections(nidx[nh..].iter().map(|&j| new[j].kind).collect(), nh);
    for k in 0..osecs.len().max(nsecs.len()) {
        osecs
            .get(k)
            .into_iter()
            .flat_map(|s| s.clone())
            .for_each(|q| delete(steps, q));
        nsecs
            .get(k)
            .into_iter()
            .flat_map(|s| s.clone())
            .for_each(|i| insert(steps, nidx[i]));
    }
}
