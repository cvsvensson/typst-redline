//! Writing the new version with the changes against the old one marked, as
//! calls to the preamble's functions, and the summary.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use similar::{Algorithm, DiffOp, capture_diff_slices};
use typst_syntax::{SyntaxKind as K, SyntaxNode, parse};

use crate::syntax::*;
use crate::{Change, ChangeKind, OLD_DIR, Options, PREAMBLE_PATH, pair_files, resolve};

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
}

impl Ctx {
    pub fn new(old_files: Vec<(String, String)>, new_files: Vec<(String, String)>, opts: Options) -> Self {
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
        Ctx {
            prefix,
            opts,
            new_targets: union(&new_files, targets),
            removed_defs: union(&old_files, definitions)
                .difference(&union(&new_files, definitions))
                .cloned()
                .collect(),
            pairs: pair_files(&old_files, &new_files),
            old_files,
            new_files,
        }
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

/// The marked files and the changes: every file the entry includes or
/// imports that differs from its old file, the entry with the summary, and
/// the old version's files under [`OLD_DIR`] if deleted content uses them.
pub fn write(ctx: &Ctx, old_label: &str, new_label: &str) -> (Vec<(String, String)>, Vec<Change>) {
    let mut w = Writer::new(ctx);
    w.file(ctx.entry());
    w.top = false;
    w.current = None;
    for (path, _) in &ctx.new_files {
        w.file(path);
    }
    let mut files = vec![(ctx.entry().to_string(), w.finish(old_label, new_label))];
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
    summary_at: Option<usize>,
    sections: Vec<Section>,
    /// Label of the section being written.
    current: Option<String>,
    changes: Vec<Change>,
    /// Deleted content imports the old version.
    old_scope: bool,
}

/// A run of tokens that is the same in both versions or changed.
#[derive(Clone)]
struct Seg {
    old: Range<usize>,
    new: Range<usize>,
    equal: bool,
}

impl Seg {
    fn extend(&mut self, to: &Seg) {
        self.old.end = to.old.end;
        self.new.end = to.new.end;
    }
}

/// Push `s`, merged into the last segment if both are changes.
fn push_seg(segs: &mut Vec<Seg>, s: Seg) {
    match segs.last_mut() {
        Some(last) if !last.equal && !s.equal => last.extend(&s),
        _ => segs.push(s),
    }
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
        }
    }

