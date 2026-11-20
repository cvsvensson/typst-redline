//! Writing the new version with the changes against the old one marked, as
//! calls to the preamble's functions, and the summary.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::ops::Range;

use similar::{Algorithm, DiffOp, capture_diff_slices};
use typst_syntax::ast::AstNode;
use typst_syntax::{RootedPath, Source, Span, SyntaxKind as K, SyntaxNode, VirtualPath, VirtualRoot, ast, parse};

use crate::align::{Step, align};
use crate::blame::{Steps, blame};
use crate::moves::{self, Run};
use crate::syntax::*;
use crate::{
    Bib, Change, ChangeKind, History, Move, Moves, OLD_DIR, Options, PREAMBLE_PATH, What, pair_files, resolve,
};

/// Whether item `i` of `doc` is spaced as part of a run with the paragraph
/// or item of its kind before it, across setup code, which a blank line
/// between them ends: that block (`None` for the text of the item the
/// document is nested in, if `head`) and whether a blank line separates
/// them. `None` if `i` is not an item or the block before it is laid out as
/// a block of its own whatever the gap.
fn attachment(doc: &Doc, i: usize, head: bool) -> Option<(Option<usize>, bool)> {
    let b = &doc.blocks[i];
    if b.kind != Kind::Item {
        return None;
    }
    let mut apart = parbreak(&b.gap);
    for j in (0..i).rev() {
        let p = &doc.blocks[j];
        let joins = match p.kind {
            Kind::Setup => {
                apart |= parbreak(&p.gap);
                continue;
            }
            Kind::Para => p
                .toks()
                .iter()
                .rev()
                .find(|t| t.kind != TK::Space && t.kind != TK::Comment)
                .is_some_and(|t| !t.block),
            Kind::Item => p.nodes[0].kind() == b.nodes[0].kind(),
            Kind::Heading => false,
        };
        return joins.then_some((Some(j), apart));
    }
    head.then_some((None, apart))
}

/// Whether whitespace `gap` holds a paragraph break.
fn parbreak(gap: &str) -> bool {
    parse(gap).children().any(|c| c.kind() == K::Parbreak)
}

/// Text that closes what comes before it.
const CLOSING: [char; 13] = [
    ',', '.', ';', ':', '!', '?', ')', ']', '}', '\'', '"', '\u{2019}', '\u{201d}',
];

/// What is known about both versions before writing.
pub struct Ctx {
    /// Prefix of the names the output uses for the preamble's functions and
    /// its own labels.
    prefix: String,
    opts: Options,
    new_targets: HashSet<String>,
    /// Strings that the new version's show rules match in text.
    shown_strings: HashSet<String>,
    /// Names defined somewhere in the old version but nowhere in the new.
    removed_defs: HashSet<String>,
    /// The project files, the entry first.
    old_files: Vec<(String, String)>,
    new_files: Vec<(String, String)>,
    /// New file path to the path of the old file it is compared with.
    pub(crate) pairs: HashMap<String, String>,
    /// The project files of the versions between the old and the new one,
    /// if each change is marked with the step that made it.
    steps: Option<Vec<Vec<(String, String)>>>,
    /// The literal paths of the old version that read a changed file, as
    /// (rewritten literal, original literal, root-relative path).
    pinned: Vec<(String, String, String)>,
    /// Each new file that is to be written, by path, with its old file, in
    /// reading order.
    aligned: Vec<(String, Aligned)>,
    /// Blocks moved, by index into `aligned`.
    moves: Vec<Run>,
    /// The move each moved old and new block belongs to, by (index into
    /// `aligned`, block).
    moved_from: HashMap<(usize, usize), usize>,
    moved_to: HashMap<(usize, usize), usize>,
    /// Setup code deleted and inserted unchanged in a file, by (index into
    /// `aligned`, block key): moved.
    moved_setup: HashSet<(usize, String)>,
}

/// A new file and the old file it is compared with, as blocks, and the
/// steps that write it.
struct Aligned {
    old_path: String,
    /// Distinct file ids, so that a span finds its node only in its file.
    sources: (Source, Source),
    docs: (Doc, Doc),
    steps: Vec<Step>,
    /// For each byte of the new file the step that wrote it, and for each
    /// byte of the old file the step that removed it (see [`blame`]).
    blame: Option<(Steps, Steps)>,
    /// The old file's [`top_bindings`].
    old_top: Vec<(usize, Option<String>)>,
    /// The names the new file binds anywhere.
    new_names: HashSet<String>,
}

impl Ctx {
    pub fn new(
        old_files: Vec<(String, String)>,
        new_files: Vec<(String, String)>,
        steps: Option<Vec<Vec<(String, String)>>>,
        swapped: bool,
        pinned: Vec<(String, String, String)>,
        opts: Options,
        moves: Moves,
    ) -> Self {
        let union = |files: &[(String, String)], f: fn(&SyntaxNode) -> HashSet<String>| {
            files.iter().flat_map(|(_, s)| f(&parse(s))).collect::<HashSet<_>>()
        };
        // Names the output adds must not collide with names in the project.
        let used = |p: &String| {
            old_files
                .iter()
                .chain(&new_files)
                .any(|(_, s)| s.contains(&format!("{p}-")))
        };
        let prefix = std::iter::once("redline".into())
            .chain((1..).map(|n| format!("redline{n}")))
            .find(|p| !used(p))
            .unwrap();
        let pairs = pair_files(&old_files, &new_files);
        let history = &steps;
        let mut aligned: Vec<(String, Aligned)> = new_files
            .iter()
            .filter_map(|(path, new_src)| {
                let old_path = pairs.get(path).cloned().unwrap_or_default();
                let old_src = source(&old_files, &old_path).unwrap_or_default();
                if *path != new_files[0].0 && old_src == new_src {
                    return None;
                }
                let parsed = |side: &str, src: &str| {
                    let name = format!("{side}/{path}");
                    let id = RootedPath::new(VirtualRoot::Project, VirtualPath::new(&name).unwrap()).intern();
                    Source::new(id, src.into())
                };
                let (os, ns) = (parsed("old", old_src), parsed("new", new_src));
                let children = |s: &Source| s.root().children().cloned().collect::<Vec<_>>();
                let docs = (Doc::new(&children(&os)), Doc::new(&children(&ns)));
                let steps = match moves {
                    // The comparison back keeps what the forward one kept:
                    // moved blocks are not matched in place.
                    Moves::Reverse(given) => {
                        let apart =
                            |doc: &Doc, path: &str, ranges: &dyn Fn(&Move) -> &(String, Range<usize>), mark: &str| {
                                let mut doc = doc.clone();
                                for (p, r) in given.iter().map(ranges) {
                                    if p == path {
                                        r.clone().for_each(|b| doc.blocks[b].key.push_str(mark));
                                    }
                                }
                                doc
                            };
                        align(
                            &apart(&docs.0, &old_path, &|m| &m.new, "\0from"),
                            &apart(&docs.1, path, &|m| &m.old, "\0to"),
                        )
                    }
                    _ => align(&docs.0, &docs.1),
                };
                let blame = history.as_ref().map(|versions| {
                    let (first, last) = if swapped {
                        (new_src.as_str(), old_src)
                    } else {
                        (old_src, new_src.as_str())
                    };
                    let mut chain = vec![first];
                    chain.extend(
                        versions
                            .iter()
                            .map(|f| source(f, path).or(source(f, &old_path)).unwrap_or_default()),
                    );
                    chain.push(last);
                    let (wrote, removed) = blame(&chain);
                    // Swapped, what this run inserts the steps removed.
                    if swapped { (removed, wrote) } else { (wrote, removed) }
                });
                let a = Aligned {
                    old_path,
                    old_top: top_bindings(os.root()),
                    new_names: all_bindings(ns.root(), &HashSet::new()).into_iter().flatten().collect(),
                    sources: (os, ns),
                    docs,
                    steps,
                    blame,
                };
                Some((path.clone(), a))
            })
            .collect();
        let moves: Vec<Run> = match moves {
            Moves::Off => vec![],
            Moves::Find => {
                // Text in a file that is not read in order may not be shown.
                let (old_shown, new_shown) = (read_in_order(&old_files), read_in_order(&new_files));
                let shown: Vec<Vec<Step>> = aligned
                    .iter()
                    .map(|(path, a)| {
                        let shown = |s: &&Step| match s {
                            Step::Del(_) => old_shown.contains(&a.old_path),
                            Step::Ins(_) => new_shown.contains(path),
                            _ => true,
                        };
                        a.steps.iter().filter(shown).copied().collect()
                    })
                    .collect();
                moves::find(
                    &aligned
                        .iter()
                        .zip(&shown)
                        .map(|((_, a), s)| (&a.docs.0, &a.docs.1, &s[..]))
                        .collect::<Vec<_>>(),
                )
            }
            Moves::Reverse(given) => given
                .iter()
                .filter_map(|m| {
                    let old = aligned.iter().position(|(_, a)| a.old_path == m.new.0)?;
                    let new = aligned.iter().position(|(p, _)| *p == m.old.0)?;
                    let all = |f: usize, blocks: &Range<usize>, step: fn(usize) -> Step| {
                        blocks.clone().all(|b| aligned[f].1.steps.contains(&step(b)))
                    };
                    (all(old, &m.new.1, Step::Del) && all(new, &m.old.1, Step::Ins)).then(|| Run {
                        old: (old, m.new.1.clone()),
                        new: (new, m.old.1.clone()),
                        key: m.key,
                    })
                })
                .collect(),
        };
        let mut moved_setup = HashSet::new();
        for (i, (_, a)) in aligned.iter().enumerate() {
            let setup = |deleted: bool| -> HashSet<&str> {
                a.steps
                    .iter()
                    .filter_map(|s| match *s {
                        Step::Del(o) if deleted => Some(&a.docs.0.blocks[o]),
                        Step::Ins(n) if !deleted => Some(&a.docs.1.blocks[n]),
                        _ => None,
                    })
                    .filter(|b| b.kind == Kind::Setup && !b.is_import())
                    .map(|b| b.key.as_str())
                    .collect()
            };
            moved_setup.extend((&setup(true) & &setup(false)).into_iter().map(|k| (i, k.to_string())));
        }
        let (mut moved_from, mut moved_to) = (HashMap::new(), HashMap::new());
        for (i, r) in moves.iter().enumerate() {
            moves::gather(&mut aligned[r.new.0].1.steps, &r.new.1);
            moved_from.extend(r.old.1.clone().map(|b| ((r.old.0, b), i)));
            moved_to.extend(r.new.1.clone().map(|b| ((r.new.0, b), i)));
        }
        Ctx {
            prefix,
            opts,
            new_targets: union(&new_files, targets),
            shown_strings: union(&new_files, shown_strings),
            removed_defs: union(&old_files, definitions)
                .difference(&union(&new_files, definitions))
                .cloned()
                .collect(),
            pairs,
            old_files,
            new_files,
            steps,
            pinned,
            aligned,
            moves,
            moved_from,
            moved_to,
            moved_setup,
        }
    }

    /// The moves, by file path.
    pub fn moves(&self) -> Vec<Move> {
        self.moves
            .iter()
            .map(|r| Move {
                old: (self.aligned[r.old.0].1.old_path.clone(), r.old.1.clone()),
                new: (self.aligned[r.new.0].0.clone(), r.new.1.clone()),
                key: r.key,
            })
            .collect()
    }

    /// The step from the last version before the new one to the new one.
    fn last_step(&self) -> Option<u8> {
        self.steps.as_ref().map(|s| s.len() as u8 + 1)
    }

    /// The file a token is from and, for each of its bytes, the step that
    /// wrote it (new file) or removed it (old file).
    fn blamed(&self, t: &Tok) -> Option<(&Source, &Steps)> {
        let id = t.at.0.id()?;
        self.aligned.iter().find_map(|(_, a)| {
            let (wrote, removed) = a.blame.as_ref()?;
            if a.sources.0.id() == id {
                Some((&a.sources.0, removed))
            } else if a.sources.1.id() == id {
                Some((&a.sources.1, wrote))
            } else {
                None
            }
        })
    }

    /// Old code as it was written, with the paths [`crate::pin_reads`]
    /// rewrote restored, and the files these read.
    fn unpin(&self, code: &str) -> (String, Vec<String>) {
        let mut code = code.to_string();
        let mut files = vec![];
        for (pin, lit, path) in &self.pinned {
            if code.contains(pin.as_str()) {
                code = code.replace(pin.as_str(), lit);
                if !files.contains(path) {
                    files.push(path.clone());
                }
            }
        }
        (code, files)
    }

