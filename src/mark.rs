//! Writing the new version with the changes against the old one marked, as
//! calls to the preamble's functions, and the summary.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::ops::Range;

use similar::{Algorithm, DiffOp, capture_diff_slices};
use typst_syntax::{RootedPath, Source, SyntaxKind as K, SyntaxNode, VirtualPath, VirtualRoot, ast, parse};

use crate::align::{Step, align};
use crate::blame::{Steps, blame};
use crate::moves::{self, Run};
use crate::syntax::*;
use crate::{Bib, Change, ChangeKind, History, Move, Moves, OLD_DIR, Options, PREAMBLE_PATH, pair_files, resolve};

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
    /// Names defined somewhere in the old version but nowhere in the new.
    removed_defs: HashSet<String>,
    /// The project files, the entry first.
    old_files: Vec<(String, String)>,
    new_files: Vec<(String, String)>,
    /// New file path to the path of the old file it is compared with.
    pairs: HashMap<String, String>,
    /// The project files of the versions between the old and the new one,
    /// if each change is marked with the step that made it.
    steps: Option<Vec<Vec<(String, String)>>>,
    /// Each new file that is to be written, by path, with its old file, in
    /// reading order.
    aligned: Vec<(String, Aligned)>,
    /// Blocks moved, by index into `aligned`.
    moves: Vec<Run>,
    /// The move each moved old and new block belongs to, by (index into
    /// `aligned`, block).
    moved_from: HashMap<(usize, usize), usize>,
    moved_to: HashMap<(usize, usize), usize>,
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
}

