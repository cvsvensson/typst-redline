//! Moved text: top-level blocks that the alignment of each file leaves
//! deleted in one place and inserted in another, possibly in another file.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use similar::{Algorithm, DiffOp, capture_diff_slices};

use crate::align::Step;
use crate::syntax::{Block, Doc, Kind, words};

/// The share of words a moved block keeps, at least, if it is also edited:
/// counted plainly, and with each word weighted by how rare it is in the
/// document, so that passages that share mostly common words are not taken
/// for one passage.
pub const MOVE_SIMILARITY: f64 = 0.6;

/// The words a block shares with its moved copy, at least, unless it moves
/// together with a longer block next to it, or it is a heading whose
/// section moves with it.
pub const MOVE_WORDS: usize = 4;

/// Consecutive blocks moved together: old blocks of the old file at index
/// `old.0` that became the new blocks of the new file at index `new.0`.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub old: (usize, Range<usize>),
    pub new: (usize, Range<usize>),
    /// The same number for the move from the new version to the old one.
    pub key: u64,
}

/// A block left over as deleted or inserted.
struct Loose<'a> {
    file: usize,
    at: usize,
    block: &'a Block,
    frame: String,
    words: Vec<String>,
    /// Its words sorted, to count shared words regardless of order.
    sorted: Vec<String>,
    weight: f64,
}

fn content_words(b: &Block) -> Vec<String> {
    words(&b.toks())
        .into_iter()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .collect()
}