    fn entry(&self) -> &str {
        &self.new_files[0].0
    }

    /// The line importing the preamble's functions under the prefix.
    fn import(&self) -> String {
        let p = &self.prefix;
        let names = ["ins", "del", "summary", "bars"].map(|n| match p.as_str() {
            "redline" => format!("redline-{n}"),
            _ => format!("redline-{n} as {p}-{n}"),
        });
        format!("#import \"/{PREAMBLE_PATH}\": {}\n", names.join(", "))
    }
}

fn source<'a>(files: &'a [(String, String)], path: &str) -> Option<&'a str> {
    files.iter().find(|(p, _)| p == path).map(|(_, s)| s.as_str())
}

/// The entry and, recursively, the files that the top-level `#include`
/// blocks of these files name: the files whose text is shown in order.
fn read_in_order(files: &[(String, String)]) -> HashSet<String> {
    let mut seen = HashSet::new();
    let mut queue = vec![files[0].0.clone()];
    while let Some(path) = queue.pop() {
        let Some(src) = source(files, &path) else { continue };
        if !seen.insert(path.clone()) {
            continue;
        }
        let doc = Doc::new(&parse(src).children().cloned().collect::<Vec<_>>());
        let included = doc.blocks.iter().filter_map(|b| resolve(&path, &b.include_target()?));
        queue.extend(included.collect::<Vec<_>>());
    }
    seen
}

/// The marked files and the changes: every file the entry includes or
/// imports that differs from its old file, the entry with the summary, and
/// the old version's files under [`OLD_DIR`] if deleted content uses them.
pub fn write(
    ctx: &Ctx,
    old_label: &str,
    new_label: &str,
    bib: &Bib,
    history: &History,
) -> (Vec<(String, String)>, Vec<Change>) {
    let mut w = Writer::new(ctx);
    w.file(ctx.entry());
    w.top = false;
    w.current = None;
    for (path, _) in &ctx.new_files {
        w.file(path);
    }
    let mut files = vec![(ctx.entry().to_string(), w.finish(old_label, new_label, bib, history))];
    files.append(&mut w.files);
    if w.old_scope {
        files.extend(
            ctx.old_files
                .iter()
                .map(|(p, s)| (format!("{OLD_DIR}/{p}"), old_copy(s))),
        );
    }
    (files, w.changes)
}

struct Section {
    file: String,
    level: usize,
    target: Option<String>,
    /// Typst expression.
    title: String,
    deleted: bool,
    /// Inserted heading.
    new: bool,
    /// The move that brought the heading here.
    moved: Option<usize>,
}

/// The file being written.
#[derive(Default)]
struct File {
    path: String,
    /// The old file it is compared with; empty if there is none.
    old_path: String,
    /// Indentation of the list item body being written.
    indent: String,
    /// Number of list items the blocks being written are nested in.
    depth: usize,
    /// Names that deleted code which is not evaluated must not use: defined
    /// in the old file or project but not in the new one.
    removed: HashSet<String>,
    /// Its index in [`Ctx::aligned`].
    index: usize,
}

struct Writer<'a> {
    ctx: &'a Ctx,
    file: File,
    out: String,
    entry_out: String,
    /// Marked files other than the entry.
    files: Vec<(String, String)>,
    done: HashSet<String>,
    /// Writing the top level of a file in reading order: headings are
    /// sections.
    top: bool,
    /// Where the bars and the summary start.
    summary_at: Option<usize>,
    sections: Vec<Section>,
    /// Label of the section being written.
    current: Option<String>,
    changes: Vec<Change>,
    /// Deleted content imports the old version.
    old_scope: bool,
    /// The last new block written is an item.
    after_item: bool,
    /// The moves whose new place is written, with the section it is in.
    arrived: HashMap<usize, Option<String>>,
    /// Diffing the arguments of the first call of code that shows as a
    /// block of content.
    content_code: bool,
    /// The old value of the argument that [`Self::marked_arg`] took in
    /// that code.
    hoisted: Option<String>,
    /// The edges of the change being written that touch CJK text.
    bare: Bare,
}

/// The edges of a change at which a CJK character meets the text on the
/// other side: there, Typst spaces or compresses the two, which the inline
/// marks leave to it with tags that take no part in the layout. A deletion
/// sits at the start of the change if an insertion follows, else at both
/// edges, once its text is gone.
#[derive(Clone, Copy, Default)]
struct Bare {
    start: bool,
    end: bool,
    replaced: bool,
}

impl Bare {
    fn of(old: &[Tok], new: &[Tok], before: Option<&Tok>, after: Option<&Tok>) -> Self {
        let starts = |t: Option<&Tok>| t.is_some_and(|t| t.text.starts_with(is_cjk));
        let ends = |t: Option<&Tok>| t.is_some_and(|t| t.text.ends_with(is_cjk));
        Bare {
            start: ends(before) || starts(old.first()) || starts(new.first()),
            end: starts(after) || ends(old.last()) || ends(new.last()),
            replaced: !new.is_empty(),
        }
    }

    /// The edges of a mark of `kind` with bare tags, as the preamble names
    /// them.
    fn edges(self, kind: ChangeKind) -> Option<&'static str> {
        let (start, end) = match kind {
            ChangeKind::Del if self.replaced => (self.start, self.start),
            ChangeKind::Del => (self.start || self.end, self.start || self.end),
            _ => (self.start, self.end),
        };
        match (start, end) {
            (true, true) => Some("both"),
            (true, false) => Some("start"),
            (false, true) => Some("end"),
            (false, false) => None,
        }
    }
}

/// A run of tokens that is the same in both versions or changed.
#[derive(Clone)]
pub(crate) struct Seg {
    pub old: Range<usize>,
    pub new: Range<usize>,
    pub equal: bool,
}

/// The runs that are the same in `old` and `new` and the changed runs
/// between them.
pub(crate) fn segments<T: Hash + Eq + Ord>(old: &[T], new: &[T]) -> Vec<Seg> {
    let mut segs = vec![];
    for op in capture_diff_slices(Algorithm::Myers, old, new) {
        let equal = matches!(op, DiffOp::Equal { .. });
        push_seg(
            &mut segs,
            Seg {
                old: op.old_range(),
                new: op.new_range(),
                equal,
            },
        );
    }
    segs
}

impl Seg {
    fn extend(&mut self, to: &Seg) {
        self.old.end = to.old.end;
        self.new.end = to.new.end;
    }
}

/// Widens changes so that no match of `strings` in the text of `new` crosses
/// the edge of a change: a mark cuts the text, and a text show rule matches
/// within one piece only.
fn cover_shown_strings(segs: &mut Vec<Seg>, new: &[Tok], strings: &HashSet<String>) {
    if strings.is_empty() {
        return;
    }
    // The text as laid out, with a character no string holds for code.
    let mut text = String::new();
    let mut starts = vec![];
    for t in new {
        starts.push(text.len());
        match t.kind {
            TK::Word => text.push_str(&t.key),
            TK::Space => text.push(if t.key == " " { ' ' } else { '\n' }),
            TK::Atom => text.push('\u{fffc}'),
            TK::Comment => {}
        }
    }
    let tok_at = |byte: usize| starts.partition_point(|&s| s <= byte) - 1;
    loop {
        let seg_of = |tok: usize| segs.iter().position(|s| s.new.contains(&tok)).unwrap();
        let cut = strings.iter().find_map(|p| {
            text.match_indices(p.as_str()).find_map(|(a, m)| {
                let (first, last) = (tok_at(a), tok_at(a + m.len() - 1) + 1);
                let (k, l) = (seg_of(first), seg_of(last - 1));
                (k != l).then_some((k, l, first, last))
            })
        });
        let Some((k, l, first, last)) = cut else { return };
        // Kept tokens outside the match stay kept; old and new tokens of a
        // kept run correspond one to one.
        let (a, b) = (&segs[k], &segs[l]);
        let before = if a.equal { first - a.new.start } else { 0 };
        let after = if b.equal { b.new.end - last } else { 0 };
        let pieces = [
            Seg {
                old: a.old.start..a.old.start + before,
                new: a.new.start..a.new.start + before,
                equal: true,
            },
            Seg {
                old: a.old.start + before..b.old.end - after,
                new: a.new.start + before..b.new.end - after,
                equal: false,
            },
            Seg {
                old: b.old.end - after..b.old.end,
                new: b.new.end - after..b.new.end,
                equal: true,
            },
        ];
        let rest: Vec<Seg> = segs.drain(k..).skip(l + 1 - k).collect();
        for s in pieces.into_iter().chain(rest) {
            push_seg(segs, s);
        }
    }
}

/// Push `s`, merged into the last segment if both are changes or both the
/// same.
fn push_seg(segs: &mut Vec<Seg>, s: Seg) {
    match segs.last_mut() {
        _ if s.old.is_empty() && s.new.is_empty() => {}
        Some(last) if last.equal == s.equal => last.extend(&s),
        _ => segs.push(s),
    }
}

/// [`segments`] of tokens compared by their text; but in a changed run,
/// code that differs only in markup (the same [`Tok::key`]) is the same,
/// each new piece with the most similar old one, so that a call is compared
/// with the one it resembles and not with a neighbour.
fn token_segments(old: &[Tok], new: &[Tok]) -> Vec<Seg> {
    let exact = |toks: &[Tok]| -> Vec<String> {
        toks.iter()
            .map(|t| if t.kind == TK::Atom { &t.text } else { &t.key }.clone())
            .collect()
    };
    let words_of = |t: &Tok| words(std::slice::from_ref(t));
    let seg = |old, new, equal| Seg { old, new, equal };
    let mut segs = vec![];
    for s in segments(&exact(old), &exact(new)) {
        if s.equal {
            push_seg(&mut segs, s);
            continue;
        }
        let (mut p, mut from) = (s.old.start, s.new.start);
        for n in s.new.clone().filter(|&n| new[n].kind == TK::Atom) {
            let nw = words_of(&new[n]);
            // A bounded number of candidates keeps the cost linear.
            let best = (p..s.old.end)
                .filter(|&q| old[q].key == new[n].key)
                .take(32)
                .map(|q| (q, similarity(&words_of(&old[q]), &nw)))
                .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)));
            if let Some((q, _)) = best {
                push_seg(&mut segs, seg(p..q, from..n, false));
                push_seg(&mut segs, seg(q..q + 1, n..n + 1, true));
                (p, from) = (q + 1, n + 1);
            }
        }
        push_seg(&mut segs, seg(p..s.old.end, from..s.new.end, false));
    }
    segs
}

impl<'a> Writer<'a> {
    fn new(ctx: &'a Ctx) -> Self {
        Writer {
            ctx,
            file: File::default(),
            out: String::new(),
            entry_out: String::new(),
            files: vec![],
            done: HashSet::new(),
            top: true,
            summary_at: None,
            sections: vec![],
            current: None,
            changes: vec![],
            old_scope: false,
            after_item: false,
            arrived: HashMap::new(),
            content_code: false,
            hoisted: None,
            bare: Bare::default(),
        }
    }

    /// Mark the changes in the new file at `path` against its old file,
    /// once; an unchanged file other than the entry is not written.
    fn file(&mut self, path: &str) {
        let ctx = self.ctx;
        let Some(index) = ctx.aligned.iter().position(|(p, _)| p == path) else {
            return;
        };
        let a = &ctx.aligned[index].1;
        if !self.done.insert(path.to_string()) {
            return;
        }
        let entry = path == ctx.entry();
        let old_path = a.old_path.clone();
        let (o, n) = (a.sources.0.root(), a.sources.1.root());
        let (mut removed, kept) = (definitions(o), definitions(n));
        removed.retain(|d| !kept.contains(d));
        removed.extend(ctx.removed_defs.iter().cloned());
        let file = File {
            path: path.into(),
            old_path,
            indent: String::new(),
            depth: 0,
            removed,
            index,
        };
        let outer = std::mem::replace(&mut self.file, file);
        let out = std::mem::take(&mut self.out);
        self.write_steps(&a.docs.0, &a.docs.1, &a.steps, true, false);
        let marked = std::mem::replace(&mut self.out, out);
        self.file = outer;
        if entry {
            self.entry_out = marked;
        } else {
            let marked = format!("{}{marked}", ctx.import());
            self.files.push((path.into(), marked));
        }
    }

    /// Mark the file a new `#include` block includes, here, so that its
    /// sections come in reading order.
    fn included(&mut self, b: &Block) {
        if let Some(path) = b.include_target().and_then(|t| resolve(&self.file.path, &t)) {
            self.file(&path);
        }
    }

