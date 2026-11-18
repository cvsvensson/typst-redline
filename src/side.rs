//! The side-by-side layout: which pages of the old and the new version show
//! the same changes, and the composite document that puts them next to each
//! other.

use std::collections::{BTreeSet, HashMap};

use crate::mark::{segments, typst_str};
use crate::{Bib, Change, ChangeKind, History, PREAMBLE_PATH};

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
    pub headings: &'a [Anchor],
}

/// A heading as laid out in one version: an identity that is the same in
/// both versions if its text is, and its page.
pub type Anchor = (u64, usize);

/// The pages that hold the same changes, in the order of the new version,
/// and the changes found in one version only (`true` for the old one).
/// A change is on a page of the version that has its content, and on the
/// other at the place the content would be. That place's page is not a
/// counterpart if its headings are on other pages in the version with the
/// content: the content fills pages of its own. Consecutive pairs with the
/// same changes are steps of one slide; only the last is kept.
pub fn pair_pages(old: &[Placed], new: &[Placed], anchors: (&[Anchor], &[Anchor])) -> (Vec<Pair>, Vec<(bool, Placed)>) {
    let same: Vec<(usize, usize)> = same_headings(anchors)
        .into_iter()
        .map(|(i, j)| (anchors.0[i].1, anchors.1[j].1))
        .collect();
    let elsewhere = |in_old: bool, page: usize, other: usize| {
        let there = same
            .iter()
            .filter(|(o, n)| if in_old { *o == page } else { *n == page });
        let mut there = there.map(|(o, n)| if in_old { *n } else { *o }).peekable();
        there.peek().is_some() && !there.any(|p| p == other)
    };
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
                        // An insertion's content is in the new version.
                        let (absent, real) = if ins { (c.page, d.page) } else { (d.page, c.page) };
                        let alone = elsewhere(ins, absent, real);
                        join(
                            &mut groups,
                            (!(alone && ins)).then_some(c.page),
                            (!(alone && !ins)).then_some(d.page),
                        );
                    }
                }
            }
        }
    }
    // A group with old pages only goes after the new pages that the old
    // pages before it are paired with.
    let known: Vec<(usize, usize)> = groups
        .iter()
        .flat_map(|(o, n)| o.iter().flat_map(|&p| n.iter().map(move |&q| (p, q))))
        .chain(same.iter().copied())
        .collect();
    let after = |o: usize| {
        known
            .iter()
            .filter(|(p, _)| *p <= o)
            .map(|(_, q)| *q)
            .max()
            .unwrap_or(0)
    };
    groups.sort_by_key(|(o, n)| {
        let o1 = o.first().copied();
        (
            n.first().copied().unwrap_or_else(|| after(o1.unwrap_or(0))),
            n.is_empty(),
            o1,
        )
    });

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

/// The headings that occur once in each version, as indices into each, in
/// the order of the first.
fn same_headings(anchors: (&[Anchor], &[Anchor])) -> Vec<(usize, usize)> {
    let unique = |a: &[Anchor]| {
        let mut m: HashMap<u64, Option<usize>> = HashMap::new();
        for (i, &(id, _)) in a.iter().enumerate() {
            m.entry(id).and_modify(|p| *p = None).or_insert(Some(i));
        }
        m
    };
    let (ua, ub) = (unique(anchors.0), unique(anchors.1));
    anchors
        .0
        .iter()
        .filter_map(|(id, _)| Some(((*ua.get(id)?)?, (*ub.get(id)?)?)))
        .collect()
}