/// The moves among the deleted and inserted blocks of `files`, each an old
/// and a new document and the steps that write it, in old reading order.
/// Blocks match if they have the same kind, marker and label, and the same
/// text up to whitespace, or else [`MOVE_SIMILARITY`] of their words; the
/// best matches are taken first, and each block is used once. Moves next
/// to each other at their old place that fill one place in the new version
/// are one move.
pub fn find(files: &[(&Doc, &Doc, &[Step])]) -> Vec<Run> {
    // The number of blocks of either version that use each word.
    let mut uses: HashMap<String, usize> = HashMap::new();
    let mut blocks = 0;
    for (old, new, _) in files {
        for b in old.blocks.iter().chain(&new.blocks) {
            blocks += 1;
            for w in content_words(b).into_iter().collect::<HashSet<_>>() {
                *uses.entry(w).or_default() += 1;
            }
        }
    }
    let weight = |w: &str| ((blocks + 1) as f64 / uses.get(w).copied().unwrap_or(1) as f64).ln();
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
                    let words = content_words(block);
                    let weight = words.iter().map(|w| weight(w)).sum();
                    let mut sorted = words.clone();
                    sorted.sort();
                    out.push(Loose {
                        file,
                        at,
                        block,
                        frame: block.frame(),
                        words,
                        sorted,
                        weight,
                    });
                }
            }
        }
        out
    };
    let (olds, news) = (loose(true), loose(false));
    let index = |ls: &[Loose]| -> HashMap<(usize, usize), usize> {
        ls.iter().enumerate().map(|(i, l)| ((l.file, l.at), i)).collect()
    };
    let (oindex, nindex) = (index(&olds), index(&news));
    // The words `o` and `n` share regardless of order, if that is enough
    // for them to match; all for the same text.
    let near = |o: &Loose, n: &Loose| -> Option<usize> {
        if o.block.kind != n.block.kind || o.frame != n.frame {
            return None;
        }
        if o.block.key == n.block.key {
            return Some(usize::MAX);
        }
        let (a, b) = (o.words.len() as f64, n.words.len() as f64);
        let t = MOVE_SIMILARITY;
        if a + b == 0.0 || b < t * a / (2.0 - t) || a < t * b / (2.0 - t) {
            return None;
        }
        let (mut at_most, mut i, mut j) = (0, 0, 0);
        while i < o.sorted.len() && j < n.sorted.len() {
            match o.sorted[i].cmp(&n.sorted[j]) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => (at_most, i, j) = (at_most + 1, i + 1, j + 1),
            }
        }
        (2.0 * at_most as f64 / (a + b) >= t).then_some(at_most)
    };
    // (similarity, words shared in order) if `o` and `n` match.
    let score = |o: &Loose, n: &Loose| -> Option<(f64, usize)> {
        if near(o, n)? == usize::MAX {
            return Some((2.0, o.words.len()));
        }
        let (a, b, t) = (o.words.len() as f64, n.words.len() as f64, MOVE_SIMILARITY);
        let (mut same, mut kept) = (0, 0.0);
        for op in capture_diff_slices(Algorithm::Myers, &o.words, &n.words) {
            if let DiffOp::Equal { old_index, len, .. } = op {
                same += len;
                kept += o.words[old_index..old_index + len]
                    .iter()
                    .map(|w| weight(w))
                    .sum::<f64>();
            }
        }
        let score = 2.0 * same as f64 / (a + b);
        let weighted = 2.0 * kept / (o.weight + n.weight).max(f64::MIN_POSITIVE);
        (score >= t && weighted >= t).then_some((score, same))
    };

    let mut m = Matching {
        opartner: vec![None; olds.len()],
        npartner: vec![None; news.len()],
    };
    let long = |l: &Loose| l.words.len() >= MOVE_WORDS;
    // Blocks moved unchanged, then the best of the edited ones.
    let mut same_text: HashMap<&str, Vec<usize>> = HashMap::new();
    for (j, n) in news.iter().enumerate().filter(|(_, n)| long(n)) {
        same_text.entry(&n.block.key).or_default().push(j);
    }
    for (i, o) in olds.iter().enumerate().filter(|(_, o)| long(o)) {
        let js = same_text.get(o.block.key.as_str()).into_iter().flatten();
        if let Some(&j) = js
            .into_iter()
            .find(|&&j| m.npartner[j].is_none() && o.frame == news[j].frame)
        {
            m.take(i, j);
        }
    }
    let mut cands: Vec<(f64, usize, usize)> = vec![];
    let mut by_token: HashMap<(usize, &str, usize), Vec<usize>> = HashMap::new();
    for (j, n) in news.iter().enumerate() {
        if long(n) && m.npartner[j].is_none() {
            for t in rarest(n, &uses) {
                by_token.entry(t).or_default().push(j);
            }
        }
    }
    // Words in order are compared only for the blocks that share the most
    // words with a block, which bounds the cost when many blocks share
    // common words.
    const COMPARED: usize = 4;
    let mut seen = vec![usize::MAX; news.len()];
    for (i, o) in olds.iter().enumerate() {
        if !long(o) || m.opartner[i].is_some() {
            continue;
        }
        let mut close: Vec<(usize, usize)> = vec![];
        for t in rarest(o, &uses) {
            for &j in by_token.get(&t).into_iter().flatten() {
                if std::mem::replace(&mut seen[j], i) != i
                    && let Some(shared) = near(o, &news[j])
                {
                    close.push((shared, j));
                }
            }
        }
        close.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for &(_, j) in close.iter().take(COMPARED) {
            if let Some((s, same)) = score(o, &news[j])
                && same >= MOVE_WORDS
            {
                cands.push((s, i, j));
            }
        }
    }
    cands.sort_by(|a, b| b.0.total_cmp(&a.0).then((a.1, a.2).cmp(&(b.1, b.2))));
    for &(_, i, j) in &cands {
        if m.opartner[i].is_none() && m.npartner[j].is_none() {
            m.take(i, j);
        }
    }

    // Short blocks next to a move in both versions.
    let extend = |m: &mut Matching, mut todo: Vec<usize>| {
        while let Some(i) = todo.pop() {
            let (o, n) = (&olds[i], &news[m.opartner[i].unwrap()]);
            for (oat, nat) in [(o.at + 1, n.at + 1), (o.at.wrapping_sub(1), n.at.wrapping_sub(1))] {
                let (Some(&a), Some(&b)) = (oindex.get(&(o.file, oat)), nindex.get(&(n.file, nat))) else {
                    continue;
                };
                if m.opartner[a].is_none() && m.npartner[b].is_none() && score(&olds[a], &news[b]).is_some() {
                    m.take(a, b);
                    todo.push(a);
                }
            }
        }
    };
    let taken = (0..olds.len()).filter(|&i| m.opartner[i].is_some()).collect();
    extend(&mut m, taken);
    // A heading moves with its section if some of the section moves into a
    // section under a heading of the same text.
    let mut headings = vec![];
    for (i, o) in olds.iter().enumerate() {
        if o.block.kind != Kind::Heading || m.opartner[i].is_some() {
            continue;
        }
        let old = &files[o.file].0.blocks;
        let section = old[o.at + 1..].iter().take_while(|b| b.kind != Kind::Heading).count();
        let heading = (o.at + 1..o.at + 1 + section).find_map(|b| {
            let n = &news[m.opartner[*oindex.get(&(o.file, b))?]?];
            let new = &files[n.file].1.blocks;
            let h = (0..n.at).rev().find(|&h| new[h].kind == Kind::Heading)?;
            nindex.get(&(n.file, h)).copied()
        });
        if let Some(j) = heading
            && m.npartner[j].is_none()
            && o.block.key == news[j].block.key
        {
            m.take(i, j);
            headings.push(i);
        }
    }
    extend(&mut m, headings);

    let mut pairs: Vec<(usize, usize, usize, usize)> = m
        .opartner
        .iter()
        .enumerate()
        .filter_map(|(i, j)| Some((olds[i].file, olds[i].at, news[(*j)?].file, news[(*j)?].at)))
        .collect();
    pairs.sort();
    // Moves next to each other at the old place are one if together they
    // fill one place in the new version, in any order.
    let mut runs: Vec<Run> = vec![];
    let mut s = 0;
    while s < pairs.len() {
        let (of, o, nf, n) = pairs[s];
        let (mut lo, mut hi, mut end, mut whole) = (n, n, s + 1, (n, n));
        while end < pairs.len() && pairs[end].0 == of && pairs[end].1 == o + end - s && pairs[end].2 == nf {
            (lo, hi) = (lo.min(pairs[end].3), hi.max(pairs[end].3));
            end += 1;
            if hi - lo == end - s - 1 {
                whole = (lo, hi);
            }
        }
        let len = whole.1 - whole.0 + 1;
        runs.push(Run {
            old: (of, o..o + len),
            new: (nf, whole.0..whole.1 + 1),
            key: 0,
        });
        s += len;
    }
    for r in &mut runs {
        let text = |(f, at): &(usize, Range<usize>), side: usize| -> String {
            let doc = if side == 0 { files[*f].0 } else { files[*f].1 };
            doc.blocks[at.clone()].iter().map(|b| b.key.as_str()).collect()
        };
        let mut texts = [text(&r.old, 0), text(&r.new, 1)];
        texts.sort();
        r.key = fnv(texts.concat().as_bytes()) % 1_000_000_007;
    }
    runs
}