    fn record(&mut self, kind: ChangeKind, text: String) {
        self.changes.push(Change {
            file: self.file.path.clone(),
            section: self.current.clone(),
            kind,
            text,
            old: String::new(),
            what: vec![],
            joined: false,
        });
    }

    /// The changes recorded from `start` on are one change at one place.
    fn join_from(&mut self, start: usize) {
        for c in self.changes.iter_mut().skip(start + 1) {
            c.joined = true;
        }
    }

    /// Record changed code that is not marked, `old` or `new` empty if it
    /// was inserted or deleted.
    fn list(&mut self, old: &str, new: &str) {
        self.list_what(old, new, vec![]);
    }

    /// [`Self::list`] with what changed in words; a file that the old code
    /// reads and that changed is added to it.
    fn list_what(&mut self, old: &str, new: &str, mut what: Vec<What>) {
        let (old, files) = self.ctx.unpin(old);
        if !new.is_empty() {
            what.extend(files.into_iter().map(What::File));
        }
        let (old, new) = differing_lines(&old, new);
        self.record(ChangeKind::Unmarked, new);
        let c = self.changes.last_mut().unwrap();
        c.old = old;
        c.what = what;
    }

    /// Emit `body`, the markup for `toks`, wrapped as a change, and record
    /// it. Deleted code sees the old file's definitions, which read the old
    /// version's files, so that it shows what was deleted.
    fn mark(&mut self, kind: ChangeKind, args: &str, body: String, toks: &[Tok]) {
        let step = self.step(toks);
        self.emit(kind, args, body, toks, step);
    }

    /// [`Self::mark`] with the step to show, if any.
    fn emit(&mut self, kind: ChangeKind, args: &str, body: String, toks: &[Tok], step: Option<u8>) {
        let call = self.call(kind, args, &body, step, self.changes.len());
        let old_scope = kind == ChangeKind::Del
            && self.ctx.opts.eval_deleted
            && !self.file.old_path.is_empty()
            && toks.iter().any(|t| t.kind == TK::Atom);
        if old_scope {
            self.old_scope = true;
            let old = typst_str(&format!("/{OLD_DIR}/{}", self.file.old_path));
            self.out.push_str(&format!("#{{import {old}: *; {call}}};"));
        } else {
            self.out.push_str(&format!("#{call};"));
        }
        self.record(kind, body);
    }

    /// The call that marks `body` as change `id`, without `#`.
    fn call(&self, kind: ChangeKind, args: &str, body: &str, step: Option<u8>, id: usize) -> String {
        let step = step.map_or(String::new(), |s| format!("step: {s}, "));
        let inline = !["block:", "heading:", "item:"].iter().any(|k| args.contains(k));
        let args = match (self.bare.edges(kind).filter(|_| inline), args) {
            (Some(e), "") => format!("bare: \"{e}\""),
            (Some(e), _) => format!("{args}, bare: \"{e}\""),
            (None, _) => args.to_string(),
        };
        let args = match args.as_str() {
            "" => format!("{step}id: {id}"),
            _ => format!("{args}, {step}id: {id}"),
        };
        format!("{}-{}({args})[{body}]", self.ctx.prefix, kind.name())
    }

    /// Whether deleted code can be evaluated with the definitions of its
    /// file.
    fn evaluates_deleted(&self) -> bool {
        self.ctx.opts.eval_deleted && !self.file.old_path.is_empty()
    }

    /// Code markup `old` (as [`Self::sanitize`] gives it) replaced by `new`,
    /// recorded as a deletion and an insertion, as a code block: the new
    /// markup alone where both show the same, so that the marks are not laid
    /// out and the summary lists them; else, given `arg`, the old value of
    /// an argument of the new code's call (as [`Self::marked_arg`] takes it)
    /// and the new code written with that argument as it is, the argument
    /// deleted before the new code if it is content; else old and new whole.
    /// Content compares equal whatever the styles it sets, its labels and
    /// the code of its `context` expressions, so content with any of these
    /// is taken to differ.
    fn replaced(
        &mut self,
        old: &str,
        new: &str,
        toks: (&[Tok], &[Tok]),
        arg: Option<(&str, &str)>,
        block: bool,
    ) -> String {
        let (id, steps) = (self.changes.len(), (self.step(toks.0), self.step(toks.1)));
        let nodes = |t: &[Tok]| t.iter().flat_map(|t| t.nodes.clone()).collect::<Vec<_>>();
        let what = self.call_args(&nodes(toks.0), &nodes(toks.1));
        self.record(ChangeKind::Del, old.into());
        self.changes[id].what = what;
        self.record(ChangeKind::Ins, new.into());
        self.join_from(id);
        self.old_scope = true;
        let p = &self.ctx.prefix;
        let (dargs, iargs) = if block {
            ("block: true", "block: true")
        } else {
            ("gap: true", "")
        };
        let mark = |kind, args, body: &str, id| {
            self.call(
                kind,
                args,
                body,
                if kind == ChangeKind::Del { steps.0 } else { steps.1 },
                id,
            )
        };
        let both = |o: &str, n: &str| {
            format!(
                "[#{}#{}]",
                mark(ChangeKind::Del, dargs, o, id),
                mark(ChangeKind::Ins, iargs, n, id + 1)
            )
        };
        let file = typst_str(&format!("/{OLD_DIR}/{}", self.file.old_path));
        let old_value = |code: &str| format!("{{import {file}: *; {code}}}");
        let by_arg = match arg {
            // The old body sits where the new call puts its body: a figure
            // centres it.
            Some((value, marked)) => format!(
                "let {p}-arg = {}; if type({p}-arg) == content [#{{let d = {}; if figure in ({p}-new.func(), ..{p}-new.fields().at(\"children\", default: ()).map(c => c.func())) {{ align(center, d) }} else {{ d }}}}#{}] else ",
                old_value(value),
                mark(ChangeKind::Del, dargs, &format!("#{p}-arg"), id),
                mark(ChangeKind::Ins, iargs, marked, id + 1)
            ),
            None => String::new(),
        };
        format!(
            "{{let {p}-old = {}; let {p}-new = [{new}]; let {p}-plain(c) = if type(c) == content {{ c.func() not in ((context none).func(), [#set text(red); a].func()) and not c.has(\"label\") and c.fields().values().all({p}-plain) }} else if type(c) == array {{ c.all({p}-plain) }} else {{ true }}; if {p}-old == {p}-new and {p}-plain({p}-new) {{ {p}-new }} else {{ {by_arg}{} }}}}",
            old_value(&format!("[{old}]")),
            both(&format!("#{p}-old"), &format!("#{p}-new"))
        )
    }

    /// Mark inserted tokens; with steps, each run of tokens that one step
    /// wrote separately.
    fn ins(&mut self, args: &str, toks: &[Tok]) {
        let (Some(last), steps) = (self.ctx.last_step(), self.tok_steps(toks)) else {
            return self.mark(ChangeKind::Ins, args, self.content(toks, ChangeKind::Ins), toks);
        };
        let runs: Vec<&[u8]> = steps.chunk_by(|a, b| a == b).collect();
        // Runs of several steps are parts of one block, each laid out inline.
        let args = if runs.len() > 1 { "" } else { args };
        let start = self.changes.len();
        let mut i = 0;
        for run in runs {
            let (lead, core, trail) = trim_spaces(&toks[i..i + run.len()]);
            i += run.len();
            self.out.push_str(&lead);
            if !core.is_empty() {
                let step = if run[0] == 0 { last } else { run[0] };
                self.emit(
                    ChangeKind::Ins,
                    args,
                    self.content(core, ChangeKind::Ins),
                    core,
                    Some(step),
                );
            }
            self.out.push_str(&trail);
        }
        self.join_from(start);
    }

    /// The step of each token: the one that wrote most of its source, or
    /// for a deletion removed it. The steps can keep text that the direct
    /// comparison sees as changed, because they align it differently: such
    /// a token takes the step that changed text next to it, else the step
    /// of a neighbouring token; 0 if none has one.
    fn tok_steps(&self, toks: &[Tok]) -> Vec<u8> {
        if self.ctx.steps.is_none() {
            return vec![];
        }
        let mut out: Vec<u8> = toks
            .iter()
            .map(|t| {
                let Some((src, steps)) = self.ctx.blamed(t) else {
                    return 0;
                };
                let text = src.text().as_bytes();
                let Some(node) = src.find(t.at.0) else { return 0 };
                let start = (node.offset() + t.at.1).min(text.len());
                let end = (start + t.source_len()).min(text.len());
                let bytes = || (start..end).filter(|&i| !text[i].is_ascii_whitespace());
                majority(bytes().map(|i| (steps[i].0, 1)))
                    .or_else(|| majority(bytes().map(|i| (steps[i].1, 1))))
                    .unwrap_or(0)
            })
            .collect();
        for i in 1..out.len() {
            if out[i] == 0 {
                out[i] = out[i - 1];
            }
        }
        for i in (1..out.len()).rev() {
            if out[i - 1] == 0 {
                out[i - 1] = out[i];
            }
        }
        out
    }

    /// The step a mark of `toks` shows: the one that made most of it, else
    /// the last.
    fn step(&self, toks: &[Tok]) -> Option<u8> {
        let last = self.ctx.last_step()?;
        let steps = self.tok_steps(toks);
        let weight = |t: &Tok| if t.kind == TK::Space { 0 } else { t.source_len() };
        Some(majority(steps.into_iter().zip(toks.iter().map(weight))).unwrap_or(last))
    }

    /// Setup code written as in the new version, its change from `old`
    /// listed; an inserted `#import` or `#include` is not, as the files it
    /// names show what it brings.
    fn unmarked(&mut self, old: &str, b: &Block) {
        self.out.push_str(&b.text());
        if !b.is_import() || !old.is_empty() {
            let what = self.call_args(&parse(old).children().cloned().collect::<Vec<_>>(), &b.nodes);
            self.list_what(old, &b.text(), what);
        }
    }

    /// The changed arguments of the first call in code `o` and `n`, if
    /// both call the same.
    fn call_args(&self, o: &[SyntaxNode], n: &[SyntaxNode]) -> Vec<What> {
        match (outer_call(o), outer_call(n)) {
            (Some((oc, oargs)), Some((nc, nargs))) if oc == nc => self.changed_args(&nc, &oargs, &nargs),
            _ => vec![],
        }
    }

    /// The entry's output source: the preamble import and the marked
    /// document, which from the place [`Self::summary_here`] chose on is
    /// drawn with bars and followed by the summary.
    fn finish(&self, old_label: &str, new_label: &str, bib: &Bib, history: &History) -> String {
        let at = self.summary_at.unwrap_or(self.entry_out.len());
        let mut out = self.entry_out.clone();
        let newline = if out[..at].ends_with('\n') || at == 0 { "" } else { "\n" };
        let p = &self.ctx.prefix;
        let start = format!(
            "{newline}#show: body => {{ {p}-bars(body); {} }}\n",
            self.summary(old_label, new_label, bib, history)
        );
        out.insert_str(at, &start);
        format!("{}{out}", self.ctx.import())
    }

