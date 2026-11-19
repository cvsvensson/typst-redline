//! The side-by-side layout: which pages of the old and the new version show
//! the same changes, and the composite document that puts them next to each
//! other.

use std::collections::{BTreeSet, HashMap};

use crate::mark::{segments, typst_str};
use crate::{Change, ChangeKind, History, PREAMBLE_PATH};

/// A change as laid out in one version: its kind, an identity that is the
/// same in both versions, the mark in this version's source that draws it,
/// and its first and last page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub kind: PlacedKind,
    pub id: u64,
    pub mark: usize,
    pub page: usize,
    pub last: usize,
}

impl Placed {
    fn pages(&self) -> std::ops::RangeInclusive<usize> {
        self.page..=self.last.max(self.page)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlacedKind {
    Del,
    Ins,
    /// The start or the end of the place where the version has a moved
    /// passage; its identity is the move's.
    Move,
}

/// Pages of the two versions that hold the same changes, and the pages of
/// earlier steps of the same slide, which are not shown; `moved` if they
/// hold the two places of a moved passage.
#[derive(Debug, Default, PartialEq)]
pub struct Pair {
    pub old: Vec<usize>,
    pub new: Vec<usize>,
    pub steps_old: Vec<usize>,
    pub steps_new: Vec<usize>,
    pub moved: bool,
}

/// One version as laid out for the side-by-side layout: `pdf` is its
/// root-relative path, `pages` its number of pages before the summary,
/// `summary` the number of pages of the summary, `unbarred` the pages with
/// changes but no change bars, `looks` for each page a value that is the
/// same for pages drawn the same, or nothing.
pub struct Side<'a> {
    pub label: &'a str,
    pub pdf: &'a str,
    pub pages: usize,
    pub summary: usize,
    pub unbarred: &'a [usize],
    pub changes: &'a [Placed],
    pub headings: &'a [Anchor],
    pub looks: &'a [u64],
}

/// A heading as laid out in one version: an identity that is the same in
/// both versions if its text is, and its page.
pub type Anchor = (u64, usize);

/// The pages of each move in one version, those of its marks and of the
/// changes inside it, and for each change the move it is inside of, if any.
fn moves_of(side: &[Placed]) -> (HashMap<u64, Vec<usize>>, Vec<Option<u64>>) {
    let mut open: Vec<u64> = vec![];
    let mut pages: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut inside = vec![];
    for c in side {
        let key = if c.kind == PlacedKind::Move {
            match open.iter().position(|&k| k == c.id) {
                Some(i) => _ = open.remove(i),
                None => open.push(c.id),
            }
            Some(c.id)
        } else {
            open.last().copied()
        };
        if let Some(k) = key {
            pages.entry(k).or_default().extend(c.pages());
        }
        inside.push(key);
    }
    // A move's place spans the pages between its ends.
    for p in pages.values_mut() {
        let (first, a, b) = (p[0], *p.iter().min().unwrap(), *p.iter().max().unwrap());
        *p = std::iter::once(first).chain((a..=b).filter(|&q| q != first)).collect();
    }
    (pages, inside)
}

/// The pages that hold the same changes, in the order of the new version,
/// and the changes found in one version only (`true` for the old one).
/// The pages of a move's place in one version are paired with those of its
/// place in the other, with the changes inside it. Any other change is on a
/// page of the version that has its content, and on the other at the place
/// the content would be. That place's page is not a counterpart if its
/// headings, or the place of a passage moved from or to it, are on other
/// pages in the version with the content: the content fills pages of its
/// own. Pages without changes that are drawn differently (`looks`) between
/// the same pages drawn alike are paired as well: all together, or one by
/// one if a page among them has a change, which shows them next to its own. Whether such a page shows a
/// listed change or only reflows after a marked one cannot be told, so it is
/// shown. Consecutive pairs with
/// the same marks are steps of one slide; only the last is kept.
pub fn pair_pages(
    old: &[Placed],
    new: &[Placed],
    anchors: (&[Anchor], &[Anchor]),
    looks: (&[u64], &[u64]),
) -> (Vec<Pair>, Vec<(bool, Placed)>) {
    let pages_of = |a: &[Anchor]| {
        let mut m: HashMap<u64, Vec<usize>> = HashMap::new();
        for &(id, page) in a {
            m.entry(id).or_default().push(page);
        }
        m
    };
    let (ua, ub) = (pages_of(anchors.0), pages_of(anchors.1));
    let ((om, oin), (nm, nin)) = (moves_of(old), moves_of(new));
    // Old and new pages that show the same headings, or the two places of a
    // moved passage. A heading that is laid out as often in both versions,
    // as on each step of a slide, is paired in order.
    let mut same: Vec<(usize, usize)> = ua
        .iter()
        .filter_map(|(id, o)| Some((o, ub.get(id).filter(|n| n.len() == o.len())?)))
        .flat_map(|(o, n)| o.iter().copied().zip(n.iter().copied()))
        .collect();
    same.extend(om.iter().filter_map(|(key, o)| Some((o[0], nm.get(key)?[0]))));
    let counterparts = |in_old: bool, page: usize| {
        same.iter()
            .filter(move |(o, n)| if in_old { *o == page } else { *n == page })
            .map(move |(o, n)| if in_old { *n } else { *o })
    };
    let elsewhere = |in_old: bool, page: usize, other: usize| {
        let mut there = counterparts(in_old, page).peekable();
        there.peek().is_some() && !there.any(|p| p == other)
    };
    let mut groups: Vec<(BTreeSet<usize>, BTreeSet<usize>)> = vec![];
    let mut moved = BTreeSet::new();
    for (key, o) in &om {
        match nm.get(key) {
            Some(n) => {
                moved.insert(o[0]);
                o.iter().for_each(|&p| join(&mut groups, Some(p), Some(n[0])));
                n.iter().for_each(|&p| join(&mut groups, Some(o[0]), Some(p)));
            }
            None => o.iter().for_each(|&p| join(&mut groups, Some(p), None)),
        }
    }
    for (key, n) in &nm {
        if !om.contains_key(key) {
            n.iter().for_each(|&p| join(&mut groups, None, Some(p)));
        }
    }
    // Changes inside a move are in another order in the other version, and
    // are paired with the move.
    let outside = |side: &[Placed], inside: &[Option<u64>], kind: PlacedKind| -> Vec<Placed> {
        let paired = |k: &u64| om.contains_key(k) && nm.contains_key(k);
        side.iter()
            .zip(inside)
            .filter(|(c, k)| c.kind == kind && !k.as_ref().is_some_and(paired))
            .map(|(c, _)| *c)
            .collect()
    };
    let mut unpaired = vec![];
    for ins in [false, true] {
        let kind = if ins { PlacedKind::Ins } else { PlacedKind::Del };
        let (o, n) = (outside(old, &oin, kind), outside(new, &nin, kind));
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
                    join(&mut groups, c.pages().filter(|_| in_old), c.pages().filter(|_| !in_old));
                }
            }
            for (i, c) in os.iter().enumerate() {
                for (j, d) in ns.iter().enumerate() {
                    if os.len() != ns.len() || i == j {
                        // An insertion's content is in the new version.
                        let (absent, real) = if ins { (c.page, d.page) } else { (d.page, c.page) };
                        let alone = elsewhere(ins, absent, real) || elsewhere(!ins, real, absent);
                        join(
                            &mut groups,
                            c.pages().filter(|_| !(alone && ins)),
                            d.pages().filter(|_| !(alone && !ins)),
                        );
                    }
                }
            }
        }
    }
    for seg in segments(looks.0, looks.1).into_iter().filter(|s| !s.equal) {
        let pages = |r: std::ops::Range<usize>| r.map(|i| i + 1).collect::<BTreeSet<_>>();
        let (o, n) = (pages(seg.old), pages(seg.new));
        let changed = |side: &[Placed], p: usize| side.iter().any(|c| c.pages().contains(&p));
        if !o.iter().any(|&p| changed(old, p)) && !n.iter().any(|&p| changed(new, p)) {
            groups.push((o, n));
            continue;
        }
        let (o, n): (Vec<_>, Vec<_>) = (o.into_iter().collect(), n.into_iter().collect());
        for i in 0..o.len().max(n.len()) {
            let at = |v: &[usize]| v.get(i.min(v.len().saturating_sub(1))).copied();
            let (po, pn) = (at(&o), at(&n));
            if !po.is_some_and(|p| changed(old, p)) || !pn.is_some_and(|p| changed(new, p)) {
                join(&mut groups, po, pn);
            }
        }
    }
    // A page without a counterpart is shown next to the page with its
    // headings in the other version, if there is one.
    let lone: Vec<(bool, usize)> = groups
        .iter()
        .filter(|(o, n)| o.is_empty() || n.is_empty())
        .flat_map(|(o, n)| o.iter().map(|&p| (true, p)).chain(n.iter().map(|&p| (false, p))))
        .collect();
    for (in_old, page) in lone {
        for other in counterparts(in_old, page).collect::<Vec<_>>() {
            let (o, n) = if in_old { (page, other) } else { (other, page) };
            join(&mut groups, Some(o), Some(n));
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

    // The steps of a slide lay out the same marks again; two pages that
    // merely make the same edit do not.
    let changes = |side: &[Placed], pages: &BTreeSet<usize>| {
        side.iter()
            .filter(|c| pages.contains(&c.page))
            .map(|c| (c.kind, c.mark))
            .collect::<Vec<_>>()
    };
    let follows = |a: &BTreeSet<usize>, b: &BTreeSet<usize>| a.last().map(|p| p + 1) == b.first().copied();
    let mut pairs: Vec<Pair> = vec![];
    for (i, (o, n)) in groups.iter().enumerate() {
        let mut pair = Pair {
            old: o.iter().copied().collect(),
            new: n.iter().copied().collect(),
            moved: o.iter().any(|p| moved.contains(p)),
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
                pair.moved |= prev.moved;
            }
        }
        pairs.push(pair);
    }
    (pairs, unpaired)
}

