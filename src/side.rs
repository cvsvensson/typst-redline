//! The side-by-side layout: which pages of the old and the new version show
//! the same changes, and the composite document that puts them next to each
//! other.

use std::collections::BTreeSet;

use crate::mark::{segments, typst_str};
use crate::{Change, ChangeKind, PREAMBLE_PATH};

/// A change as laid out in one version: an insertion or a deletion, an
/// identity that is the same in both versions, and its page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub ins: bool,
    pub id: u64,
    pub page: usize,
}

/// Pages of the two versions that hold the same changes, and the pages of
/// earlier steps of the same slide, which are not shown.
#[derive(Debug, Default, PartialEq)]
pub struct Pair {
    pub old: Vec<usize>,
    pub new: Vec<usize>,
    pub steps_old: Vec<usize>,
    pub steps_new: Vec<usize>,
}

/// One version as laid out for the side-by-side layout: `pdf` is its
/// root-relative path, `pages` its number of pages, `unbarred` the pages
/// with changes but no change bars.
pub struct Side<'a> {
    pub label: &'a str,
    pub pdf: &'a str,
    pub pages: usize,
    pub unbarred: &'a [usize],
    pub changes: &'a [Placed],
}

/// The pages that hold the same changes, in the order of the new version,
/// and the changes found in one version only (`true` for the old one).
/// Consecutive pairs with the same changes are steps of one slide; only the
/// last is kept.
pub fn pair_pages(old: &[Placed], new: &[Placed]) -> (Vec<Pair>, Vec<(bool, Placed)>) {
    let mut groups: Vec<(BTreeSet<usize>, BTreeSet<usize>)> = vec![];
    let mut unpaired = vec![];
    for ins in [false, true] {
        let (o, n): (Vec<Placed>, Vec<Placed>) = (
            old.iter().filter(|c| c.ins == ins).copied().collect(),
            new.iter().filter(|c| c.ins == ins).copied().collect(),
        );
        let ids = |s: &[Placed]| s.iter().map(|c| c.id).collect::<Vec<_>>();
        // Each version is marked by its own comparison, which can write a
        // change differently (a deleted list item carries its marker) or
        // split it differently. Changes between the same matched neighbours
        // are paired in order if they are as many in both versions, and all
        // with all otherwise.
        for seg in segments(&ids(&o), &ids(&n)) {
            let (os, ns) = (&o[seg.old], &n[seg.new]);
            if os.is_empty() || ns.is_empty() {
                for (in_old, c) in os.iter().map(|c| (true, c)).chain(ns.iter().map(|c| (false, c))) {
                    unpaired.push((in_old, *c));
                    let page = Some(c.page);
                    join(&mut groups, page.filter(|_| in_old), page.filter(|_| !in_old));
                }
            }
            for (i, c) in os.iter().enumerate() {
                for (j, d) in ns.iter().enumerate() {
                    if os.len() != ns.len() || i == j {
                        join(&mut groups, Some(c.page), Some(d.page));
                    }
                }
            }
        }
    }
    groups.sort_by_key(|(o, n)| (n.first().copied().unwrap_or(usize::MAX), o.first().copied()));

    let changes = |side: &[Placed], pages: &BTreeSet<usize>| {
        side.iter()
            .filter(|c| pages.contains(&c.page))
            .map(|c| (c.ins, c.id))
            .collect::<Vec<_>>()
    };
    let follows = |a: &BTreeSet<usize>, b: &BTreeSet<usize>| a.last().map(|p| p + 1) == b.first().copied();
    let mut pairs: Vec<Pair> = vec![];
    for (i, (o, n)) in groups.iter().enumerate() {
        let mut pair = Pair {
            old: o.iter().copied().collect(),
            new: n.iter().copied().collect(),
            ..Pair::default()
        };
        if i > 0 {
            let (po, pn) = &groups[i - 1];
            if follows(po, o)
                && follows(pn, n)
                && changes(old, po) == changes(old, o)
                && changes(new, pn) == changes(new, n)
            {
                let prev = pairs.pop().unwrap();
                pair.steps_old = [prev.steps_old, prev.old].concat();
                pair.steps_new = [prev.steps_new, prev.new].concat();
            }
        }
        pairs.push(pair);
    }
    (pairs, unpaired)
}

/// Put old page `o` and new page `n` into one group, merging the groups
/// that hold either.
fn join(groups: &mut Vec<(BTreeSet<usize>, BTreeSet<usize>)>, o: Option<usize>, n: Option<usize>) {
    let mut group: (BTreeSet<usize>, BTreeSet<usize>) = (o.into_iter().collect(), n.into_iter().collect());
    groups.retain(|(go, gn)| {
        let joined = o.is_some_and(|o| go.contains(&o)) || n.is_some_and(|n| gn.contains(&n));
        if joined {
            group.0.extend(go);
            group.1.extend(gn);
        }
        !joined
    });
    groups.push(group);
}

/// The source of the composite document: a call to the preamble's
/// `redline-side-by-side` with the paired pages, the changes found in one
/// version only and the changes that are not marked.
pub fn composite(old: &Side, new: &Side, changes: &[Change]) -> String {
    let (pairs, unpaired) = pair_pages(old.changes, new.changes);
    let list = |v: &[usize]| format!("({})", v.iter().map(|p| format!("{p},")).collect::<String>());
    let side = |s: &Side| {
        format!(
            "(label: {}, pdf: {}, pages: {}, unbarred: {})",
            typst_str(s.label),
            typst_str(s.pdf),
            s.pages,
            list(s.unbarred)
        )
    };
    let mut out = format!(
        "#import \"/{PREAMBLE_PATH}\": redline-side-by-side\n#redline-side-by-side((\n  old: {},\n  new: {},\n  pairs: (\n",
        side(old),
        side(new)
    );
    for p in &pairs {
        out.push_str(&format!(
            "    (old: {}, new: {}, steps-old: {}, steps-new: {}),\n",
            list(&p.old),
            list(&p.new),
            list(&p.steps_old),
            list(&p.steps_new)
        ));
    }
    out.push_str("  ),\n  unpaired: (\n");
    for (in_old, c) in &unpaired {
        let side = if *in_old { "old" } else { "new" };
        out.push_str(&format!("    (side: \"{side}\", ins: {}, page: {}),\n", c.ins, c.page));
    }
    out.push_str("  ),\n  unmarked: (\n");
    for c in changes.iter().filter(|c| c.kind == ChangeKind::Unmarked) {
        out.push_str(&format!(
            "    (file: {}, text: {}),\n",
            typst_str(&c.file),
            typst_str(&c.text)
        ));
    }
    out.push_str("  ),\n))\n");
    out
}