    fn summary(&self, old_label: &str, new_label: &str, bib: &Bib, history: &History) -> String {
        let count = |f: &dyn Fn(&Change) -> bool| self.changes.iter().filter(|c| f(c)).count();
        let front = |file: &str, title: String| Section {
            file: file.into(),
            level: 1,
            target: None,
            title,
            deleted: false,
            new: false,
            moved: None,
        };
        // Changes outside sections: before the entry's first heading, and in
        // files that are not read in order (imported, or included by code).
        let first = front(self.ctx.entry(), "[Start of the document]".into());
        let others: Vec<Section> = self
            .files
            .iter()
            .map(|(p, _)| front(p, format!("[In #raw({})]", typst_str(p))))
            .collect();
        // A heading deleted where a heading with the same title remains was
        // changed or moved, not deleted: its changes count with the section
        // before it.
        let moved = |s: &Section| {
            s.deleted
                && self
                    .sections
                    .iter()
                    .any(|t| !t.deleted && collapse_ws(&t.title) == collapse_ws(&s.title))
        };
        let mut listed: Vec<(&Section, Vec<&Section>)> = vec![];
        for s in std::iter::once(&first).chain(&self.sections).chain(&others) {
            match listed.last_mut() {
                Some((_, parts)) if moved(s) => parts.push(s),
                _ => listed.push((s, vec![s])),
            }
        }
        // A move into a moved section of blocks that belonged to the section
        // in the old version, after its heading, is part of the section's move.
        let part_of = |r: usize| {
            let Some(s) = self
                .arrived
                .get(&r)
                .and_then(|t| self.sections.iter().find(|s| s.target == *t))
            else {
                return r;
            };
            let Some(own) = s.moved else { return r };
            let (a, b) = (&self.ctx.moves[own].old, &self.ctx.moves[r].old);
            let blocks = &self.ctx.aligned[a.0].1.docs.0.blocks;
            let within = a.0 == b.0
                && a.1.start < b.1.start
                && blocks[a.1.start + 1..b.1.start]
                    .iter()
                    .all(|x| x.kind != Kind::Heading || x.heading_level() > s.level);
            if within { own } else { r }
        };
        let mut entries = String::new();
        for (s, parts) in listed {
            let here = |c: &Change| {
                parts
                    .iter()
                    .any(|p| c.section == p.target && (p.target.is_some() || c.file == p.file))
            };
            // A move is counted apart from edits, once, in the section it
            // reached; a moved section's own move is its label.
            let n = count(&|c| here(c) && c.kind != ChangeKind::Moved && !c.joined);
            let moves: HashSet<usize> = self
                .changes
                .iter()
                .filter(|c| here(c) && c.kind == ChangeKind::Moved && !c.old.is_empty())
                .filter_map(|c| c.text.parse().ok())
                .map(part_of)
                .collect();
            // Moves that left the section are not counted here, but listed
            // with their numbers, which their old places show.
            let left = self
                .changes
                .iter()
                .any(|c| here(c) && c.kind == ChangeKind::Moved && c.old.is_empty());
            if n == 0 && moves.is_empty() && !left && !s.deleted {
                continue;
            }
            let moves = moves.len() - usize::from(s.moved.is_some_and(|r| moves.contains(&r)));
            let code: String = self
                .changes
                .iter()
                .enumerate()
                .filter(|(_, c)| here(c) && c.kind == ChangeKind::Unmarked)
                .map(|(id, c)| format!("(id: {id}, {}),", c.code_fields()))
                .collect();
            // Marks that show only a bar, with what they mark in words.
            let bars: String = self
                .changes
                .iter()
                .enumerate()
                .filter(|(_, c)| here(c) && c.kind == ChangeKind::Ins && !c.what.is_empty())
                .map(|(id, c)| {
                    format!(
                        "(id: {id}, {}),",
                        Change {
                            old: c.text.clone(),
                            ..c.clone()
                        }
                        .code_fields()
                    )
                })
                .collect();
            // A move is checked at its new place; at its old place it is
            // `from`.
            let marks: String = self
                .changes
                .iter()
                .enumerate()
                .filter(|(_, c)| here(c))
                .filter(|(_, c)| matches!(c.kind, ChangeKind::Ins | ChangeKind::Del | ChangeKind::Moved))
                .map(|(id, c)| {
                    let mut m = c.unmarked();
                    m.old = self.ctx.unpin(&m.old).0;
                    let mv = match c.kind {
                        ChangeKind::Moved if c.old.is_empty() => format!("move: {}, from: true, ", c.text),
                        ChangeKind::Moved => format!("move: {}, ", c.text),
                        _ if c.joined => "joined: true, ".into(),
                        // Replaced code, in words where not laid out.
                        ChangeKind::Del if !c.what.is_empty() => {
                            format!(
                                "what: ({}), ",
                                c.what.iter().map(|w| w.to_typst() + ",").collect::<String>()
                            )
                        }
                        _ => String::new(),
                    };
                    format!(
                        "(id: {id}, {mv}old: {}, new: {}),",
                        typst_str(&m.old),
                        typst_str(&m.text)
                    )
                })
                .collect();
            let target = match &s.target {
                Some(l) if !s.deleted => format!("<{l}>"),
                _ => "none".into(),
            };
            // Moves are listed only where there are some.
            let moved = match (moves, s.moved.is_some()) {
                (0, false) => String::new(),
                (_, false) => format!("moves: {moves}, "),
                _ => format!("moves: {moves}, moved: true, "),
            };
            entries.push_str(&format!(
                "  (file: {}, level: {}, target: {target}, title: {}, count: {n}, {moved}deleted: {}, new: {}, code: ({code}), bars: ({bars}), marks: ({marks})),\n",
                typst_str(&s.file),
                s.level,
                s.title,
                s.deleted,
                s.new,
            ));
        }
        let sections: String = self
            .sections
            .iter()
            .filter(|s| !s.deleted)
            .filter_map(|s| s.target.as_ref())
            .map(|l| format!("<{l}>, "))
            .collect();
        let p = &self.ctx.prefix;
        let history = history
            .to_typst(old_label, new_label)
            .map_or(String::new(), |h| format!("  history: {h},\n"));
        format!(
            "{p}-summary((\n  old: {}, new: {}, ins: {}, del: {},{}\n  sections: ({sections}),\n  entries: (\n{entries}  ),\n  bib: {},\n{history}))",
            typst_str(old_label),
            typst_str(new_label),
            count(&|c| c.kind == ChangeKind::Ins),
            count(&|c| c.kind == ChangeKind::Del),
            match self.ctx.moves.len() {
                0 => String::new(),
                n => format!(" moved: {n},"),
            },
            bib.to_typst(),
        )
    }

    /// The bars and the summary go at the top level of the entry, in a show
    /// rule before its first show rule and its first content, so that the
    /// document's template and show rules do not apply to the summary.
    fn summary_here(&mut self) {
        if self.top && self.file.path == self.ctx.entry() && self.summary_at.is_none() {
            self.summary_at = Some(self.out.len());
        }
    }

    fn begin_new(&mut self, b: &Block) {
        let config = b.kind == Kind::Setup
            && matches!(
                b.nodes[1].kind(),
                K::SetRule | K::ShowRule | K::LetBinding | K::ModuleImport
            );
        if !config {
            self.summary_here();
        }
        // A heading or item starts a line, also after a deletion.
        let line = matches!(b.kind, Kind::Heading | Kind::Item) && !b.gap.contains('\n');
        if line && !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        self.out.push_str(&b.gap);
        self.after_item = b.kind == Kind::Item;
        if config && b.nodes[1].kind() == K::ShowRule {
            // The bars wrap the rest of the document in a context, which
            // must not hide the document's content from a template that a
            // show rule applies, such as a slide package's.
            self.summary_here();
        }
    }

    /// If `b` is a heading the summary lists, start its section; returns the
    /// label to put after it if it has none.
    fn section(&mut self, b: &Block, new: bool) -> String {
        if b.kind != Kind::Heading || b.heading_level() > 2 || !self.top {
            return String::new();
        }
        let (target, label) = match b.label() {
            Some(l) => (l, String::new()),
            None => {
                let l = format!("{}-sec-{}", self.ctx.prefix, self.sections.len());
                (l.clone(), format!(" <{l}>"))
            }
        };
        let title: String = b
            .toks()
            .iter()
            .filter(|t| t.kind != TK::Comment)
            .map(|t| t.text.as_str())
            .collect();
        self.sections.push(Section {
            file: self.file.path.clone(),
            level: b.heading_level(),
            target: Some(target.clone()),
            title: format!("[{}]", title.trim()),
            deleted: false,
            new,
            moved: None,
        });
        self.current = Some(target);
        label
    }

    /// Write `new` marked against `old`; `head` if both follow the text of
    /// an item, as its nested items do.
    fn docs(&mut self, old: &Doc, new: &Doc, head: bool) {
        self.write_steps(old, new, &align(old, new), false, head);
    }

    /// Write `new` by `steps`; at the top level of a file (`top`), blocks
    /// that moved are written as moves.
    fn write_steps(&mut self, old: &Doc, new: &Doc, steps: &[Step], top: bool, head: bool) {
        let ctx = self.ctx;
        let file = self.file.index;
        let moved = |map: &'a HashMap<(usize, usize), usize>, b: usize| map.get(&(file, b)).filter(|_| top);
        let old_of: HashMap<usize, usize> = steps
            .iter()
            .filter_map(|s| match *s {
                Step::Keep(Some(o), n) | Step::Pair(o, n) => Some((n, o)),
                _ => None,
            })
            .collect();
        // Only a deleted section's own blocks count with it.
        let mut outer = None;
        // A deleted item is written in the body of the item before it.
        let mut in_item = false;
        // Blocks deleted and inserted next to each other in one section are
        // one change, from its first change on; a heading starts another.
        let mut run: Option<(usize, Option<String>)> = None;
        for (k, &s) in steps.iter().enumerate() {
            match s {
                Step::Del(o) if old.blocks[o].kind == Kind::Heading => {
                    outer.get_or_insert_with(|| self.current.clone());
                }
                Step::Del(_) => {}
                _ => {
                    if let Some(c) = outer.take() {
                        self.current = c;
                    }
                }
            }
            let block = match s {
                Step::Del(o) if moved(&ctx.moved_from, o).is_none() => Some(&old.blocks[o]),
                Step::Ins(n) if moved(&ctx.moved_to, n).is_none() => Some(&new.blocks[n]),
                _ => None,
            };
            let start = (self.changes.len(), self.current.clone());
            run = match block.map(|b| b.kind) {
                None | Some(Kind::Setup) => None,
                Some(Kind::Heading) => Some(start),
                _ => run.filter(|r| r.1 == start.1).or(Some(start)),
            };
            match s {
                Step::Del(o) => {
                    // A list that ends here in the new version ends before
                    // a deleted block that is not an item.
                    let next = steps[k + 1..].iter().find_map(|s| match *s {
                        Step::Keep(_, n) | Step::Pair(_, n) | Step::Ins(n) => Some(&new.blocks[n]),
                        Step::Del(_) => None,
                    });
                    let list_ends = self.after_item && next.is_none_or(|b| b.kind != Kind::Item);
                    match moved(&ctx.moved_from, o) {
                        Some(&r) if ctx.moves[r].old.1.start == o => self.moved_from(r, list_ends),
                        Some(_) => {}
                        None if top && self.moved_setup(&old.blocks[o]) => {}
                        None => {
                            // A paragraph that went on after setup code that
                            // is kept stays in its paragraph.
                            let ob = &old.blocks[o];
                            let after_setup = k > 0
                                && matches!(steps[k - 1], Step::Keep(Some(p), _) | Step::Pair(p, _)
                                    if p + 1 == o && old.blocks[p].kind == Kind::Setup);
                            let inline = ob.kind == Kind::Para && after_setup && !parbreak(&ob.gap);
                            let args = self.item_args(old, o, in_item).or(inline.then(String::new));
                            self.deleted(ob, list_ends, args)
                        }
                    }
                }
                Step::Ins(n) => match moved(&ctx.moved_to, n) {
                    Some(&r) if ctx.moves[r].new.1.start == n => self.moved_to(r),
                    Some(_) => {}
                    None if top && self.moved_setup(&new.blocks[n]) => self.moved_code(&new.blocks[n]),
                    None => self.write_new(old, new, s, ("", "")),
                },
                _ => {
                    let spacing = self.spacing(old, new, s, &old_of, head);
                    self.write_new(old, new, s, (&spacing, ""))
                }
            }
            if let Step::Keep(_, n) | Step::Pair(_, n) | Step::Ins(n) = s {
                in_item = new.blocks[n].kind == Kind::Item;
            }
            if let Some((start, section)) = &mut run {
                self.join_from(*start);
                *section = self.current.clone();
            }
        }
        if let Some(c) = outer {
            self.current = c;
        }
        self.out.push_str(&new.trailing);
    }

    /// For a kept or changed item whose separation from the paragraph or item
    /// before it changed, from a blank line to none or back, which spaces the
    /// items differently: an empty mark for the start of the item, which
    /// draws a bar there, recorded with the item's first line and a note.
    fn spacing(&mut self, old: &Doc, new: &Doc, s: Step, old_of: &HashMap<usize, usize>, head: bool) -> String {
        let (Step::Keep(Some(o), n) | Step::Pair(o, n)) = s else {
            return String::new();
        };
        let nb = &new.blocks[n];
        let (Some((np, apart)), Some((op, was_apart))) = (attachment(new, n, head), attachment(old, o, head)) else {
            return String::new();
        };
        if apart == was_apart || np.map(|p| old_of.get(&p).copied()) != op.map(Some) {
            return String::new();
        }
        let note = if apart {
            "Blank line added before this item: it is spaced apart from the text before it."
        } else {
            "Blank line before this item removed: it is spaced close to the text before it."
        };
        let line = nb.text().trim().lines().next().unwrap_or_default().to_string();
        let step = self.step(&[]).map_or(String::new(), |s| format!("step: {s}, "));
        let call = format!("#{}-ins({step}id: {})[];", self.ctx.prefix, self.changes.len());
        self.record(ChangeKind::Ins, line);
        self.changes.last_mut().unwrap().what = vec![What::Note(note.into())];
        call
    }