/// Put old pages `o` and new pages `n` into one group, merging the groups
/// that hold any of them.
fn join(
    groups: &mut Vec<(BTreeSet<usize>, BTreeSet<usize>)>,
    o: impl IntoIterator<Item = usize>,
    n: impl IntoIterator<Item = usize>,
) {
    let mut group: (BTreeSet<usize>, BTreeSet<usize>) = (o.into_iter().collect(), n.into_iter().collect());
    groups.retain(|(go, gn)| {
        let joined = !group.0.is_disjoint(go) || !group.1.is_disjoint(gn);
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
/// version only, whether text moved, the changes that are not marked other
/// than those the summary of the new version lists, and the commits if
/// there are any.
pub fn composite(old: &Side, new: &Side, moved: bool, unmarked: &[Change], history: &History) -> String {
    let (mut pairs, mut unpaired) = pair_pages(
        old.changes,
        new.changes,
        (old.headings, new.headings),
        (old.looks, new.looks),
    );
    // A change at the end of a version without pages of its own, such as an
    // empty file, is placed on the summary after them.
    for p in &mut pairs {
        p.old.retain(|&q| q <= old.pages);
        p.new.retain(|&q| q <= new.pages);
    }
    pairs.retain(|p| !p.old.is_empty() || !p.new.is_empty());
    unpaired.retain(|(in_old, c)| c.page <= if *in_old { old.pages } else { new.pages });
    let list = |v: &[usize]| format!("({})", v.iter().map(|p| format!("{p},")).collect::<String>());
    let side = |s: &Side| {
        format!(
            "(label: {}, pdf: {}, pages: {}, summary: {}, unbarred: {})",
            typst_str(s.label),
            typst_str(s.pdf),
            s.pages,
            s.summary,
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
            "    (old: {}, new: {}, steps-old: {}, steps-new: {}, moved: {}),\n",
            list(&p.old),
            list(&p.new),
            list(&p.steps_old),
            list(&p.steps_new),
            p.moved
        ));
    }
    out.push_str("  ),\n  unpaired: (\n");
    for (in_old, c) in &unpaired {
        let side = if *in_old { "old" } else { "new" };
        let ins = c.kind == PlacedKind::Ins;
        out.push_str(&format!("    (side: \"{side}\", ins: {ins}, page: {}),\n", c.page));
    }
    out.push_str("  ),\n  unmarked: (\n");
    for c in unmarked.iter().filter(|c| c.kind == ChangeKind::Unmarked) {
        out.push_str(&format!("    (file: {}, {}),\n", typst_str(&c.file), c.code_fields()));
    }
    out.push_str("  ),\n");
    if moved {
        out.push_str("  moved: true,\n");
    }
    if let Some(h) = history.to_typst(old.label, new.label) {
        out.push_str(&format!("  history: {h},\n"));
    }
    out.push_str("))\n");
    out
}
