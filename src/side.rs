//! The side-by-side layout: which pages of the old and the new version show
//! the same changes, and the composite document that puts them next to each
//! other.

use std::collections::{BTreeSet, HashMap};

use crate::mark::{segments, typst_str};
use crate::{Bib, Change, ChangeKind, History, PREAMBLE_PATH};

/// A change as laid out in one version: its kind, an identity that is the
/// same in both versions, and its page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub kind: PlacedKind,
    pub id: u64,
    pub page: usize,
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
/// root-relative path, `pages` its number of pages, `unbarred` the pages
/// with changes but no change bars.
pub struct Side<'a> {
    pub label: &'a str,
    pub pdf: &'a str,
    pub pages: usize,
    pub unbarred: &'a [usize],
    pub changes: &'a [Placed],
    pub headings: &'a [Heading],
}

/// A heading as laid out in one version: an identity that is the same in
/// both versions if its text is, and its page.
pub type Anchor = (u64, usize);

/// A heading as laid out in one version, with its text.
pub struct Heading {
    pub id: u64,
    pub page: usize,
    pub title: String,
}

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
            pages.entry(k).or_default().push(c.page);
        }
        inside.push(key);
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
/// own. Consecutive pairs with the same changes are steps
/// of one slide; only the last is kept.
pub fn pair_pages(old: &[Placed], new: &[Placed], anchors: (&[Anchor], &[Anchor])) -> (Vec<Pair>, Vec<(bool, Placed)>) {
    let unique = |a: &[Anchor]| {
        let mut m: HashMap<u64, Option<usize>> = HashMap::new();
        for &(id, page) in a {
            m.entry(id).and_modify(|p| *p = None).or_insert(Some(page));
        }
        m
    };
    let (ua, ub) = (unique(anchors.0), unique(anchors.1));
    let ((om, oin), (nm, nin)) = (moves_of(old), moves_of(new));
    // Old and new pages that show the same headings, or the two places of a
    // moved passage.
    let mut same: Vec<(usize, usize)> = ua.iter().filter_map(|(id, o)| Some(((*o)?, (*ub.get(id)?)?))).collect();
    same.extend(om.iter().filter_map(|(key, o)| Some((o[0], nm.get(key)?[0]))));
    let elsewhere = |in_old: bool, page: usize, other: usize| {
        let there = same
            .iter()
            .filter(|(o, n)| if in_old { *o == page } else { *n == page });
        let mut there = there.map(|(o, n)| if in_old { *n } else { *o }).peekable();
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
            .map(|c| (c.kind, c.id))
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
/// changes, and the commits if there are any. Each shown page is titled by
/// its first heading, or else the last heading before it.
pub fn composite(old: &Side, new: &Side, changes: &[Change], bib: &Bib, history: &History) -> String {
    let anchors = |s: &Side| s.headings.iter().map(|h| (h.id, h.page)).collect::<Vec<_>>();
    let (pairs, unpaired) = pair_pages(old.changes, new.changes, (&anchors(old), &anchors(new)));
    let title = |s: &Side, page: usize| {
        let hs = s.headings;
        let h = hs
            .iter()
            .find(|h| h.page == page)
            .or_else(|| hs.iter().rev().find(|h| h.page < page));
        typst_str(h.map_or("", |h| h.title.as_str()))
    };
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
        let titles: String = (0..p.old.len().max(p.new.len()))
            .map(|r| match p.new.get(r).or(p.new.last()) {
                Some(&n) => title(new, n) + ",",
                None => title(old, p.old[r]) + ",",
            })
            .collect();
        out.push_str(&format!(
            "    (old: {}, new: {}, steps-old: {}, steps-new: {}, titles: ({titles}), moved: {}),\n",
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
    for c in changes.iter().filter(|c| c.kind == ChangeKind::Unmarked) {
        out.push_str(&format!("    (file: {}, {}),\n", typst_str(&c.file), c.code_fields()));
    }
    out.push_str(&format!("  ),\n  bib: {},\n", bib.to_typst()));
    if changes.iter().any(|c| c.kind == ChangeKind::Moved) {
        out.push_str("  moved: true,\n");
    }
    if let Some(h) = history.to_typst(old.label, new.label) {
        out.push_str(&format!("  history: {h},\n"));
    }
    out.push_str("))\n");
    out
}
