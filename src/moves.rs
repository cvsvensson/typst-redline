//! Moved text: top-level blocks that the alignment of each file leaves
//! deleted in one place and inserted in another, possibly in another file.

use std::collections::HashMap;
use std::ops::Range;

use crate::align::Step;
use crate::syntax::{Block, Doc, Kind, shared, words};

/// The share of words a moved block keeps, at least, if it is also edited.
pub const MOVE_SIMILARITY: f64 = 0.6;

/// The words a block shares with its moved copy, at least, unless it is
/// next to a longer move in both versions, as a heading moved with its
/// section is.
pub const MOVE_WORDS: usize = 4;

/// Consecutive blocks moved together: old blocks of the old file at index
/// `old.0` that became the new blocks of the new file at index `new.0`,
/// one for one.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub old: (usize, Range<usize>),
    pub new: (usize, Range<usize>),
}

/// A block left over as deleted or inserted.
struct Loose<'a> {
    file: usize,
    at: usize,
    block: &'a Block,
    words: Vec<String>,
}

/// The moves among the deleted and inserted blocks of `files`, each an old
/// and a new document and the steps that write it, in old reading order.
/// Blocks match if they have the same kind, marker and label, and the same
/// text up to whitespace, or else [`MOVE_SIMILARITY`] of their words; the
/// best matches are taken first, and each block is used once.
pub fn find(files: &[(&Doc, &Doc, &[Step])]) -> Vec<Run> {
    let loose = |deleted: bool| -> Vec<Loose> {
        let mut out = vec![];
        for (file, (old, new, steps)) in files.iter().enumerate() {
            for s in steps.iter() {
                let (doc, at) = match *s {
                    Step::Del(o) if deleted => (old, o),
                    Step::Ins(n) if !deleted => (new, n),
                    _ => continue,
                };
                let block = &doc.blocks[at];
                if block.kind != Kind::Setup {
                    let words = words(&block.toks())
                        .into_iter()
                        .filter(|w| w.chars().any(char::is_alphanumeric))
                        .collect();
                    out.push(Loose { file, at, block, words });
                }
            }
        }
        out
    };
    let (olds, news) = (loose(true), loose(false));
    fn bag<'a>(l: &'a Loose) -> HashMap<&'a str, usize> {
        let mut m = HashMap::new();
        l.words.iter().for_each(|w| *m.entry(w.as_str()).or_default() += 1);
        m
    }
    let (obags, nbags): (Vec<_>, Vec<_>) = (olds.iter().map(bag).collect(), news.iter().map(bag).collect());
    // (score, shared words, old, new); identical blocks score above all.
    let mut cands: Vec<(f64, usize, usize, usize)> = vec![];
    for (i, o) in olds.iter().enumerate() {
        for (j, n) in news.iter().enumerate() {
            if o.block.kind != n.block.kind || o.block.frame() != n.block.frame() {
                continue;
            }
            if o.block.key == n.block.key {
                cands.push((2.0, o.words.len(), i, j));
                continue;
            }
            let total = (o.words.len() + n.words.len()) as f64;
            let at_most: usize = obags[i]
                .iter()
                .map(|(w, c)| *c.min(nbags[j].get(w).unwrap_or(&0)))
                .sum();
            if total == 0.0 || 2.0 * at_most as f64 / total < MOVE_SIMILARITY {
                continue;
            }
            let same = shared(&o.words, &n.words);
            let score = 2.0 * same as f64 / total;
            if score >= MOVE_SIMILARITY {
                cands.push((score, same, i, j));
            }
        }
    }
    cands.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (mut oused, mut nused) = (vec![false; olds.len()], vec![false; news.len()]);
    let mut taken: Vec<(usize, usize)> = vec![];
    let mut take = |i: usize, j: usize, taken: &mut Vec<(usize, usize)>| {
        if oused[i] || nused[j] {
            return false;
        }
        (oused[i], nused[j]) = (true, true);
        taken.push((i, j));
        true
    };
    for &(_, same, i, j) in &cands {
        if same >= MOVE_WORDS {
            take(i, j, &mut taken);
        }
    }
    // Short blocks next to a move in both versions, until none is added.
    let beside = |i: usize, j: usize, taken: &[(usize, usize)]| {
        let (o, n) = (&olds[i], &news[j]);
        taken.iter().any(|&(a, b)| {
            let (p, q) = (&olds[a], &news[b]);
            p.file == o.file
                && q.file == n.file
                && (p.at + 1 == o.at && q.at + 1 == n.at || o.at + 1 == p.at && n.at + 1 == q.at)
        })
    };
    loop {
        let before = taken.len();
        for &(_, same, i, j) in &cands {
            if same < MOVE_WORDS && beside(i, j, &taken) {
                take(i, j, &mut taken);
            }
        }
        if taken.len() == before {
            break;
        }
    }
    let mut pairs: Vec<(usize, usize, usize, usize)> = taken
        .iter()
        .map(|&(i, j)| (olds[i].file, olds[i].at, news[j].file, news[j].at))
        .collect();
    pairs.sort();
    let mut runs: Vec<Run> = vec![];
    for (of, o, nf, n) in pairs {
        match runs.last_mut() {
            Some(r) if r.old.0 == of && r.old.1.end == o && r.new.0 == nf && r.new.1.end == n => {
                r.old.1.end += 1;
                r.new.1.end += 1;
            }
            _ => runs.push(Run {
                old: (of, o..o + 1),
                new: (nf, n..n + 1),
            }),
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use typst_syntax::parse;

    use super::*;
    use crate::align::align;

    fn doc(src: &str) -> Doc {
        Doc::new(&parse(src).children().cloned().collect::<Vec<_>>())
    }

    /// The moves from `old` to `new` as (old blocks, new blocks).
    fn found(old: &str, new: &str) -> Vec<(Range<usize>, Range<usize>)> {
        let (o, n) = (doc(old), doc(new));
        let steps = align(&o, &n);
        find(&[(&o, &n, &steps)])
            .into_iter()
            .map(|r| (r.old.1, r.new.1))
            .collect()
    }

    const LAKE: &str = "Herons wade along the shallow lake at dawn, hunting small fish.";
    const TRAIN: &str = "The night train crosses the mountains before reaching the coast.";
    const BREAD: &str = "Fresh bread needs flour, water, salt and a long slow rise.";
    const FOREST: &str = "Old beech trees shade the forest floor through the whole summer.";

    #[test]
    fn moved_section_is_one_run_and_may_be_edited() {
        let old = format!(
            "= One

{LAKE}

= Two

{TRAIN}

{BREAD}

= Three

{FOREST}
"
        );
        let edited = FOREST.replace("whole", "long");
        let new = format!(
            "= One

{LAKE}

= Three

{edited}

= Two

{TRAIN}

{BREAD}
"
        );
        assert_eq!(found(&old, &new), [(5..7, 2..4)]);
    }

    #[test]
    fn short_block_moved_alone_is_not_a_move() {
        let old = format!(
            "{LAKE}

See below.

{TRAIN}

{BREAD}
"
        );
        let new = format!(
            "{LAKE}

{TRAIN}

{BREAD}

See below.
"
        );
        assert_eq!(found(&old, &new), []);
    }
}