    /// Mark the changes in the new file at `path` against its old file,
    /// once; an unchanged file other than the entry is not written.
    fn file(&mut self, path: &str) {
        let ctx = self.ctx;
        let Some(new_src) = source(&ctx.new_files, path) else {
            return;
        };
        if !self.done.insert(path.to_string()) {
            return;
        }
        let old_path = ctx.pairs.get(path).cloned().unwrap_or_default();
        let old_src = source(&ctx.old_files, &old_path).unwrap_or_default();
        let entry = path == ctx.entry();
        if !entry && old_src == new_src {
            return;
        }
        let (o, n) = (parse(old_src), parse(new_src));
        let (mut removed, kept) = (definitions(&o), definitions(&n));
        removed.retain(|d| !kept.contains(d));
        removed.extend(ctx.removed_defs.iter().cloned());
        let file = File {
            path: path.into(),
            old_path,
            indent: String::new(),
            removed,
        };
        let outer = std::mem::replace(&mut self.file, file);
        let out = std::mem::take(&mut self.out);
        let children = |r: &SyntaxNode| r.children().cloned().collect::<Vec<_>>();
        self.docs(&Doc::new(&children(&o)), &Doc::new(&children(&n)));
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
        });
    }

    /// Emit `body`, the markup for `toks`, wrapped as a change, and record
    /// it. Deleted code sees the old file's definitions, which read the old
    /// version's files, so that it shows what was deleted.
    fn mark(&mut self, kind: ChangeKind, args: &str, body: String, toks: &[Tok]) {
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

    /// Setup code written as in the new version, its change listed.
    fn unmarked(&mut self, b: &Block) {
        self.out.push_str(&b.text());
        if !b.is_import() {
            self.record(ChangeKind::Unmarked, b.text());
        }
    }

    /// The entry's output source: the preamble import and the marked
    /// document with the summary at the place [`Self::summary_here`] chose.
    fn finish(&self, old_label: &str, new_label: &str) -> String {
        let at = self.summary_at.unwrap_or(self.entry_out.len());
        let mut out = self.entry_out.clone();
        let newline = if out[..at].ends_with('\n') || at == 0 { "" } else { "\n" };
        out.insert_str(at, &format!("{newline}{}", self.summary(old_label, new_label)));
        format!("{}{out}", self.ctx.import())
    }

    fn summary(&self, old_label: &str, new_label: &str) -> String {
        let count = |f: &dyn Fn(&Change) -> bool| self.changes.iter().filter(|c| f(c)).count();
        let front = |file: &str, title: String| Section {
            file: file.into(),
            level: 1,
            target: None,
            title,
            deleted: false,
            new: false,
        };
        // Changes outside sections: before the entry's first heading, and in
        // files that are not read in order (imported, or included by code).
        let first = front(self.ctx.entry(), "[Before the first section]".into());
        let others: Vec<Section> = self
            .files
            .iter()
            .map(|(p, _)| front(p, format!("[Definitions in #raw({}), marked where used]", typst_str(p))))
            .collect();
        let mut entries = String::new();
        for s in std::iter::once(&first).chain(&self.sections).chain(&others) {
            let here = |c: &Change| c.section == s.target && (s.target.is_some() || c.file == s.file);
            let n = if s.deleted { 0 } else { count(&here) };
            // A heading deleted where a heading with the same title remains
            // was changed or moved, not deleted.
            let kept = |t: &Section| !t.deleted && collapse_ws(&t.title) == collapse_ws(&s.title);
            if n == 0 && !s.deleted || s.deleted && self.sections.iter().any(kept) {
                continue;
            }
            let unmarked = !s.deleted && count(&|c| here(c) && c.kind == ChangeKind::Unmarked) > 0;
            let target = s.target.as_ref().map_or("none".to_string(), |l| format!("<{l}>"));
            entries.push_str(&format!(
                "  (file: {}, level: {}, target: {target}, title: {}, count: {n}, deleted: {}, new: {}, unmarked: {unmarked}),\n",
                typst_str(&s.file),
                s.level,
                s.title,
                s.deleted,
                s.new,
            ));
        }
        let p = &self.ctx.prefix;
        format!(
            "#set page(foreground: {p}-bars)\n#{p}-summary((\n  old: {}, new: {}, ins: {}, del: {},\n  entries: (\n{entries}  ),\n))\n",
            typst_str(old_label),
            typst_str(new_label),
            count(&|c| c.kind == ChangeKind::Ins),
            count(&|c| c.kind == ChangeKind::Del),
        )
    }

    /// The summary goes at the top level of the entry, before anything
    /// other than the rules and definitions it starts with.
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
        });
        self.current = Some(target);
        label
    }

    fn docs(&mut self, old: &Doc, new: &Doc) {
        for op in capture_diff_slices(Algorithm::Patience, &old.keys(), &new.keys()) {
            match op {
                DiffOp::Equal { .. } => new.blocks[op.new_range()].iter().for_each(|b| self.kept(b)),
                _ => self.region(&old.blocks[op.old_range()], &new.blocks[op.new_range()]),
            }
        }
        self.out.push_str(&new.trailing);
    }

    /// A run of differing blocks. Similar blocks of the same kind, marker
    /// and label are paired and diffed word by word; around each pair,
    /// deletions come first. Pairing looks a bounded number of blocks ahead,
    /// so that its cost stays linear in the length of the run.
    fn region(&mut self, old: &[Block], new: &[Block]) {
        let owords: Vec<Vec<String>> = old.iter().map(|b| words(&b.toks())).collect();
        let mut p = 0;
        let mut pending: Vec<&Block> = vec![];
        for nb in new {
            let nwords = words(&nb.toks());
            let best = most_similar(
                (p..old.len().min(p + 32))
                    .filter(|&q| old[q].kind == nb.kind && old[q].frame() == nb.frame())
                    .map(|q| (q, similarity(&owords[q], &nwords))),
            );
            match best {
                Some(q) => {
                    self.unpaired(&old[p..q], &pending);
                    pending.clear();
                    self.pair(&old[q], nb);
                    p = q + 1;
                }
                None => pending.push(nb),
            }
        }
        self.unpaired(&old[p..], &pending);
    }

    /// Deleted and inserted blocks in turn, so that a replaced block is
    /// followed by its replacement; but leading setup code of the new
    /// version goes first (deleted content may use its definitions), and
    /// text before the first heading on either side stays before the
    /// headings.
    fn unpaired(&mut self, old: &[Block], new: &[&Block]) {
        let ns = new.iter().position(|b| b.kind != Kind::Setup).unwrap_or(new.len());
        let oh = old.iter().position(|b| b.kind == Kind::Heading).unwrap_or(old.len());
        let nh = new.iter().position(|b| b.kind == Kind::Heading).unwrap_or(new.len());
        new[..ns].iter().for_each(|b| self.inserted(b));
        for (old, new) in [(&old[..oh], &new[ns..nh]), (&old[oh..], &new[nh..])] {
            for k in 0..old.len().max(new.len()) {
                old.get(k).inspect(|b| self.deleted(b));
                new.get(k).inspect(|b| self.inserted(b));
            }
        }
    }

    fn kept(&mut self, b: &Block) {
        self.begin_new(b);
        let label = self.section(b, false);
        self.out.push_str(&b.text());
        self.out.push_str(&label);
        self.included(b);
    }

    /// A changed block and the old block it replaces.
    fn pair(&mut self, ob: &Block, nb: &Block) {
        self.begin_new(nb);
        if nb.kind == Kind::Setup {
            let (ot, nt) = (ob.toks(), nb.toks());
            if !(ot.len() == 1 && nt.len() == 1 && self.diff_code(&ot[0], &nt[0])) {
                self.unmarked(nb);
            }
            self.included(nb);
            return;
        }
        let label = self.section(nb, false);
        let (prefix, nbody, suffix) = nb.parts();
        let (_, obody, _) = ob.parts();
        self.out.push_str(&prefix);
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
        self.out.push_str(&suffix);
        self.out.push_str(&label);
    }

    fn inserted(&mut self, nb: &Block) {
        self.begin_new(nb);
        if nb.kind == Kind::Setup {
            let at = self.out.len();
            if nb.is_import() || !self.ctx.opts.content_blocks || !self.inserted_code(&nb.nodes) {
                self.out.truncate(at);
                self.unmarked(nb);
            }
            self.included(nb);
            return;
        }
        let label = self.section(nb, true);
        let (prefix, nodes, suffix) = nb.parts();
        self.out.push_str(&prefix);
        let toks = tokenize(&nodes);
        self.mark(
            ChangeKind::Ins,
            "block: true",
            self.content(&toks, ChangeKind::Ins),
            &toks,
        );
        self.out.push_str(&suffix);
        self.out.push_str(&label);
    }

    fn deleted(&mut self, ob: &Block) {
        if ob.kind == Kind::Setup {
            // A deleted include of a file that is gone shows its content,
            // through a root-relative path.
            let path = ob.include_target().and_then(|t| resolve(&self.file.old_path, &t));
            match path {
                Some(p) if !self.ctx.pairs.values().any(|o| *o == p) && source(&self.ctx.old_files, &p).is_some() => {
                    self.old_scope = true;
                    let root = parse(&format!("#include {}", typst_str(&format!("/{OLD_DIR}/{p}"))));
                    self.deleted(&Block {
                        kind: Kind::Para,
                        gap: String::new(),
                        nodes: root.children().cloned().collect(),
                        key: String::new(),
                    });
                }
                _ if !ob.is_import() => self.record(ChangeKind::Unmarked, ob.text()),
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
                self.sections.push(Section {
                    file: self.file.path.clone(),
                    level,
                    target: None,
                    title: format!("[{body}]"),
                    deleted: true,
                    new: false,
                });
            }
            args = format!("heading: {level}");
        }
        self.summary_here();
        // An indented continuation line, not a block or item of its own: it
        // must not end a list, number an item or add a paragraph break that
        // the new version lacks.
        self.out.push_str(&format!("\n{}  ", self.file.indent));
        self.mark(ChangeKind::Del, &args, body, &toks);
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

    /// Source for deleted content: labels removed, references to unknown
    /// targets turned into text. `None` if the code cannot be kept live:
    /// deleted code is not evaluated (and then neither is math using removed
    /// definitions), or it uses deleted labels.
    fn sanitize(&self, nodes: &[SyntaxNode]) -> Option<String> {
        let mut out = String::new();
        for n in nodes {
            self.sanitize_into(n, true, &mut out)?;
        }
        Some(out)
    }

    fn sanitize_into(&self, n: &SyntaxNode, markup: bool, out: &mut String) -> Option<()> {
        let eval = self.ctx.opts.eval_deleted;
        match n.kind() {
            K::Label if markup => {}
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
                let m = n.kind() == K::Markup;
                for c in n.children() {
                    self.sanitize_into(c, m, out)?;
                }
            }
        }
        Some(())
    }

    fn diff_tokens(&mut self, old: &[Tok], new: &[Tok]) {
        let ok: Vec<&str> = old.iter().map(|t| t.key.as_str()).collect();
        let nk: Vec<&str> = new.iter().map(|t| t.key.as_str()).collect();
        let mut segs = vec![];
        for op in capture_diff_slices(Algorithm::Myers, &ok, &nk) {
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
                } else if !self.diff_code(ot, nt) {
                    self.change(std::slice::from_ref(ot), std::slice::from_ref(nt));
                }
            }
        }
    }

    /// Write code `n` with the changes inside its content blocks against
    /// `o` marked, and a changed argument that is shown as content replaced
    /// by the old one deleted and the new one inserted. Writes nothing and
    /// returns false if they differ elsewhere, or if diffing inside code is
    /// off.
    fn diff_code(&mut self, o: &Tok, n: &Tok) -> bool {
        self.ctx.opts.content_blocks && self.diff_code_nodes(&o.nodes, &n.nodes, false)
    }

    /// Labels, comments and spaces are written as in `n` and compared only
    /// for the summary: they do not show. `args` says that the nodes are
    /// the arguments of a call.
    fn diff_code_nodes(&mut self, o: &[SyntaxNode], n: &[SyntaxNode], args: bool) -> bool {
        let (at, count) = (self.out.len(), self.changes.len());
        let trivia = |x: &&SyntaxNode| matches!(x.kind(), K::Label | K::LineComment | K::BlockComment | K::Space);
        // A call that takes a content block likely shows its other
        // positional arguments too: a body, a cell. Others, such as `str`
        // or `rgb`, would fail on content, and the call is replaced whole.
        let shows = args
            && n.iter().any(|x| {
                x.kind() == K::ContentBlock
                    || x.kind() == K::Named && x.children().last().is_some_and(|v| v.kind() == K::ContentBlock)
            });
        let mut rest = o.iter().filter(|x| !trivia(x));
        let mut ok = true;
        let mut line_start = false;
        for x in n {
            if trivia(&x) {
                self.out.push_str(&x.full_text());
                line_start = x.full_text().contains('\n');
                continue;
            }
            ok = ok
                && rest.next().is_some_and(|y| {
                    self.diff_code_node(y, x)
                        || shows && matches!(x.kind(), K::FuncCall | K::Equation) && self.replace(y, x, line_start)
                });
            line_start = false;
        }
        let ok = ok && rest.next().is_none();
        let labels = |v: &[SyntaxNode]| {
            v.iter()
                .filter(|x| x.kind() == K::Label)
                .map(|x| x.full_text())
                .collect::<Vec<_>>()
        };
        if ok && labels(o) != labels(n) {
            self.record(ChangeKind::Unmarked, labels(n).concat().to_string());
        }
        if !ok {
            self.out.truncate(at);
            self.changes.truncate(count);
        }
        ok
    }

    fn diff_code_node(&mut self, o: &SyntaxNode, n: &SyntaxNode) -> bool {
        if o.kind() != n.kind() {
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
        self.diff_code_nodes(&children(o), &children(n), n.kind() == K::Args)
    }

    /// Write a content argument `n` as `o` deleted and `n` inserted, as
    /// blocks if `n` starts a line.
    fn replace(&mut self, o: &SyntaxNode, n: &SyntaxNode, block: bool) -> bool {
        let tok = |x: &SyntaxNode| {
            let nodes = vec![SyntaxNode::leaf(K::Hash, "#"), x.clone()];
            Tok {
                kind: TK::Atom,
                key: String::new(),
                text: text(&nodes),
                nodes,
                block,
            }
        };
        let (ot, nt) = ([tok(o)], [tok(n)]);
        let args = block_arg(&nt);
        self.out.push('[');
        self.mark(ChangeKind::Del, args, self.content(&ot, ChangeKind::Del), &ot);
        self.mark(ChangeKind::Ins, args, self.content(&nt, ChangeKind::Ins), &nt);
        self.out.push(']');
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
                    self.mark(
                        ChangeKind::Ins,
                        "block: true",
                        self.content(core, ChangeKind::Ins),
                        core,
                    );
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
            if self.diff_code(o, n) {
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
                self.mark(ChangeKind::Ins, args, self.content(core, ChangeKind::Ins), core);
            }
            self.out.push_str(&trail);
            self.out.push_str(gap.map_or("", |t| &t.text));
        }
        self.out.push_str(&ntrail);
    }
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

/// A Typst string literal holding `s`.
fn typst_str(s: &str) -> String {
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
