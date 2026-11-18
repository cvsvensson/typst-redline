//! Which step of a series of versions wrote or removed each piece of text.

use crate::mark::segments;
use crate::syntax::split_words;

/// A step and a second step for each byte of a file.
pub type Steps = Vec<(u8, u8)>;

/// For each byte of the last of `versions`, the step that wrote it, and for
/// each byte of the first, the step that removed it; step `i` leads from
/// version `i - 1` to version `i`. Each comes with a second step for text
/// that the steps keep but that the direct comparison of the first and the
/// last version may see as changed: the last step that changed text next to
/// it, and the first. 0 stands for none.
pub fn blame(versions: &[&str]) -> (Steps, Steps) {
    let pieces: Vec<Vec<&str>> = versions.iter().map(|v| split_words(v)).collect();
    let n0 = pieces[0].len();
    let (mut wrote, mut removed) = (vec![(0u8, 0u8); n0], vec![(0u8, 0u8); n0]);
    // The piece of the first version that each piece of the current one is.
    let mut origin: Vec<Option<usize>> = (0..n0).map(Some).collect();
    for (i, w) in pieces.windows(2).enumerate() {
        let step = i as u8 + 1;
        let (mut next, mut next_origin) = (vec![(step, step); w[1].len()], vec![None; w[1].len()]);
        let mut kept = vec![false; w[0].len()];
        let segs = segments(&w[0], &w[1]);
        for s in segs.iter().filter(|s| s.equal) {
            for (a, b) in s.old.clone().zip(s.new.clone()) {
                next[b] = wrote[a];
                next_origin[b] = origin[a];
                kept[a] = true;
            }
        }
        for s in segs.iter().filter(|s| !s.equal) {
            for b in beside(&w[1], s.new.start, s.new.end) {
                next[b].1 = step;
            }
            for a in beside(&w[0], s.old.start, s.old.end) {
                if let Some(k) = origin[a]
                    && removed[k].1 == 0
                {
                    removed[k].1 = step;
                }
            }
        }
        for (a, k) in origin.iter().enumerate() {
            if let Some(k) = *k
                && !kept[a]
                && removed[k].0 == 0
            {
                removed[k].0 = step;
            }
        }
        wrote = next;
        origin = next_origin;
    }
    let bytes = |pieces: &[&str], steps: &[(u8, u8)]| -> Steps {
        pieces
            .iter()
            .zip(steps)
            .flat_map(|(p, &s)| std::iter::repeat_n(s, p.len()))
            .collect()
    };
    (bytes(pieces.last().unwrap(), &wrote), bytes(&pieces[0], &removed))
}

/// The pieces right before and after `start..end`, each up to and including
/// the nearest one that is not whitespace.
fn beside(pieces: &[&str], start: usize, end: usize) -> Vec<usize> {
    let blank = |i: usize| pieces[i].trim().is_empty();
    let mut out = vec![];
    for i in (0..start).rev() {
        out.push(i);
        if !blank(i) {
            break;
        }
    }
    for i in end..pieces.len() {
        out.push(i);
        if !blank(i) {
            break;
        }
    }
    out
}