/// Pages of two versions, each marked against a common base, that show the
/// same part of the document; `at` holds the position of each change and
/// then each heading, in pages from the top. Positions map from one version
/// to the other linearly between the headings both have, and a page with a
/// change is paired with the page its position maps to.
pub fn pair_sections(a: &Side, b: &Side, at: [&[f64]; 2]) -> Vec<Pair> {
    let mut knots = vec![(0.0, 0.0)];
    for (i, j) in same_headings((a.headings, b.headings)) {
        let (x, y) = (at[0][a.changes.len() + i], at[1][b.changes.len() + j]);
        let &(px, py) = knots.last().unwrap();
        // Headings the versions put in a different order are left out.
        if x >= px && y >= py {
            knots.push((x, y));
        }
    }
    knots.push((a.pages as f64, b.pages as f64));
    let map = |x: f64, from_a: bool| {
        let k = |&(p, q): &(f64, f64)| if from_a { (p, q) } else { (q, p) };
        let w = knots
            .windows(2)
            .find(|w| x <= k(&w[1]).0)
            .unwrap_or(&knots[knots.len() - 2..]);
        let ((x0, y0), (x1, y1)) = (k(&w[0]), k(&w[1]));
        if x1 > x0 {
            y0 + (x - x0) * (y1 - y0) / (x1 - x0)
        } else {
            y0
        }
    };
    let mut groups = vec![];
    for (from_a, side, other, at) in [(true, a, b, at[0]), (false, b, a, at[1])] {
        for (c, &x) in side.changes.iter().zip(at) {
            let q = (map(x, from_a).floor() as usize + 1).clamp(1, other.pages);
            let (pa, pb) = if from_a { (c.page, q) } else { (q, c.page) };
            join(&mut groups, Some(pa), Some(pb));
        }
    }
    groups.sort_by_key(|(o, n)| (o.first().copied(), n.first().copied()));
    groups
        .into_iter()
        .map(|(o, n)| Pair {
            old: o.into_iter().collect(),
            new: n.into_iter().collect(),
            ..Pair::default()
        })
        .collect()
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
/// version only, the changes that are not marked and the bibliography
/// changes, and the commits if there are any.
pub fn composite(old: &Side, new: &Side, changes: &[Change], bib: &Bib, history: &History) -> String {
    let (pairs, unpaired) = pair_pages(old.changes, new.changes, (old.headings, new.headings));
    let extra = history
        .to_typst()
        .map(|h| format!("  history: {h},\n"))
        .unwrap_or_default();
    source(old, new, &pairs, &unpaired, changes, bib, &extra)
}

/// The composite for two versions `a` and `b`, each marked against their
/// merge base `base`, with positions `at` as [`pair_sections`] takes them
/// and the changes, bibliography changes and history of each.
pub fn compare_composite(
    a: &Side,
    b: &Side,
    at: [&[f64]; 2],
    base: &str,
    runs: [(&[Change], &Bib, &History); 2],
) -> String {
    let pairs = pair_sections(a, b, at);
    let changes = [runs[0].0, runs[1].0].concat();
    let both = |f: fn(&Bib) -> &Vec<String>| [&f(runs[0].1)[..], f(runs[1].1)].concat();
    let bib = Bib {
        added: both(|b| &b.added),
        removed: both(|b| &b.removed),
        changed: both(|b| &b.changed),
        files: both(|b| &b.files),
    };
    let history = |h: &History| h.to_typst().unwrap_or_else(|| "none".into());
    let extra = format!(
        "  base: {},\n  histories: ({}, {}),\n",
        typst_str(base),
        history(runs[0].2),
        history(runs[1].2)
    );
    source(a, b, &pairs, &[], &changes, &bib, &extra)
}

fn source(
    old: &Side,
    new: &Side,
    pairs: &[Pair],
    unpaired: &[(bool, Placed)],
    changes: &[Change],
    bib: &Bib,
    extra: &str,
) -> String {
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
    for p in pairs {
        out.push_str(&format!(
            "    (old: {}, new: {}, steps-old: {}, steps-new: {}),\n",
            list(&p.old),
            list(&p.new),
            list(&p.steps_old),
            list(&p.steps_new)
        ));
    }
    out.push_str("  ),\n  unpaired: (\n");
    for (in_old, c) in unpaired {
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
    out.push_str(&format!("  ),\n  bib: {},\n{extra}", bib.to_typst()));
    out.push_str("))\n");
    out
}