    /// A step with `marks` at the start and the end of the new block's
    /// body; a deletion inside a passage that goes on after it.
    fn write_new(&mut self, old: &Doc, new: &Doc, s: Step, marks: (&str, &str)) {
        match s {
            Step::Keep(_, n) => self.kept(&new.blocks[n], marks),
            Step::Pair(o, n) => self.pair(&old.blocks[o], &new.blocks[n], marks),
            Step::Ins(n) => self.inserted(&new.blocks[n], marks),
            Step::Del(o) => self.deleted(&old.blocks[o], false, None),
        }
    }

    /// The step of the commit that made move `r`, with the steps shown.
    fn move_step(&self, r: usize) -> Option<u8> {
        let run = &self.ctx.moves[r];
        let blocks = &self.ctx.aligned[run.new.0].1.docs.1.blocks[run.new.1.clone()];
        self.step(&blocks.iter().flat_map(|b| b.toks()).collect::<Vec<_>>())
    }

    /// The call that marks an end of move `r`: `kind` "del" for the line at
    /// the old place, "ins" for the empty markers around the new place.
    fn move_call(&self, kind: &str, r: usize, args: &str, body: &str) -> String {
        let step = self.move_step(r).map_or(String::new(), |s| format!(", step: {s}"));
        let key = self.ctx.moves[r].key;
        format!(
            "#{}-{kind}(move: (id: {r}, key: {key}, {args}){step})[{body}];",
            self.ctx.prefix
        )
    }

    /// The old place of move `r`: one line that names the start of what
    /// moved and links to its new place.
    fn moved_from(&mut self, r: usize, list_ends: bool) {
        let run = &self.ctx.moves[r];
        let blocks = &self.ctx.aligned[run.old.0].1.docs.0.blocks[run.old.1.clone()];
        let ob = &blocks[0];
        let (note, more) = self.move_note(ob);
        let args = format!(
            "heading: {}, down: {}, more: {}",
            ob.kind == Kind::Heading,
            !self.arrived.contains_key(&r),
            more || blocks.len() > 1
        );
        self.summary_here();
        self.out.push_str(&self.continuation(ob, list_ends));
        self.out.push_str(&self.move_call("del", r, &args, &note));
        self.record(ChangeKind::Moved, r.to_string());
    }

    /// The first words of a block, as markup, and whether the block goes on
    /// after them: of a figure, those of its caption; of other code, those
    /// of its content arguments; a display equation inline.
    fn move_note(&self, b: &Block) -> (String, bool) {
        let toks = b.toks();
        self.first_words(&toks)
            .unwrap_or_else(|| match toks.iter().find(|t| t.kind == TK::Atom) {
                Some(t) => (placeholder(&t.nodes), true),
                None => (String::new(), false),
            })
    }

    /// [`Self::move_note`], if the tokens start with text.
    fn first_words(&self, toks: &[Tok]) -> Option<(String, bool)> {
        const WORDS: usize = 30;
        let (_, core, _) = trim_spaces(toks);
        if let Some(t) = core.first().filter(|t| t.kind == TK::Atom) {
            let more = core.len() > 1;
            if let Some((words, inner)) = caption(&t.nodes).and_then(|c| self.first_words(&tokenize(&c))) {
                return Some((format!("Figure: {words}"), more || inner));
            }
            if t.nodes[0].kind() == K::Equation && t.block {
                let math = self.sanitize(&t.nodes).and_then(|src| {
                    let inner = src.trim().strip_prefix('$')?.strip_suffix('$')?;
                    Some(format!("${}$", collapse_ws(inner)))
                });
                return Some((math.unwrap_or_else(|| placeholder(&t.nodes)), more));
            }
            if t.nodes[0].kind() == K::Hash
                && let Some((words, inner)) = content_blocks(&t.nodes)
                    .iter()
                    .find_map(|m| self.first_words(&tokenize(m)))
            {
                return Some((words, more || inner));
            }
        }
        let (mut end, mut words, mut more) = (0, 0, false);
        for t in toks {
            let code = t.kind == TK::Atom && (t.block || t.nodes[0].kind() == K::Hash);
            let word = t.kind == TK::Atom || t.kind == TK::Word && t.text.chars().any(char::is_alphanumeric);
            if code || word && words == WORDS {
                more = true;
                break;
            }
            words += usize::from(word);
            end += 1;
        }
        let (_, core, _) = trim_spaces(&toks[..end]);
        let one_line: Vec<Tok> = core
            .iter()
            .map(|t| match t.kind {
                TK::Space => Tok {
                    text: " ".into(),
                    ..t.clone()
                },
                _ => t.clone(),
            })
            .collect();
        (!core.is_empty()).then(|| (self.content(&one_line, ChangeKind::Del), more))
    }

    /// The new place of move `r`: its new blocks with the changes against
    /// its old blocks marked, between the markers of the new place.
    fn moved_to(&mut self, r: usize) {
        let ctx = self.ctx;
        let run = &ctx.moves[r];
        let (of, nf) = (&ctx.aligned[run.old.0].1, &ctx.aligned[run.new.0].1);
        let part = |d: &Doc, blocks: &Range<usize>| Doc {
            blocks: d.blocks[blocks.clone()].to_vec(),
            trailing: String::new(),
        };
        let (old, new) = (part(&of.docs.0, &run.old.1), part(&nf.docs.1, &run.new.1));
        let steps = align(&old, &new);
        let writes = |s: &Step| !matches!(s, Step::Del(_));
        let (first, last) = (
            steps.iter().position(writes).unwrap(),
            steps.iter().rposition(writes).unwrap(),
        );
        // A marker next to a block-level element or on a line of its own
        // must not be boxed, which would add a line.
        let block = |b: &Block, end: bool| {
            let toks = b.toks();
            let mut solid = toks.iter().filter(|t| !matches!(t.kind, TK::Space | TK::Comment));
            b.kind == Kind::Heading || if end { solid.next_back() } else { solid.next() }.is_some_and(|t| t.block)
        };
        let args = format!(
            "block: {}, heading: {}",
            block(&new.blocks[0], false),
            new.blocks[0].kind == Kind::Heading
        );
        let start = self.move_call("ins", r, &args, "");
        let end = self.move_call(
            "ins",
            r,
            &format!("end: true, block: {}", block(new.blocks.last().unwrap(), true)),
            "",
        );
        self.arrived.insert(r, None);
        let sections = self.sections.len();
        // Deleted code is evaluated with the definitions of its own file.
        let path = std::mem::replace(&mut self.file.old_path, of.old_path.clone());
        for (k, &s) in steps.iter().enumerate() {
            let marks = (
                if k == first { start.as_str() } else { "" },
                if k == last { end.as_str() } else { "" },
            );
            self.write_new(&old, &new, s, marks);
            if k == first {
                self.arrived.insert(r, self.current.clone());
                self.record(ChangeKind::Moved, r.to_string());
                self.changes.last_mut().unwrap().old = format!("Moved: {}", self.move_note(&old.blocks[0]).0);
            }
        }
        self.file.old_path = path;
        for s in &mut self.sections[sections..] {
            s.moved = Some(r);
        }
    }

    /// The text before a block's body and a move's start marker: after it,
    /// but before a heading on a line of its own, because outlines and
    /// running headers copy a heading's body.
    fn open(&mut self, b: &Block, prefix: &str, mark: &str) {
        if b.kind == Kind::Heading && !mark.is_empty() {
            self.out.push_str(&format!("{mark}\n{prefix}"));
        } else {
            self.out.push_str(prefix);
            self.out.push_str(mark);
        }
    }

    /// A move's end marker and the text after a block's body, as in
    /// [`Self::open`].
    fn close(&mut self, b: &Block, mark: &str, suffix: &str) {
        if b.kind == Kind::Heading && !mark.is_empty() {
            self.out.push_str(&format!("{suffix}\n{mark}"));
        } else {
            self.out.push_str(mark);
            self.out.push_str(suffix);
        }
    }

    /// A kept block with `marks` written at the start and the end of its
    /// body.
    fn kept(&mut self, b: &Block, marks: (&str, &str)) {
        self.begin_new(b);
        let label = self.section(b, false);
        let (prefix, body, suffix) = b.parts();
        self.open(b, &prefix, marks.0);
        self.out.push_str(&text(&body));
        self.close(b, marks.1, &(suffix + &label));
        self.included(b);
    }

    /// A changed block and the old block it replaces, with `marks` as in
    /// [`Self::kept`].
    fn pair(&mut self, ob: &Block, nb: &Block, marks: (&str, &str)) {
        self.begin_new(nb);
        if nb.kind == Kind::Setup {
            let (ot, nt) = (ob.toks(), nb.toks());
            // Code that only reads a changed file is listed whole, to show
            // where the file is read.
            let file_only = self.ctx.unpin(&ob.text()).0 == nb.text();
            if file_only || !(ot.len() == 1 && nt.len() == 1 && self.diff_code(&ot[0], &nt[0], true)) {
                self.unmarked(&ob.text(), nb);
            }
            self.included(nb);
            return;
        }
        let label = self.section(nb, false);
        let (prefix, nbody, suffix) = nb.parts();
        let (_, obody, osuffix) = ob.parts();
        // The text after a heading's or item's body is its label.
        if collapse_ws(&osuffix) != collapse_ws(&suffix) {
            self.list(osuffix.trim(), suffix.trim());
        }
        match (ob.term(), nb.term()) {
            (Some((_, oterm, _)), Some((marker, nterm, colon))) => {
                self.out.push_str(&marker);
                self.diff_tokens(&tokenize(&oterm), &tokenize(&nterm));
                self.open(nb, &colon, marks.0);
            }
            _ => self.open(nb, &prefix, marks.0),
        }
        // Nested items are blocks of their own, so that a change stays
        // inside its item.
        let (ohead, onested) = split_nested(&obody);
        let (nhead, nnested) = split_nested(&nbody);
        self.diff_tokens(&tokenize(ohead), &tokenize(nhead));
        let (odoc, ndoc) = (Doc::new(onested), Doc::new(nnested));
        let gap = ndoc.blocks.first().or(odoc.blocks.first()).map_or("", |b| &b.gap);
        let indent = gap.rsplit('\n').next().unwrap().to_string();
        let outer = std::mem::replace(&mut self.file.indent, indent);
        self.file.depth += 1;
        let text = |toks: Vec<Tok>| toks.iter().any(|t| t.kind != TK::Space && t.kind != TK::Comment);
        self.docs(&odoc, &ndoc, text(tokenize(ohead)) && text(tokenize(nhead)));
        self.file.depth -= 1;
        self.file.indent = outer;
        self.close(nb, marks.1, &(suffix + &label));
    }

    /// An inserted block, with `marks` as in [`Self::kept`].
    fn inserted(&mut self, nb: &Block, marks: (&str, &str)) {
        self.begin_new(nb);
        if nb.kind == Kind::Setup {
            let at = self.out.len();
            if nb.is_import() || !self.ctx.opts.content_blocks || !self.inserted_code(&nb.nodes) {
                self.out.truncate(at);
                self.unmarked("", nb);
            }
            self.included(nb);
            return;
        }
        let label = self.section(nb, true);
        let (prefix, nodes, suffix) = nb.parts();
        // A term is marked apart from its description: its colon must stay
        // outside the marks.
        match nb.term() {
            Some((marker, term, colon)) => {
                let toks = tokenize(&term);
                let (lead, core, trail) = trim_spaces(&toks);
                self.out.push_str(&(marker + &lead));
                if !core.is_empty() {
                    self.ins("block: true", core);
                }
                self.open(nb, &(trail + &colon), marks.0);
            }
            None => self.open(nb, &prefix, marks.0),
        }
        self.ins("block: true", &tokenize(&nodes));
        self.close(nb, marks.1, &(suffix + &label));
    }

    /// Whether top-level block `b` is setup code that moved unchanged
    /// within its file.
    fn moved_setup(&self, b: &Block) -> bool {
        b.kind == Kind::Setup && self.ctx.moved_setup.contains(&(self.file.index, b.key.clone()))
    }

    /// Setup code moved unchanged, at its new place: written unmarked and
    /// listed once.
    fn moved_code(&mut self, b: &Block) {
        self.begin_new(b);
        let text = b.text();
        self.out.push_str(&text);
        let note = "Moved here unchanged from another place in this file; content between the two places can now see other settings or definitions.";
        self.list_what(&text, &text, vec![What::Note(note.into())]);
    }