impl Ctx {
    pub fn new(
        old_files: Vec<(String, String)>,
        new_files: Vec<(String, String)>,
        steps: Option<Vec<Vec<(String, String)>>>,
        swapped: bool,
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
            removed_defs: union(&old_files, definitions)
                .difference(&union(&new_files, definitions))
                .cloned()
                .collect(),
            pairs,
            old_files,
            new_files,
            steps,
            aligned,
            moves,
            moved_from,
            moved_to,
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
            removed,
            index,
        };
        let outer = std::mem::replace(&mut self.file, file);
        let out = std::mem::take(&mut self.out);
        self.write_steps(&a.docs.0, &a.docs.1, &a.steps, true);
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
        });
    }

    /// Record changed code that is not marked, `old` or `new` empty if it
    /// was inserted or deleted.
    fn list(&mut self, old: &str, new: &str) {
        let (old, new) = differing_lines(old, new);
        self.record(ChangeKind::Unmarked, new);
        self.changes.last_mut().unwrap().old = old;
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
        let step = step.map_or(String::new(), |s| format!("step: {s}, "));
        let args = match args {
            "" => format!("{step}id: {}", self.changes.len()),
            _ => format!("{args}, {step}id: {}", self.changes.len()),
        };
        let p = &self.ctx.prefix;
        let call = format!("{p}-{}({args})[{body}]", kind.name());
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

    /// Mark inserted tokens; with steps, each run of tokens that one step
    /// wrote separately.
    fn ins(&mut self, args: &str, toks: &[Tok]) {
        let (Some(last), steps) = (self.ctx.last_step(), self.tok_steps(toks)) else {
            return self.mark(ChangeKind::Ins, args, self.content(toks, ChangeKind::Ins), toks);
        };
        let runs: Vec<&[u8]> = steps.chunk_by(|a, b| a == b).collect();
        // Runs of several steps are parts of one block, each laid out inline.
        let args = if runs.len() > 1 { "" } else { args };
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
    /// listed.
    fn unmarked(&mut self, old: &str, b: &Block) {
        self.out.push_str(&b.text());
        if !b.is_import() {
            self.list(old, &b.text());
        }
    }

    /// The entry's output source: the preamble import and the marked
    /// document with the summary at the place [`Self::summary_here`] chose.
    fn finish(&self, old_label: &str, new_label: &str, bib: &Bib, history: &History) -> String {
        let at = self.summary_at.unwrap_or(self.entry_out.len());
        let mut out = self.entry_out.clone();
        let newline = if out[..at].ends_with('\n') || at == 0 { "" } else { "\n" };
        let start = format!(
            "{newline}#show: {}-bars\n{}",
            self.ctx.prefix,
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
        let first = front(self.ctx.entry(), "[Before the first section]".into());
        let others: Vec<Section> = self
            .files
            .iter()
            .map(|(p, _)| front(p, format!("[Definitions in #raw({}), marked where used]", typst_str(p))))
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
            // A move is counted apart from edits, once in each section it
            // left or reached; a moved section's own move is its label.
            let n = count(&|c| here(c) && c.kind != ChangeKind::Moved);
            let moves: HashSet<usize> = self
                .changes
                .iter()
                .filter(|c| here(c) && c.kind == ChangeKind::Moved)
                .filter_map(|c| c.text.parse().ok())
                .map(part_of)
                .collect();
            if n == 0 && moves.is_empty() && !s.deleted {
                continue;
            }
            let moves = moves.len() - usize::from(s.moved.is_some_and(|r| moves.contains(&r)));
            let code: String = self
                .changes
                .iter()
                .filter(|c| here(c) && c.kind == ChangeKind::Unmarked)
                .map(|c| format!("(old: {}, new: {}),", typst_str(&c.old), typst_str(&c.text)))
                .collect();
            // A move is checked once, at its new place.
            let marks: String = self
                .changes
                .iter()
                .enumerate()
                .filter(|(_, c)| here(c))
                .filter(|(_, c)| {
                    matches!(c.kind, ChangeKind::Ins | ChangeKind::Del)
                        || c.kind == ChangeKind::Moved && !c.old.is_empty()
                })
                .map(|(id, c)| {
                    let m = c.unmarked();
                    let mv = match c.kind {
                        ChangeKind::Moved => format!("move: {}, ", c.text),
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
            let before = if std::ptr::eq(s, &first) { "before: true, " } else { "" };
            entries.push_str(&format!(
                "  (file: {}, level: {}, target: {target}, title: {}, count: {n}, {moved}{before}deleted: {}, new: {}, code: ({code}), marks: ({marks})),\n",
                typst_str(&s.file),
                s.level,
                s.title,
                s.deleted,
                s.new,
            ));
        }
        let p = &self.ctx.prefix;
        let history = history
            .to_typst(old_label, new_label)
            .map_or(String::new(), |h| format!("  history: {h},\n"));
        format!(
            "#{p}-summary((\n  old: {}, new: {}, ins: {}, del: {},{}\n  entries: (\n{entries}  ),\n  bib: {},\n{history}))\n",
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

    /// The summary goes at the top level of the entry, before its first
    /// show rule and its first content, so that the document's template
    /// and show rules do not apply to it.
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

    fn docs(&mut self, old: &Doc, new: &Doc) {
        self.write_steps(old, new, &align(old, new), false);
    }

    /// Write `new` by `steps`; at the top level of a file (`top`), blocks
    /// that moved are written as moves.
    fn write_steps(&mut self, old: &Doc, new: &Doc, steps: &[Step], top: bool) {
        let ctx = self.ctx;
        let file = self.file.index;
        let moved = |map: &'a HashMap<(usize, usize), usize>, b: usize| map.get(&(file, b)).filter(|_| top);
        // Only a deleted section's own blocks count with it.
        let mut outer = None;
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
            match s {
                Step::Del(o) => {
                    // A list that ends here in the new version ends before
                    // a deleted block that is not an item.
                    let next = steps[k + 1..].iter().find_map(|s| match *s {
                        Step::Keep(n) | Step::Pair(_, n) | Step::Ins(n) => Some(&new.blocks[n]),
                        Step::Del(_) => None,
                    });
                    let list_ends = self.after_item && next.is_none_or(|b| b.kind != Kind::Item);
                    match moved(&ctx.moved_from, o) {
                        Some(&r) if ctx.moves[r].old.1.start == o => self.moved_from(r, list_ends),
                        Some(_) => {}
                        None => self.deleted(&old.blocks[o], list_ends),
                    }
                }
                Step::Ins(n) => match moved(&ctx.moved_to, n) {
                    Some(&r) if ctx.moves[r].new.1.start == n => self.moved_to(r),
                    Some(_) => {}
                    None => self.write_new(old, new, s, ("", "")),
                },
                _ => self.write_new(old, new, s, ("", "")),
            }
        }
        if let Some(c) = outer {
            self.current = c;
        }
        self.out.push_str(&new.trailing);
    }

    /// A step with `marks` at the start and the end of the new block's
    /// body; a deletion inside a passage that goes on after it.
    fn write_new(&mut self, old: &Doc, new: &Doc, s: Step, marks: (&str, &str)) {
        match s {
            Step::Keep(n) => self.kept(&new.blocks[n], marks),
            Step::Pair(o, n) => self.pair(&old.blocks[o], &new.blocks[n], marks),
            Step::Ins(n) => self.inserted(&new.blocks[n], marks),
            Step::Del(o) => self.deleted(&old.blocks[o], false),
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
        let count = |k: Kind| blocks.iter().filter(|b| b.kind == k).count();
        let args = format!(
            "heading: {}, down: {}, more: {more}, paragraphs: {}, items: {}",
            ob.kind == Kind::Heading,
            !self.arrived.contains_key(&r),
            count(Kind::Para),
            count(Kind::Item)
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
        let start = self.move_call("ins", r, &format!("block: {}", block(&new.blocks[0], false)), "");
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
            if !(ot.len() == 1 && nt.len() == 1 && self.diff_code(&ot[0], &nt[0], true)) {
                self.unmarked(&ob.text(), nb);
            }
            self.included(nb);
            return;
        }
        let label = self.section(nb, false);
        let (prefix, nbody, suffix) = nb.parts();
        let (_, obody, _) = ob.parts();
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
        self.docs(&odoc, &ndoc);
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

    fn deleted(&mut self, ob: &Block, list_ends: bool) {
        if ob.kind == Kind::Setup {
            // A deleted include of a file that is gone shows its content,
            // through a root-relative path.
            let path = ob.include_target().and_then(|t| resolve(&self.file.old_path, &t));
            match path {
                Some(p) if !self.ctx.pairs.values().any(|o| *o == p) && source(&self.ctx.old_files, &p).is_some() => {
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
                    );
                }
                _ if !ob.is_import() => self.list(&ob.text(), ""),
                _ => {}
            }
            return;
        }
        let (prefix, nodes, _) = ob.parts();
        let toks = tokenize(&nodes);
        // A deleted item shows its marker: it is written as a continuation
        // of the block before it.
        let marker = match ob.nodes[0].kind() {
            K::ListItem => "#sym.bullet ".into(),
            K::EnumItem => prefix.trim().replace('.', "\\.").replace('+', "#sym.bullet") + " ",
            K::TermItem => prefix.trim().trim_start_matches('/').trim_start().to_string() + " ",
            _ => String::new(),
        };
        let body = marker + &self.content(&toks, ChangeKind::Del);
        let mut args = "block: true".to_string();
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
    /// removed definitions), or it uses deleted labels.
    fn sanitize(&self, nodes: &[SyntaxNode]) -> Option<String> {
        let mut out = String::new();
        for n in nodes {
            self.sanitize_into(n, K::Markup, &mut out)?;
        }
        Some(out)
    }

    fn sanitize_into(&self, n: &SyntaxNode, parent: K, out: &mut String) -> Option<()> {
        let eval = self.ctx.opts.eval_deleted;
        match n.kind() {
            K::Label if parent == K::Markup => {}
            // A label passed as a named argument or written in math is one
            // that code attaches, possibly at layout where the preamble cannot
            // remove it; under another name it does not define the label a
            // second time. A positional label is taken for a reference.
            K::Label if matches!(parent, K::Named | K::Math) => {
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
                    self.sanitize_into(c, n.kind(), out)?;
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
        for s in merged {
            if !s.equal {
                self.change(&old[s.old], &new[s.new]);
                continue;
            }
            for (ot, nt) in old[s.old].iter().zip(&new[s.new]) {
                if ot.text == nt.text || nt.kind != TK::Atom {
                    self.out.push_str(&nt.text);
                } else if !self.diff_code(ot, nt, false) {
                    self.change(std::slice::from_ref(ot), std::slice::from_ref(nt));
                }
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
    /// if diffing inside code is off.
    fn diff_code(&mut self, o: &Tok, n: &Tok, list: bool) -> bool {
        self.ctx.opts.content_blocks && self.diff_code_nodes(&o.nodes, &n.nodes, list, None)
    }

    /// Labels, comments and spaces are written as in `n` and compared only
    /// for the summary: they do not show. `call` is the code before the
    /// nodes if they are the arguments of a call: what is called, or a set
    /// rule up to its arguments.
    fn diff_code_nodes(&mut self, o: &[SyntaxNode], n: &[SyntaxNode], list: bool, call: Option<&str>) -> bool {
        let (at, count) = (self.out.len(), self.changes.len());
        let trivia = |x: &&SyntaxNode| matches!(x.kind(), K::Label | K::LineComment | K::BlockComment | K::Space);
        let mut rest = o.iter().filter(|x| !trivia(x));
        let mut ok = true;
        let mut before = String::new();
        for x in n {
            if trivia(&x) {
                self.out.push_str(&x.full_text());
            } else {
                ok = ok
                    && rest.next().is_some_and(|y| {
                        self.diff_code_node(y, x, list && call.is_none(), &before)
                            || list && call.is_some_and(|c| self.listed_arg(c, y, x))
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
                self.docs(&Doc::new(&om), &Doc::new(&nm));
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
        let call = (n.kind() == K::Args).then_some(before);
        self.diff_code_nodes(&children(o), &children(n), list && n.kind() != K::Closure, call)
    }

    /// Write the changed argument `n` of a call to `call` and list it with
    /// the old argument `o`.
    fn listed_arg(&mut self, call: &str, o: &SyntaxNode, n: &SyntaxNode) -> bool {
        self.out.push_str(&n.full_text());
        self.list(
            &format!("{call}({})", o.full_text()),
            &format!("{call}({})", n.full_text()),
        );
        true
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
    /// version lacks belongs to the deletion.
    fn change(&mut self, old: &[Tok], new: &[Tok]) {
        let (olead, ocore, otrail) = trim_spaces(old);
        let (nlead, ncore, ntrail) = trim_spaces(new);
        if let ([o], [n]) = (ocore, ncore)
            && o.kind == TK::Atom
            && n.kind == TK::Atom
        {
            let at = self.out.len();
            self.out.push_str(&nlead);
            if self.diff_code(o, n, false) {
                self.out.push_str(&ntrail);
                return;
            }
            self.out.truncate(at);
        }
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
        // Paragraphs wholly inserted inside the run are marked as blocks.
        let paras: Vec<&[Tok]> = ncore.split(|t| t.key == "\n\n").collect();
        let breaks = ncore.iter().filter(|t| t.key == "\n\n");
        for (i, (para, gap)) in paras.iter().zip(breaks.map(Some).chain([None])).enumerate() {
            let (lead, core, trail) = trim_spaces(para);
            self.out.push_str(&lead);
            if !core.is_empty() {
                let args = if i > 0 && i + 1 < paras.len() {
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