/// The rarest words of a block, each with its number of occurrence so far,
/// by the number of blocks that use them: two blocks that share at least
/// [`MOVE_SIMILARITY`] of their words share one of these, so that only such
/// blocks need to be compared.
fn rarest<'a>(l: &'a Loose, uses: &HashMap<String, usize>) -> Vec<(usize, &'a str, usize)> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut toks: Vec<(usize, &str, usize)> = l
        .words
        .iter()
        .map(|w| {
            let k = seen.entry(w).or_default();
            *k += 1;
            (uses.get(w).copied().unwrap_or(0), w.as_str(), *k)
        })
        .collect();
    toks.sort();
    // Shared words make up the share t of both blocks' words only if they
    // are at least t / (2 - t) of each block's.
    let t = MOVE_SIMILARITY;
    let least = (t * toks.len() as f64 / (2.0 - t)).ceil() as usize;
    toks.truncate(toks.len() + 1 - least.max(1));
    toks
}

/// The partner of each loose old and new block.
struct Matching {
    opartner: Vec<Option<usize>>,
    npartner: Vec<Option<usize>>,
}

impl Matching {
    fn take(&mut self, i: usize, j: usize) {
        self.opartner[i] = Some(j);
        self.npartner[j] = Some(i);
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(0xcbf29ce484222325, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

/// Put the steps that write the new blocks `new` next to each other, with
/// the deletions that came between them first.
pub fn gather(steps: &mut [Step], new: &Range<usize>) {
    let at = |n: usize| steps.iter().position(|s| *s == Step::Ins(n));
    if let (Some(a), Some(b)) = (at(new.start), at(new.end - 1)) {
        steps[a..=b].sort_by_key(|s| !matches!(s, Step::Del(_)));
    }
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
        let old = format!("= One\n\n{LAKE}\n\n= Two\n\n{TRAIN}\n\n{BREAD}\n\n= Three\n\n{FOREST}\n");
        let edited = FOREST.replace("whole", "long");
        let new = format!("= One\n\n{LAKE}\n\n= Three\n\n{edited}\n\n= Two\n\n{TRAIN}\n\n{BREAD}\n");
        assert_eq!(found(&old, &new), [(5..7, 2..4)]);
    }

    #[test]
    fn short_block_moved_alone_is_not_a_move() {
        let old = format!("{LAKE}\n\nSee below.\n\n{TRAIN}\n\n{BREAD}\n");
        let new = format!("{LAKE}\n\n{TRAIN}\n\n{BREAD}\n\nSee below.\n");
        assert_eq!(found(&old, &new), []);
    }

    #[test]
    fn section_with_a_short_heading_moves_whole_also_reordered() {
        let old = format!("= Setup\n\n{LAKE}\n\n{TRAIN}\n\n= Results\n\n{BREAD}\n\n{FOREST}\n");
        let new = format!("= Results\n\n{BREAD}\n\n{FOREST}\n\n= Setup\n\n{TRAIN}\n\n{LAKE}\n");
        assert_eq!(found(&old, &new), [(0..3, 3..6)]);
    }

    #[test]
    fn passages_sharing_mostly_common_words_are_not_a_move() {
        let old = format!(
            "= One\n\n{LAKE}\n\nWe thank the referee for a careful reading of the manuscript and for the many helpful comments.\n\n= Two\n\n{TRAIN}\n"
        );
        let new = format!(
            "= One\n\n{LAKE}\n\n= Two\n\n{TRAIN}\n\nWe thank the editor of the journal for the handling of the manuscript and for the many reminders.\n"
        );
        assert_eq!(found(&old, &new), []);
    }
}