    /// The `item` argument of deleted list or enum item `o` of `old`, written
    /// in the body of the item before it if `in_item`.
    fn item_args(&self, old: &Doc, o: usize, in_item: bool) -> Option<String> {
        let kind = old.blocks[o].nodes[0].kind();
        if !matches!(kind, K::ListItem | K::EnumItem) {
            return None;
        }
        // The number the old version gave it: one more than the item before
        // it in the same enumeration, from the last explicit number, else
        // counted from the enumeration's start.
        let (mut number, mut explicit) = (0, false);
        if kind == K::EnumItem {
            for b in old.blocks[..=o]
                .iter()
                .rev()
                .take_while(|b| b.nodes[0].kind() == K::EnumItem)
            {
                let marker = b.parts().0;
                if let Ok(n) = marker.trim().trim_end_matches('.').parse::<usize>() {
                    number += n;
                    explicit = true;
                    break;
                }
                number += 1;
            }
        }
        let tight = attachment(old, o, false).is_none_or(|(_, apart)| !apart);
        let number = if kind == K::EnumItem {
            format!("{number}, counted: {}", !explicit)
        } else {
            "none".into()
        };
        Some(format!(
            "item: (depth: {}, number: {number}, inside: {in_item}, tight: {tight})",
            self.file.depth + 1
        ))
    }

    /// Deleted block `ob`, marked with `args` if given, else as a block.
    fn deleted(&mut self, ob: &Block, list_ends: bool, args: Option<String>) {
        if ob.kind == Kind::Setup {
            // A deleted include of a file that is gone shows its content,
            // through a root-relative path.
            let path = ob.include_target().and_then(|t| resolve(&self.file.old_path, &t));
            let paired = path.as_ref().is_some_and(|p| self.ctx.pairs.values().any(|o| o == p));
            let include = ob.nodes.get(1).is_some_and(|n| n.kind() == K::ModuleInclude);
            match path {
                Some(p) if !paired && source(&self.ctx.old_files, &p).is_some() => {
                    self.old_scope = true;
                    let root = parse(&format!("#include {}", typst_str(&format!("/{OLD_DIR}/{p}"))));
                    self.deleted(
                        &Block {
                            kind: Kind::Para,
                            gap: String::new(),
                            nodes: root.children().cloned().collect(),
                            key: String::new(),
                        },
                        list_ends,
                        None,
                    );
                }
                // An include whose content cannot be shown is listed.
                _ if !ob.is_import() || include && !paired => self.list(&ob.text(), ""),
                _ => {}
            }
            return;
        }
        let (prefix, nodes, _) = ob.parts();
        let toks = tokenize(&nodes);
        // A deleted term item shows its term: it is written as a
        // continuation of the block before it. List and enum items take
        // their marker from `item`.
        let marker = match ob.nodes[0].kind() {
            K::TermItem => prefix.trim().trim_start_matches('/').trim_start().to_string() + " ",
            _ => String::new(),
        };
        let body = marker + &self.content(&toks, ChangeKind::Del);
        let mut args = args.unwrap_or("block: true".into());
        if ob.kind == Kind::Heading {
            let level = ob.heading_level();
            if level <= 2 && self.top {
                // Its own deleted blocks are in this section; its key is
                // not a label.
                let key = format!("{}-del-{}", self.ctx.prefix, self.sections.len());
                self.sections.push(Section {
                    file: self.file.path.clone(),
                    level,
                    target: Some(key.clone()),
                    title: format!("[{body}]"),
                    deleted: true,
                    new: false,
                    moved: None,
                });
                self.current = Some(key);
            }
            args = format!("heading: {level}");
        }
        self.summary_here();
        self.out.push_str(&self.continuation(ob, list_ends));
        self.mark(ChangeKind::Del, &args, body, &toks);
    }

    /// The start of the line of deleted block `ob`: an indented
    /// continuation line, not a block or item of its own, that must not
    /// end a list, number an item or add a paragraph break that the new
    /// version lacks; but unindented after a list that ends here, so that
    /// a block that is not an item is not part of the last item.
    fn continuation(&self, ob: &Block, list_ends: bool) -> String {
        let indent = &self.file.indent;
        if list_ends && ob.kind != Kind::Item {
            format!("\n{indent}")
        } else {
            format!("\n{indent}  ")
        }
    }

    /// Markup for tokens placed inside a wrapper's content block.
    fn content(&self, toks: &[Tok], side: ChangeKind) -> String {
        let mut s = String::new();
        for t in toks {
            match t.kind {
                TK::Comment => {}
                TK::Space => s.push_str(&t.text),
                TK::Word => {
                    // Text that would start a list item, heading or term at
                    // the start of a line inside the wrapper.
                    let line = s.rsplit('\n').next().unwrap().trim();
                    let number = !line.is_empty() && line.chars().all(|c| c.is_ascii_digit());
                    let mut text = t.text.replace('[', "\\[").replace(']', "\\]");
                    if line.is_empty() && text.starts_with(['-', '+', '=', '/']) || number && text.starts_with('.') {
                        text.insert(0, '\\');
                    }
                    s.push_str(&text);
                }
                TK::Atom if side == ChangeKind::Del => match self.sanitize(&t.nodes) {
                    Some(src) => s.push_str(&src),
                    None => s.push_str(&placeholder(&t.nodes)),
                },
                TK::Atom => s.push_str(&t.text),
            }
        }
        // A line break would escape the closing bracket.
        if s.ends_with('\\') {
            s.push(' ');
        }
        s
    }

    /// Source for deleted content: labels removed or renamed, references to
    /// unknown targets turned into text. `None` if the code cannot be kept
    /// live: deleted code is not evaluated (and then neither is math using
    /// removed definitions), it would not mean what it meant (see
    /// [`Self::faithful`]), or it uses deleted labels.
    fn sanitize(&self, nodes: &[SyntaxNode]) -> Option<String> {
        if !self.faithful(nodes) {
            return None;
        }
        fn attached(n: &SyntaxNode, parent: K, names: &mut HashSet<String>) {
            if n.kind() == K::Label && matches!(parent, K::Named | K::Math) {
                names.insert(label_name(n));
            }
            for c in n.children() {
                attached(c, n.kind(), names);
            }
        }
        let mut renamed = HashSet::new();
        for n in nodes {
            attached(n, K::Markup, &mut renamed);
        }
        let mut out = String::new();
        for n in nodes {
            self.sanitize_into(n, K::Markup, &renamed, &mut out)?;
        }
        Some(out)
    }

    /// Whether old code, evaluated where the new file has it with the last
    /// definitions of its old file, means what it meant: no name it uses is
    /// bound in a block, function or loop around it, or at the top level of
    /// its file at or after it; and a name that its file does not bind before
    /// it, the new file binds nowhere.
    fn faithful(&self, nodes: &[SyntaxNode]) -> bool {
        let free = free_names(nodes);
        if free.is_empty() {
            return true;
        }
        let Some(span) = nodes.iter().map(SyntaxNode::span).find(|s| !s.is_detached()) else {
            return false;
        };
        let Some(a) = self
            .ctx
            .aligned
            .iter()
            .map(|(_, a)| a)
            .find(|a| Some(a.sources.0.id()) == span.id())
        else {
            return false;
        };
        let Some(at) = a.sources.0.find(span) else {
            return false;
        };
        let place = at.offset();
        let own: HashSet<Span> = nodes.iter().map(SyntaxNode::span).collect();
        let scope = std::iter::successors(Some(&at), |n| n.parent())
            .filter(|n| is_scope(n.kind()))
            .last();
        let local = scope.map_or(vec![], |s| all_bindings(s.get(), &own));
        let new_names = &self.ctx.aligned[self.file.index].1.new_names;
        let binds = |b: &Option<String>, x: &String| b.as_ref().is_none_or(|b| b == x);
        free.iter().all(|x| {
            let top = |after: bool| a.old_top.iter().any(|(o, b)| (*o >= place) == after && binds(b, x));
            !local.iter().any(|b| binds(b, x)) && !top(true) && (top(false) || !new_names.contains(x))
        })
    }

    fn sanitize_into(&self, n: &SyntaxNode, parent: K, renamed: &HashSet<String>, out: &mut String) -> Option<()> {
        let eval = self.ctx.opts.eval_deleted;
        match n.kind() {
            K::Label if parent == K::Markup => {}
            // A label passed as a named argument or written in math is one
            // that code attaches, possibly at layout where the preamble cannot
            // remove it; under another name it does not define the label a
            // second time. A positional label is taken for a reference, to
            // the renamed label when the same content attaches it.
            K::Label if matches!(parent, K::Named | K::Math) || renamed.contains(&label_name(n)) => {
                out.push_str(&format!("<{}-del-label-{}>", self.ctx.prefix, label_name(n)));
            }
            K::Label => {
                if !self.ctx.new_targets.contains(&label_name(n)) {
                    return None;
                }
                out.push_str(n.leaf_text());
            }
            K::Ref => {
                let name = ref_name(n);
                if self.ctx.new_targets.contains(&name) {
                    out.push_str(&n.full_text());
                } else {
                    out.push_str(&format!("#emph({});", typst_str(&name)));
                }
            }
            K::Hash if !eval => return None,
            K::Error => return None,
            K::MathIdent if !eval && self.file.removed.contains(n.leaf_text().as_str()) => return None,
            _ if n.children().len() == 0 => out.push_str(&old_leaf(n)),
            _ => {
                for c in n.children() {
                    self.sanitize_into(c, n.kind(), renamed, out)?;
                }
            }
        }
        Some(())
    }

    fn diff_tokens(&mut self, old: &[Tok], new: &[Tok]) {
        let segs = token_segments(old, new);
        // Absorb a kept run between two changes when it is at most one
        // short word, or much shorter than the changes around it: whole
        // phrases replaced read better than alternating fragments.
        let count = |toks: &[Tok]| toks.iter().filter(|t| matches!(t.kind, TK::Word | TK::Atom)).count();
        let size = |s: &Seg| count(&old[s.old.clone()]).max(count(&new[s.new.clone()]));
        let mut merged: Vec<Seg> = vec![];
        let mut i = 0;
        while i < segs.len() {
            let s = segs[i].clone();
            let kept = &new[s.new.clone()];
            let inner = i > 0 && i + 1 < segs.len();
            let short = kept.iter().all(|t| t.kind != TK::Atom)
                && match count(kept) {
                    0 => true,
                    1 => kept.iter().all(|t| t.kind != TK::Word || t.text.chars().count() <= 4),
                    n => inner && 2 * n < size(merged.last().unwrap()).min(size(&segs[i + 1])),
                };
            if s.equal && inner && short {
                merged.last_mut().unwrap().extend(&segs[i + 1]);
                i += 2;
                continue;
            }
            push_seg(&mut merged, s);
            i += 1;
        }
        // A change of punctuation alone is hard to see: it takes the word it
        // is attached to along.
        let word = |t: &Tok| t.kind == TK::Word && t.text.chars().any(char::is_alphanumeric);
        for i in 0..merged.len() {
            let Seg { old: o, new: n, equal } = merged[i].clone();
            if equal
                || old[o]
                    .iter()
                    .chain(&new[n.clone()])
                    .any(|t| word(t) || t.kind == TK::Atom)
            {
                continue;
            }
            if i > 0 && !merged[i - 1].new.is_empty() && word(&new[n.start - 1]) {
                merged[i - 1].old.end -= 1;
                merged[i - 1].new.end -= 1;
                merged[i].old.start -= 1;
                merged[i].new.start -= 1;
            } else if i + 1 < merged.len() && !merged[i + 1].new.is_empty() && word(&new[n.end]) {
                merged[i + 1].old.start += 1;
                merged[i + 1].new.start += 1;
                merged[i].old.end += 1;
                merged[i].new.end += 1;
            }
        }
        cover_shown_strings(&mut merged, new, &self.ctx.shown_strings);
        let outer = self.bare;
        let around = |r: &Range<usize>| (r.start.checked_sub(1).map(|i| &new[i]), new.get(r.end));
        for s in merged {
            if !s.equal {
                let (before, after) = around(&s.new);
                self.bare = Bare::of(&old[s.old.clone()], &new[s.new.clone()], before, after);
                let last = s.new.end == new.len();
                self.change(&old[s.old], &new[s.new], last);
                self.bare = outer;
                continue;
            }
            for (j, (ot, nt)) in s.new.clone().zip(old[s.old].iter().zip(&new[s.new.clone()])) {
                if ot.text == nt.text || nt.kind != TK::Atom {
                    self.out.push_str(&nt.text);
                    continue;
                }
                let ((before, after), (o, n)) = (
                    around(&(j..j + 1)),
                    (std::slice::from_ref(ot), std::slice::from_ref(nt)),
                );
                self.bare = Bare::of(o, n, before, after);
                if !self.diff_code(ot, nt, false) {
                    self.change(o, n, false);
                }
                self.bare = outer;
            }
        }
    }

    /// Write code `n` with the changes inside its content blocks against
    /// `o` marked. Any other change cannot be marked inside the code: the
    /// call would get marks where it expects a value. With `list`, each
    /// changed argument of the outermost call, other than content, is
    /// written as in `n` and listed instead. Otherwise, or if the code
    /// differs outside the arguments of a call, writes nothing and returns
    /// false, so that the caller marks or lists the code whole. Also false
    /// if diffing inside code is off. Without `list`, one changed argument
    /// of the call that the code starts with can be marked as a value, see
    /// [`Self::marked_arg`].
    fn diff_code(&mut self, o: &Tok, n: &Tok, list: bool) -> bool {
        if !self.ctx.opts.content_blocks {
            return false;
        }
        let (at, count) = (self.out.len(), self.changes.len());
        // An argument of the call that the code starts with is evaluated
        // where the code stands, so that it can be bound ahead of the code.
        let top = !list && n.block && n.nodes.get(1).is_some_and(|c| c.kind() == K::FuncCall);
        let outer = std::mem::replace(&mut self.content_code, top);
        let outer_bound = self.hoisted.take();
        let ok = self.diff_code_nodes(&o.nodes, &n.nodes, list, None, "");
        self.content_code = outer;
        let Some(arg) = std::mem::replace(&mut self.hoisted, outer_bound).filter(|_| ok) else {
            return ok;
        };
        let Some(old) = self.sanitize(&o.nodes) else {
            self.out.truncate(at);
            self.changes.truncate(count);
            return false;
        };
        let marked = self.out.split_off(at);
        let toks = (std::slice::from_ref(o), std::slice::from_ref(n));
        let code = self.replaced(&old, &n.text, toks, Some((&arg, &marked)), true);
        self.out.push_str(&format!("#{code};"));
        true
    }

    /// Labels, comments and spaces are written as in `n` and compared only
    /// for the summary: they do not show. `call` is the code before the
    /// nodes if they are the arguments of a call: what is called, or a set
    /// rule up to its arguments, from `start` on.
    fn diff_code_nodes(
        &mut self,
        o: &[SyntaxNode],
        n: &[SyntaxNode],
        list: bool,
        call: Option<&str>,
        start: &str,
    ) -> bool {
        let (at, count) = (self.out.len(), self.changes.len());
        let trivia = |x: &&SyntaxNode| matches!(x.kind(), K::Label | K::LineComment | K::BlockComment | K::Space);
        let mut rest = o.iter().filter(|x| !trivia(x));
        let mut ok = true;
        let mut before = start.to_string();
        // The body of a call that also takes content by name, such as a
        // figure's next to its caption: its only positional argument.
        let positional = |x: &SyntaxNode| x.cast::<ast::Expr>().is_some();
        let body = n.iter().filter(|x| positional(x)).count() == 1
            && n.iter().any(|x| {
                x.cast::<ast::Named>()
                    .is_some_and(|a| a.expr().to_untyped().kind() == K::ContentBlock)
            });
        for x in n {
            if trivia(&x) {
                self.out.push_str(&x.full_text());
            } else {
                ok = ok
                    && rest.next().is_some_and(|y| {
                        // Only the body of the first call is marked apart.
                        let top = call.is_some() && std::mem::take(&mut self.content_code);
                        let same = self.diff_code_node(y, x, list && call.is_none(), &before);
                        self.content_code |= top;
                        same || call.is_some_and(|c| {
                            if list {
                                self.listed_arg(c, y, x)
                            } else {
                                top && body && positional(x) && self.marked_arg(y, x)
                            }
                        })
                    });
            }
            before.push_str(&x.full_text());
        }
        let ok = ok && rest.next().is_none();
        let labels = |v: &[SyntaxNode]| {
            v.iter()
                .filter(|x| x.kind() == K::Label)
                .map(|x| x.full_text().to_string())
                .collect::<String>()
        };
        if ok && labels(o) != labels(n) {
            self.list(&labels(o), &labels(n));
        }
        if !ok {
            self.out.truncate(at);
            self.changes.truncate(count);
        }
        ok
    }

    /// `before` is the code before `n`: the code called if `n` is its
    /// arguments.
    fn diff_code_node(&mut self, o: &SyntaxNode, n: &SyntaxNode, list: bool, before: &str) -> bool {
        let block = |m: &SyntaxNode| m.cast::<ast::Equation>().map(|e| e.block());
        if o.kind() != n.kind() || block(o) != block(n) {
            return false;
        }
        let children = |m: &SyntaxNode| m.children().cloned().collect::<Vec<_>>();
        if n.kind() == K::Markup {
            let (om, nm) = (children(o), children(n));
            let top = std::mem::replace(&mut self.top, false);
            // Markup without blocks stays inline: no paragraph breaks around
            // its changes.
            let blocks = |m: &[SyntaxNode]| {
                m.iter()
                    .any(|c| c.kind() == K::Parbreak || c.kind() == K::Heading || is_item(c.kind()))
            };
            if blocks(&om) || blocks(&nm) {
                self.docs(&Doc::new(&om), &Doc::new(&nm), false);
            } else {
                self.diff_tokens(&tokenize(&om), &tokenize(&nm));
            }
            self.top = top;
            return true;
        }
        if n.children().len() == 0 {
            let same = o.leaf_text() == n.leaf_text();
            if same {
                self.out.push_str(n.leaf_text());
            }
            return same;
        }
        if n.kind() == K::Math && self.ctx.opts.fine_math {
            return self.diff_math(o, n);
        }
        let call = (n.kind() == K::Args).then_some(before);
        // A set rule that a show rule applies is named with it.
        let start = if n.kind() == K::SetRule && before.starts_with("show") {
            before
        } else {
            ""
        };
        self.diff_code_nodes(&children(o), &children(n), list && n.kind() != K::Closure, call, start)
    }

    /// Write formula `n` with the changed parts against `o` marked inside
    /// it, see the preamble's `redline-math`. False, with nothing written, if
    /// most of the formula changed, which reads better as the formula
    /// replaced whole, or a change cannot be marked inside it.
    fn diff_math(&mut self, o: &SyntaxNode, n: &SyntaxNode) -> bool {
        let (at, count) = (self.out.len(), self.changes.len());
        let outer = std::mem::take(&mut self.bare);
        let mut kept = 0;
        let ok = self.math_nodes(o, n, &mut kept) && 2 * kept >= math_size(o).max(math_size(n));
        self.bare = outer;
        if !ok {
            self.out.truncate(at);
            self.changes.truncate(count);
        }
        ok
    }

    /// Write node `n` of a formula with the changes against `o` inside it
    /// marked; false if they cannot be. `kept` counts the unchanged leaves.
    fn math_nodes(&mut self, o: &SyntaxNode, n: &SyntaxNode, kept: &mut usize) -> bool {
        if o.kind() != n.kind() || n.children().len() == 0 {
            return false;
        }
        if n.kind() == K::Math {
            let children = |m: &SyntaxNode| m.children().cloned().collect::<Vec<_>>();
            return self.math_seq(&children(o), &children(n), kept);
        }
        let mut rest = o.children().filter(|x| !math_trivia(x));
        let mut before = None;
        for x in n.children() {
            if math_trivia(x) {
                self.out.push_str(&x.full_text());
                continue;
            }
            let Some(y) = rest.next() else { return false };
            // A part that a mark can replace: the operand of an attachment or
            // a fraction, whose parentheses do not show, or a positional
            // argument of a call.
            let operand = match n.kind() {
                K::MathAttach => before.is_some_and(|b| matches!(b, K::Hat | K::Underscore)),
                K::MathFrac => x.kind() != K::Slash,
                K::MathArgs => !matches!(
                    x.kind(),
                    K::LeftParen | K::RightParen | K::Comma | K::Semicolon | K::Named | K::Spread
                ),
                _ => false,
            };
            before = Some(x.kind());
            let same = collapse_ws(y.full_text().as_str()) == collapse_ws(x.full_text().as_str());
            if same {
                self.out.push_str(&x.full_text());
                *kept += math_size(x);
            } else if !self.math_try(y, x, kept)
                && !(operand && self.math_change(&operand_body(y), &operand_body(x), n.kind() != K::MathArgs))
            {
                return false;
            }
        }
        rest.next().is_none()
    }

    /// [`Self::math_nodes`], with nothing written if it fails.
    fn math_try(&mut self, o: &SyntaxNode, n: &SyntaxNode, kept: &mut usize) -> bool {
        let (at, count, k) = (self.out.len(), self.changes.len(), *kept);
        let ok = self.math_nodes(o, n, kept);
        if !ok {
            self.out.truncate(at);
            self.changes.truncate(count);
            *kept = k;
        }
        ok
    }

    /// Write the nodes `n` of a sequence in a formula with the changed runs
    /// against `o` marked.
    fn math_seq(&mut self, o: &[SyntaxNode], n: &[SyntaxNode], kept: &mut usize) -> bool {
        let key = |x: &SyntaxNode| match x.kind() {
            K::Space => " ".to_string(),
            _ => collapse_ws(x.full_text().as_str()),
        };
        let keys = |v: &[SyntaxNode]| v.iter().map(key).collect::<Vec<_>>();
        let trim = |v: &[SyntaxNode]| {
            let a = v.iter().position(|x| !math_trivia(x)).unwrap_or(v.len());
            let b = v.iter().rposition(|x| !math_trivia(x)).map_or(a, |b| b + 1);
            (a, b)
        };
        for s in segments(&keys(o), &keys(n)) {
            let new = &n[s.new.clone()];
            if s.equal {
                for x in new {
                    self.out.push_str(&x.full_text());
                    *kept += math_size(x);
                }
                continue;
            }
            let old = &o[s.old.clone()];
            let ((oa, ob), (na, nb)) = (trim(old), trim(new));
            let (oc, nc) = (&old[oa..ob], &new[na..nb]);
            for x in &new[..na] {
                self.out.push_str(&x.full_text());
            }
            let inside = oc.len() == 1 && nc.len() == 1 && self.math_try(&oc[0], &nc[0], kept);
            if !inside && !self.math_change(oc, nc, false) {
                return false;
            }
            for x in &new[nb..] {
                self.out.push_str(&x.full_text());
            }
        }
        true
    }

    /// Mark the nodes `o` of a formula deleted and `n` inserted, in
    /// parentheses if `group`; false if a line break or alignment point is
    /// deleted, or inserted alone, as no mark shows it.
    fn math_change(&mut self, o: &[SyntaxNode], n: &[SyntaxNode], group: bool) -> bool {
        let layout = |x: &SyntaxNode| matches!(x.kind(), K::Linebreak | K::MathAlignPoint);
        if o.iter().any(layout) || n.iter().all(|x| layout(x) || math_trivia(x)) && !n.is_empty() {
            return false;
        }
        let tok = |v: &[SyntaxNode]| Tok {
            kind: TK::Atom,
            key: String::new(),
            text: text(v),
            nodes: v.to_vec(),
            block: false,
            at: (v[0].span(), 0),
        };
        let start = self.changes.len();
        if group {
            self.out.push('(');
        }
        if !o.is_empty() {
            let Some(src) = self.sanitize(o) else { return false };
            let args = if n.is_empty() {
                "math: true"
            } else {
                "math: true, gap: true"
            };
            self.mark(ChangeKind::Del, args, math_body(&src), &[tok(o)]);
        }
        if !n.is_empty() {
            let t = tok(n);
            self.mark(ChangeKind::Ins, "math: true", math_body(&t.text), &[t]);
        }
        if group {
            self.out.push(')');
        }
        self.join_from(start);
        true
    }

    /// Write the changed body `n` of a call as it is and keep the old body
    /// `o`, for [`Self::diff_code`] to show its value deleted before the
    /// call, which it marks inserted whole: so that the content the call
    /// takes by name, such as a figure's caption, is shown once, and the call
    /// is the new version's.
    fn marked_arg(&mut self, o: &SyntaxNode, n: &SyntaxNode) -> bool {
        if self.hoisted.is_some() || !self.evaluates_deleted() {
            return false;
        }
        let Some(old) = self.sanitize(std::slice::from_ref(o)) else {
            return false;
        };
        self.hoisted = Some(old);
        self.out.push_str(&n.full_text());
        true
    }

    /// Write the changed argument `n` of a call to `call` and list it with
    /// the old argument `o`.
    fn listed_arg(&mut self, call: &str, o: &SyntaxNode, n: &SyntaxNode) -> bool {
        self.out.push_str(&n.full_text());
        let what = self.changed_args(call, std::slice::from_ref(o), std::slice::from_ref(n));
        self.list_what(
            &format!("{call}({})", o.full_text()),
            &format!("{call}({})", n.full_text()),
            what,
        );
        true
    }

    /// The arguments of a call to `call` that differ between the old
    /// arguments `o` and the new `n`: named ones by name, others by position
    /// with what is called as their name. A value too long to read in a line
    /// is elided.
    fn changed_args(&self, call: &str, o: &[SyntaxNode], n: &[SyntaxNode]) -> Vec<What> {
        const SHOWN: usize = 60;
        let value = |v: &str| match v.contains('\n') || v.chars().count() > SHOWN {
            true => "…".to_string(),
            false => v.to_string(),
        };
        let named = |args: &[SyntaxNode], unpin: bool| -> Vec<(String, String)> {
            let mut pos = 0;
            args.iter()
                .map(|a| {
                    let (name, v) = match a.cast::<ast::Named>() {
                        Some(named) => (
                            named.name().as_str().to_string(),
                            named.expr().to_untyped().full_text().to_string(),
                        ),
                        None => {
                            pos += 1;
                            (format!("{}\0{pos}", collapse_ws(call)), a.full_text().to_string())
                        }
                    };
                    (name, if unpin { self.ctx.unpin(&v).0 } else { v })
                })
                .collect()
        };
        let (old, new) = (named(o, true), named(n, false));
        let find = |args: &[(String, String)], name: &str| args.iter().find(|(m, _)| m == name).map(|(_, v)| v.clone());
        let mut what: Vec<What> = vec![];
        for name in new.iter().chain(&old).map(|(m, _)| m) {
            let (o, n) = (find(&old, name), find(&new, name));
            if o == n {
                continue;
            }
            let w = What::Arg {
                name: name.split('\0').next().unwrap().to_string(),
                old: o.as_deref().map(value),
                new: n.as_deref().map(value),
            };
            if !what.contains(&w) {
                what.push(w);
            }
        }
        what
    }

    /// Write new code with the markup of its content blocks marked
    /// inserted; false if it has no content blocks.
    fn inserted_code(&mut self, nodes: &[SyntaxNode]) -> bool {
        let mut found = false;
        for n in nodes {
            if n.kind() == K::ContentBlock {
                let toks = tokenize(&markup(n));
                let (lead, core, trail) = trim_spaces(&toks);
                self.out.push('[');
                self.out.push_str(&lead);
                if !core.is_empty() {
                    self.ins("block: true", core);
                    found = true;
                }
                self.out.push_str(&trail);
                self.out.push(']');
            } else if n.children().len() == 0 {
                self.out.push_str(n.leaf_text());
            } else {
                found |= self.inserted_code(&n.children().cloned().collect::<Vec<_>>());
            }
        }
        found
    }

    /// A changed run: old struck, new highlighted. The new version's
    /// whitespace stays outside the marks: before the deletion if the old
    /// text had a space there too, else between deletion and insertion, so
    /// that deleted punctuation stays next to its word. A space the new
    /// version lacks belongs to the deletion. `last` if the run ends the
    /// text.
    fn change(&mut self, old: &[Tok], new: &[Tok], last: bool) {
        let (olead, ocore, otrail) = trim_spaces(old);
        let (nlead, ncore, ntrail) = trim_spaces(new);
        if let ([o], [n]) = (ocore, ncore)
            && o.kind == TK::Atom
            && n.kind == TK::Atom
        {
            let at = self.out.len();
            self.out.push_str(&nlead);
            let toks = (std::slice::from_ref(o), std::slice::from_ref(n));
            if self.diff_code(o, n, false) {
                self.out.push_str(&ntrail);
                return;
            }
            // Code can show the same after a change; other atoms cannot.
            if self.evaluates_deleted()
                && o.nodes[0].kind() == K::Hash
                && n.nodes[0].kind() == K::Hash
                && let Some(old) = self.sanitize(&o.nodes)
            {
                let code = self.replaced(&old, &n.text, toks, None, n.block);
                self.out.push_str(&format!("#{code};{ntrail}"));
                return;
            }
            self.out.truncate(at);
        }
        let start = self.changes.len();
        let before = !olead.is_empty() || ocore.is_empty();
        if before {
            self.out.push_str(&nlead);
        }
        if !ocore.is_empty() {
            let pad = |o: &str, n: &str| if o.is_empty() || !n.is_empty() { "" } else { " " };
            let body = self.content(ocore, ChangeKind::Del);
            let trail = pad(&otrail, &ntrail);
            let body = format!("{}{body}{trail}", pad(&olead, &nlead));
            // A deletion right before an insertion is set apart from it,
            // unless the insertion starts with closing punctuation.
            let joined = trail.is_empty() && (before || nlead.is_empty());
            let closing = ncore
                .first()
                .is_some_and(|t| t.kind == TK::Word && t.text.starts_with(CLOSING));
            let mut args = block_arg(ocore).to_string();
            if joined && !ncore.is_empty() && !closing && args.is_empty() {
                args.push_str("gap: true");
            }
            self.mark(ChangeKind::Del, &args, body, ocore);
        }
        if !before {
            self.out.push_str(&nlead);
        }
        // Paragraphs wholly inserted inside the run, or at its end if it
        // ends the text, are marked as blocks.
        let paras: Vec<&[Tok]> = ncore.split(|t| t.key == "\n\n").collect();
        let breaks = ncore.iter().filter(|t| t.key == "\n\n");
        for (i, (para, gap)) in paras.iter().zip(breaks.map(Some).chain([None])).enumerate() {
            let (lead, core, trail) = trim_spaces(para);
            self.out.push_str(&lead);
            if !core.is_empty() {
                let args = if i > 0 && (i + 1 < paras.len() || last) {
                    "block: true"
                } else {
                    block_arg(core)
                };
                self.ins(args, core);
            }
            self.out.push_str(&trail);
            self.out.push_str(gap.map_or("", |t| &t.text));
        }
        self.out.push_str(&ntrail);
        self.join_from(start);
    }
}

/// Spaces and comments in a formula, which do not show.
fn math_trivia(n: &SyntaxNode) -> bool {
    matches!(n.kind(), K::Space | K::LineComment | K::BlockComment)
}

/// An operand of an attachment or fraction without the parentheses that
/// do not show around it.
fn operand_body(n: &SyntaxNode) -> Vec<SyntaxNode> {
    let parts: Vec<&SyntaxNode> = n.children().collect();
    match parts[..] {
        [l, m, r] if l.leaf_text() == "(" && r.leaf_text() == ")" && m.kind() == K::Math => {
            m.children().cloned().collect()
        }
        _ => vec![n.clone()],
    }
}

/// Math source as an inline equation; a line break at its end would
/// escape the closing dollar.
fn math_body(src: &str) -> String {
    let pad = if src.ends_with('\\') { " " } else { "" };
    format!("${src}{pad}$")
}

/// The number of leaves of a formula that show.
fn math_size(n: &SyntaxNode) -> usize {
    match n.children().len() {
        0 => usize::from(!math_trivia(n)),
        _ => n.children().map(math_size).sum(),
    }
}

/// The most voted step other than 0, the later on a tie.
fn majority(votes: impl Iterator<Item = (u8, usize)>) -> Option<u8> {
    let mut count = [0usize; 256];
    for (s, w) in votes {
        count[s as usize] += w;
    }
    (1..256)
        .max_by_key(|&s| count[s])
        .filter(|&s| count[s] > 0)
        .map(|s| s as u8)
}

/// A changed run that is only display equations or code on their own
/// lines is marked as a block.
fn block_arg(run: &[Tok]) -> &'static str {
    let block = run.iter().all(|t| t.block || matches!(t.kind, TK::Space | TK::Comment));
    if block { "block: true" } else { "" }
}

/// Readable text standing for deleted code or math that cannot be
/// evaluated: what it calls or names, in brackets.
fn placeholder(nodes: &[SyntaxNode]) -> String {
    let mut name = None;
    for n in nodes {
        walk(n, &mut |c| {
            if name.is_none() && matches!(c.kind(), K::Ident | K::MathIdent) {
                name = Some(c.leaf_text().to_string());
            }
        });
    }
    let math = nodes.iter().any(|n| n.kind() == K::Equation);
    let name = if math {
        "formula".into()
    } else {
        name.unwrap_or("code".into())
    };
    format!("#emph({});", typst_str(&format!("[{name}]")))
}

/// The first call or set rule in `nodes`, outermost first: the code before
/// its arguments, with whitespace collapsed, and the arguments.
fn outer_call(nodes: &[SyntaxNode]) -> Option<(String, Vec<SyntaxNode>)> {
    for n in nodes {
        if matches!(n.kind(), K::FuncCall | K::SetRule)
            && let Some(args) = n.children().find(|c| c.kind() == K::Args)
        {
            let callee: Vec<SyntaxNode> = n.children().take_while(|c| c.kind() != K::Args).cloned().collect();
            let items = args.children().filter(|a| {
                !matches!(
                    a.kind(),
                    K::LeftParen | K::RightParen | K::Comma | K::Space | K::LineComment | K::BlockComment
                )
            });
            return Some((collapse_ws(&text(&callee)), items.cloned().collect()));
        }
        // Not into a function's body or a block, where it is one of many.
        if matches!(n.kind(), K::Closure | K::CodeBlock | K::ContentBlock) {
            continue;
        }
        if let Some(call) = outer_call(&n.children().cloned().collect::<Vec<_>>()) {
            return Some(call);
        }
    }
    None
}

/// The markup of the caption of a `#figure` call.
fn caption(nodes: &[SyntaxNode]) -> Option<Vec<SyntaxNode>> {
    let call = nodes.get(1).filter(|n| n.kind() == K::FuncCall)?;
    if call.children().next()?.leaf_text() != "figure" {
        return None;
    }
    let args = call.children().find(|c| c.kind() == K::Args)?;
    let caption = args
        .children()
        .find(|a| a.kind() == K::Named && a.children().next().is_some_and(|n| n.leaf_text() == "caption"))?;
    caption
        .children()
        .last()
        .filter(|v| v.kind() == K::ContentBlock)
        .map(markup)
}

/// The markup of the content blocks in code, outside other content blocks.
fn content_blocks(nodes: &[SyntaxNode]) -> Vec<Vec<SyntaxNode>> {
    fn into(n: &SyntaxNode, out: &mut Vec<Vec<SyntaxNode>>) {
        match n.kind() {
            K::ContentBlock => out.push(markup(n)),
            _ => n.children().for_each(|c| into(c, out)),
        }
    }
    let mut out = vec![];
    nodes.iter().for_each(|n| into(n, &mut out));
    out
}

/// The source of an old file to be evaluated under [`OLD_DIR`]: paths
/// relative to the root lead there.
fn old_copy(src: &str) -> String {
    fn into(n: &SyntaxNode, out: &mut String) {
        match n.children().len() {
            0 => out.push_str(&old_leaf(n)),
            _ => n.children().for_each(|c| into(c, out)),
        }
    }
    let mut out = String::new();
    into(&parse(src), &mut out);
    out
}

/// A leaf of old source, with a string that is a path from the root led to
/// [`OLD_DIR`].
fn old_leaf(n: &SyntaxNode) -> String {
    let t = n.leaf_text();
    match t.strip_prefix("\"/") {
        Some(rest) if n.kind() == K::Str && rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') => {
            format!("\"/{OLD_DIR}/{rest}")
        }
        _ => t.to_string(),
    }
}

/// The lines of `old` and of `new` between those both start and end with,
/// without the indentation they all share.
fn differing_lines(old: &str, new: &str) -> (String, String) {
    let (o, n): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let pre = o.iter().zip(&n).take_while(|(a, b)| a == b).count();
    let room = o.len().min(n.len()) - pre;
    let suf = o
        .iter()
        .rev()
        .zip(n.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    let (o, n) = (&o[pre..o.len() - suf], &n[pre..n.len() - suf]);
    if o.is_empty() && n.is_empty() {
        return (old.into(), new.into());
    }
    let cut = o
        .iter()
        .chain(n)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let show = |ls: &[&str]| {
        ls.iter()
            .map(|l| l.get(cut..).unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    };
    (show(o), show(n))
}

/// A Typst string literal holding `s`.
pub(crate) fn typst_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
