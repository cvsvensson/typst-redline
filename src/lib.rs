//! Produce a Typst document that shows the new version of a file with the
//! changes against an old version marked.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use similar::{Algorithm, DiffOp, capture_diff_slices};
use typst_syntax::{SyntaxKind as K, SyntaxNode, ast, parse};

pub const PREAMBLE: &str = include_str!("preamble.typ");

/// Path of the preamble in the output, relative to the project root.
pub const PREAMBLE_PATH: &str = ".redline-preamble.typ";

/// A version of a project: `read` returns the file at a root-relative path
/// with `/` separators, or `None` if it does not exist; `entry` is the path
/// of the main file.
pub struct Version<'a> {
    pub label: String,
    pub entry: String,
    pub read: &'a dyn Fn(&str) -> Option<String>,
}

pub struct Options {
    /// Typst source defining the functions the output calls (see [`PREAMBLE`]).
    pub preamble: String,
    /// Evaluate deleted code expressions; otherwise show them as source text.
    pub eval_deleted: bool,
    /// Diff inside the content blocks of code expressions that differ only
    /// there; otherwise such expressions are compared whole.
    pub content_blocks: bool,
}

pub struct Redline {
    /// Files to lay over the new version, as (root-relative path, content).
    pub files: Vec<(String, String)>,
    pub changes: Vec<Change>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Change {
    pub file: String,
    /// Label of the enclosing section, `None` before the first section.
    pub section: Option<String>,
    pub kind: ChangeKind,
    /// Typst markup of the changed content.
    pub text: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ChangeKind {
    Ins,
    Del,
    /// A change the output does not show.
    Unmarked,
}

impl ChangeKind {
    fn name(self) -> &'static str {
        match self {
            ChangeKind::Ins => "ins",
            ChangeKind::Del => "del",
            ChangeKind::Unmarked => "unmarked",
        }
    }
}

/// The new version of the project with the changes against the old version
/// marked in every file the entry includes or imports, a summary page in
/// the entry, and the preamble the marked files import.
pub fn redline(old: &Version, new: &Version, opts: &Options) -> Redline {
    let mut warnings = vec![];
    let old_files = project(old, &mut warnings);
    let new_files = project(new, &mut warnings);
    let pairs = pair_files(&old_files, &new_files);
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
    let ctx = Ctx {
        prefix,
        eval_deleted: opts.eval_deleted,
        content_blocks: opts.content_blocks,
        new_targets: union(&new_files, targets),
        removed_defs: union(&old_files, definitions)
            .difference(&union(&new_files, definitions))
            .cloned()
            .collect(),
        old_files,
        new_files,
        pairs,
    };
    let mut w = Writer::new(&ctx, &new.entry);
    w.file(&new.entry);
    w.top = false;
    w.current = None;
    for (path, _) in &ctx.new_files {
        w.file(path);
    }
    let mut files = vec![
        (PREAMBLE_PATH.into(), opts.preamble.clone()),
        (new.entry.clone(), w.finish(&old.label, &new.label)),
    ];
    files.append(&mut w.files);
    // An old file no longer reachable is staged in its old version, so that
    // deleted code including it shows what was deleted.
    for (path, src) in &ctx.old_files {
        let stale = (new.read)(path).is_some_and(|s| normalize(&s) != *src);
        if stale && !ctx.pairs.values().any(|o| o == path) {
            files.push((path.clone(), src.clone()));
        }
    }
    Redline {
        files,
        changes: w.changes,
        warnings,
    }
}

/// The entry and the files it includes or imports by a literal path,
/// directly or not, with their sources. A missing entry is empty.
fn project(v: &Version, warnings: &mut Vec<String>) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = vec![];
    let mut queue = vec![v.entry.clone()];
    while let Some(path) = queue.pop() {
        if files.iter().any(|(p, _)| *p == path) {
            continue;
        }
        let Some(src) = (v.read)(&path).map(|s| normalize(&s)) else {
            if path == v.entry {
                warnings.push(format!("{path} does not exist in {}", v.label));
                files.push((path, String::new()));
            }
            continue;
        };
        let mut deps = vec![];
        walk(&parse(&src), &mut |n| {
            let (source, include) = if let Some(i) = n.cast::<ast::ModuleInclude>() {
                (i.source(), true)
            } else if let Some(i) = n.cast::<ast::ModuleImport>() {
                (i.source(), false)
            } else {
                return;
            };
            let warn = match source {
                ast::Expr::Str(s) if s.get().starts_with('@') => None,
                ast::Expr::Str(s) => match resolve(&path, &s.get()) {
                    Some(p) => {
                        deps.push(p);
                        None
                    }
                    None => Some(format!("{path}: {} is outside the root", s.get())),
                },
                _ if include => Some(format!(
                    "{path}: the included path is computed, so changes inside it are not marked"
                )),
                _ => None,
            };
            if let Some(w) = warn.filter(|w| !warnings.contains(w)) {
                warnings.push(w);
            }
        });
        files.push((path, src));
        // Depth first in source order, so that files come in reading order.
        queue.extend(deps.into_iter().rev());
    }
    files
}

/// A path relative to the file `from`, or to the root if it starts with
/// `/`, as a normalized root-relative path; `None` if it leaves the root.
fn resolve(from: &str, target: &str) -> Option<String> {
    let path = match (target.strip_prefix('/'), from.rsplit_once('/')) {
        (Some(abs), _) => abs.to_string(),
        (None, Some((dir, _))) => format!("{dir}/{target}"),
        (None, None) => target.to_string(),
    };
    let mut parts = vec![];
    for c in path.split('/') {
        match c {
            "" | "." => {}
            ".." => drop(parts.pop()?),
            c => parts.push(c),
        }
    }
    Some(parts.join("/"))
}

/// The old file each new file is compared with: the entries with each
/// other, then files at the same path, then the most similar remaining file
/// (a renamed file).
fn pair_files(old: &[(String, String)], new: &[(String, String)]) -> HashMap<String, String> {
    let mut pairs = HashMap::from([(new[0].0.clone(), old[0].0.clone())]);
    for (p, _) in &new[1..] {
        if p != &old[0].0 && old.iter().any(|(q, _)| q == p) {
            pairs.insert(p.clone(), p.clone());
        }
    }
    let toks = |s: &str| words(&tokenize(&parse(s).children().cloned().collect::<Vec<_>>()));
    for (p, s) in &new[1..] {
        if pairs.contains_key(p) {
            continue;
        }
        let ntoks = toks(s);
        let best = old
            .iter()
            .filter(|(q, _)| !pairs.values().any(|o| o == q))
            .map(|(q, os)| (q, similarity(&toks(os), &ntoks)))
            .filter(|&(_, s)| s >= 0.4)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((q, _)) = best {
            pairs.insert(p.clone(), q.clone());
        }
    }
    pairs
}

/// The literal path of an `#include` block.
fn include_target(b: &Block) -> Option<String> {
    match b.nodes.get(1)?.cast::<ast::ModuleInclude>()?.source() {
        ast::Expr::Str(s) => Some(s.get().to_string()),
        _ => None,
    }
}

/// An `#import` or `#include` block: its changes show in the files it
/// names, not in the line itself.
fn is_import(b: &Block) -> bool {
    b.nodes
        .get(1)
        .is_some_and(|n| matches!(n.kind(), K::ModuleImport | K::ModuleInclude))
}

/// Strip a byte-order mark and convert CRLF and CR line endings to LF.
fn normalize(s: &str) -> String {
    s.strip_prefix('\u{feff}')
        .unwrap_or(s)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
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

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Setup,
    Heading,
    Item,
    Para,
}

struct Block {
    kind: Kind,
    /// Whitespace between the previous block and this one.
    gap: String,
    nodes: Vec<SyntaxNode>,
    key: String,
}

impl Block {
    fn text(&self) -> String {
        self.nodes.iter().map(|n| n.full_text().to_string()).collect()
    }

    /// The node holding the editable content (heading or item), if any.
    fn head(&self) -> Option<&SyntaxNode> {
        matches!(self.kind, Kind::Heading | Kind::Item).then(|| &self.nodes[0])
    }

    /// Text before the body markup (`= `, `- `, `/ Term: `), the body nodes
    /// and the trailing text (label) of a heading or item; for a paragraph,
    /// all nodes.
    fn parts(&self) -> (String, Vec<SyntaxNode>, String) {
        let body = self
            .head()
            .and_then(|h| h.children().rposition(|c| c.kind() == K::Markup).map(|i| (h, i)));
        let Some((head, i)) = body else {
            return (String::new(), self.nodes.clone(), String::new());
        };
        let prefix = head.children().take(i).map(|c| c.full_text().to_string()).collect();
        let suffix = self.nodes[1..].iter().map(|n| n.full_text().to_string()).collect();
        (
            prefix,
            head.children().nth(i).unwrap().children().cloned().collect(),
            suffix,
        )
    }

    fn toks(&self) -> Vec<Tok> {
        tokenize(&self.parts().1)
    }

    /// The marker and label around the body; blocks that differ in it are
    /// not compared word by word, so that the change is marked.
    fn frame(&self) -> String {
        let (prefix, _, suffix) = self.parts();
        collapse_ws(&format!("{prefix}\n{suffix}"))
    }

    fn label(&self) -> Option<String> {
        self.nodes.iter().find(|n| n.kind() == K::Label).map(label_name)
    }

    fn heading_level(&self) -> usize {
        self.nodes[0]
            .children()
            .find(|c| c.kind() == K::HeadingMarker)
            .map_or(1, |m| m.len())
    }
}

struct Doc {
    blocks: Vec<Block>,
    trailing: String,
}

impl Doc {
    fn new(children: &[SyntaxNode]) -> Self {
        let mut doc = Doc {
            blocks: vec![],
            trailing: String::new(),
        };
        let mut cur: Vec<SyntaxNode> = vec![];
        let mut kind = Kind::Para;
        let mut ws: Vec<SyntaxNode> = vec![];
        let mut gap = String::new();
        let flush =
            |doc: &mut Doc, cur: &mut Vec<SyntaxNode>, ws: &mut Vec<SyntaxNode>, gap: &mut String, kind: Kind| {
                if cur.is_empty() {
                    return;
                }
                let nodes = std::mem::take(cur);
                let key = format!(
                    "{kind:?}:{}",
                    collapse_ws(&nodes.iter().map(|n| n.full_text().to_string()).collect::<String>())
                );
                doc.blocks.push(Block {
                    kind,
                    gap: std::mem::take(gap),
                    nodes,
                    key,
                });
                *gap = ws.drain(..).map(|n| n.full_text().to_string()).collect();
            };
        let mut i = 0;
        while i < children.len() {
            let n = &children[i];
            match n.kind() {
                K::Parbreak => {
                    flush(&mut doc, &mut cur, &mut ws, &mut gap, kind);
                    gap.extend(ws.drain(..).map(|n| n.full_text().to_string()));
                    gap.push_str(&n.full_text());
                }
                K::Space | K::LineComment | K::BlockComment => {
                    if cur.is_empty() {
                        gap.push_str(&n.full_text());
                    } else if kind != Kind::Para && n.full_text().contains('\n') {
                        flush(&mut doc, &mut cur, &mut ws, &mut gap, kind);
                        gap.push_str(&n.full_text());
                    } else {
                        ws.push(n.clone());
                    }
                }
                K::Label if !cur.is_empty() => {
                    cur.append(&mut ws);
                    cur.push(n.clone());
                }
                _ => {
                    let mut unit = vec![n.clone()];
                    let mut k = Kind::Para;
                    match n.kind() {
                        K::Heading => k = Kind::Heading,
                        kind if is_item(kind) => k = Kind::Item,
                        K::Hash if i + 1 < children.len() => {
                            i += 1;
                            unit.push(children[i].clone());
                            if is_setup(&children[i]) {
                                k = Kind::Setup;
                            }
                            if children.get(i + 1).is_some_and(|c| c.kind() == K::Semicolon) {
                                i += 1;
                                unit.push(children[i].clone());
                            }
                        }
                        _ => {}
                    }
                    if k != Kind::Para || kind != Kind::Para {
                        flush(&mut doc, &mut cur, &mut ws, &mut gap, kind);
                    }
                    kind = k;
                    cur.append(&mut ws);
                    cur.extend(unit);
                }
            }
            i += 1;
        }
        flush(&mut doc, &mut cur, &mut ws, &mut gap, kind);
        doc.trailing = std::mem::take(&mut gap);
        doc
    }

    fn keys(&self) -> Vec<&str> {
        self.blocks.iter().map(|b| b.key.as_str()).collect()
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum TK {
    Word,
    Space,
    Atom,
    Comment,
}

#[derive(Clone, Debug)]
struct Tok {
    kind: TK,
    key: String,
    text: String,
    nodes: Vec<SyntaxNode>,
    /// Display equation or code alone on its line: laid out as a block.
    block: bool,
}

fn tokenize(nodes: &[SyntaxNode]) -> Vec<Tok> {
    let mut toks: Vec<Tok> = vec![];
    let mut i = 0;
    while i < nodes.len() {
        let n = &nodes[i];
        match n.kind() {
            K::Text => {
                for piece in split_words(n.leaf_text()) {
                    let space = piece.chars().all(char::is_whitespace);
                    toks.push(Tok {
                        kind: if space { TK::Space } else { TK::Word },
                        key: if space { " ".into() } else { piece.to_string() },
                        // Literal inside a word, but markup next to a wrapper.
                        text: piece.replace('_', "\\_").replace('*', "\\*"),
                        nodes: vec![],
                        block: false,
                    });
                }
            }
            K::Space | K::Parbreak => {
                let t = n.full_text().to_string();
                let key = if n.kind() == K::Parbreak { "\n\n" } else { " " };
                toks.push(Tok {
                    kind: TK::Space,
                    key: key.into(),
                    text: t,
                    nodes: vec![],
                    block: false,
                });
            }
            K::LineComment | K::BlockComment => {
                toks.push(Tok {
                    kind: TK::Comment,
                    key: "//".into(),
                    text: n.full_text().to_string(),
                    nodes: vec![],
                    block: false,
                });
            }
            K::Label
                if toks
                    .iter()
                    .rev()
                    .find(|t| t.kind != TK::Space)
                    .is_some_and(|t| t.kind == TK::Atom) =>
            {
                let mut spaces = vec![];
                while toks.last().is_some_and(|t| t.kind == TK::Space) {
                    spaces.push(toks.pop().unwrap());
                }
                let atom = toks.last_mut().unwrap();
                for s in spaces.into_iter().rev() {
                    atom.text.push_str(&s.text);
                    atom.nodes.push(SyntaxNode::leaf(K::Space, s.text));
                }
                atom.text.push_str(&n.full_text());
                atom.nodes.push(n.clone());
                atom.key = skeleton(&atom.nodes);
            }
            _ => {
                let mut unit = vec![n.clone()];
                if n.kind() == K::Hash && i + 1 < nodes.len() {
                    i += 1;
                    unit.push(nodes[i].clone());
                    if nodes.get(i + 1).is_some_and(|c| c.kind() == K::Semicolon) {
                        i += 1;
                        unit.push(nodes[i].clone());
                    }
                }
                let text: String = unit.iter().map(|n| n.full_text().to_string()).collect();
                let line_start = toks.last().is_none_or(|t| t.kind == TK::Space && t.text.contains('\n'));
                let line_end = nodes.get(i + 1).is_none_or(|c| c.full_text().starts_with('\n'));
                let display = n.kind() == K::Equation && n.children().nth(1).is_some_and(|c| c.kind() == K::Space);
                toks.push(Tok {
                    kind: TK::Atom,
                    key: skeleton(&unit),
                    text,
                    block: display || (n.kind() == K::Hash && line_start && line_end),
                    nodes: unit,
                });
            }
        }
        i += 1;
    }
    toks
}

/// Split text into runs of whitespace and of other characters, so that
/// punctuation stays with its word.
fn split_words(s: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut start = 0;
    // Scripts written without spaces between words: each character is a word.
    let unspaced = |c: char| {
        matches!(c as u32, 0x0E00..=0x0EFF | 0x1000..=0x109F | 0x1780..=0x17FF | 0x2E80..=0x9FFF
            | 0xF900..=0xFAFF | 0xFF66..=0xFF9F | 0x20000..=0x3FFFF)
    };
    let class = |c: char| {
        if unspaced(c) {
            2
        } else if c.is_whitespace() {
            1
        } else {
            0
        }
    };
    let mut prev: Option<u8> = None;
    for (i, c) in s.char_indices() {
        let k = class(c);
        if let Some(p) = prev
            && (p != k || k == 2)
        {
            out.push(&s[start..i]);
            start = i;
        }
        prev = Some(k);
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// Code taken from the new version unmarked: rules, bindings, and calls
/// that cannot be wrapped or must not appear twice.
fn is_setup(expr: &SyntaxNode) -> bool {
    match expr.kind() {
        K::SetRule | K::ShowRule | K::LetBinding | K::ModuleImport | K::ModuleInclude => true,
        K::FuncCall => expr.children().next().is_some_and(|f| {
            f.kind() == K::Ident
                && matches!(
                    f.leaf_text().as_str(),
                    "pagebreak" | "colbreak" | "bibliography" | "outline" | "v" | "assert"
                )
        }),
        _ => false,
    }
}

fn is_item(kind: K) -> bool {
    matches!(kind, K::ListItem | K::EnumItem | K::TermItem)
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn label_name(n: &SyntaxNode) -> String {
    let t = n.leaf_text();
    t.trim_start_matches('<').trim_end_matches('>').to_string()
}

fn ref_name(n: &SyntaxNode) -> String {
    n.children()
        .next()
        .map_or(String::new(), |m| m.leaf_text().trim_start_matches('@').to_string())
}

/// Labels and citation keys that a reference in the output may use: the new
/// version's labels and everything the new version references.
fn targets(root: &SyntaxNode) -> HashSet<String> {
    let mut set = HashSet::new();
    walk(root, &mut |n| match n.kind() {
        K::Label => drop(set.insert(label_name(n))),
        K::Ref => drop(set.insert(ref_name(n))),
        _ => {}
    });
    set
}

/// Names bound by top-level `#let` and `#import` items.
fn definitions(root: &SyntaxNode) -> HashSet<String> {
    let mut set = HashSet::new();
    for n in root.children() {
        match n.kind() {
            K::LetBinding => {
                let name = n.children().find_map(|c| match c.kind() {
                    K::Ident => Some(c.leaf_text().to_string()),
                    K::Closure => c
                        .children()
                        .find(|d| d.kind() == K::Ident)
                        .map(|d| d.leaf_text().to_string()),
                    _ => None,
                });
                set.extend(name);
            }
            K::ModuleImport => {
                for c in n.children().filter(|c| c.kind() == K::ImportItems) {
                    walk(c, &mut |d| {
                        if d.kind() == K::Ident {
                            set.insert(d.leaf_text().to_string());
                        }
                    });
                }
            }
            _ => {}
        }
    }
    set
}

fn walk(n: &SyntaxNode, f: &mut impl FnMut(&SyntaxNode)) {
    f(n);
    for c in n.children() {
        walk(c, f);
    }
}

struct Ctx {
    /// Prefix of the names the output uses for the preamble's functions and
    /// its own labels.
    prefix: String,
    eval_deleted: bool,
    content_blocks: bool,
    new_targets: HashSet<String>,
    removed_defs: HashSet<String>,
    old_files: Vec<(String, String)>,
    new_files: Vec<(String, String)>,
    /// New file path to the path of the old file it is compared with.
    pairs: HashMap<String, String>,
}

fn source<'a>(files: &'a [(String, String)], path: &str) -> Option<&'a str> {
    files.iter().find(|(p, _)| p == path).map(|(_, s)| s.as_str())
}

impl Ctx {
    /// The line importing the preamble's functions under the prefix.
    fn import(&self) -> String {
        let p = &self.prefix;
        let names = ["ins", "del", "summary", "bars"].map(|n| match p.as_str() {
            "redline" => format!("redline-{n}"),
            _ => format!("redline-{n} as {p}-{n}"),
        });
        format!("#import \"/{PREAMBLE_PATH}\": {}\n", names.join(", "))
    }

    /// Source for deleted content: labels removed, references to unknown
    /// targets turned into text. Returns `None` if the code cannot be kept live
    /// (it uses removed definitions or deleted labels).
    fn sanitize(&self, nodes: &[SyntaxNode], removed: &HashSet<String>) -> Option<String> {
        let mut out = String::new();
        for n in nodes {
            self.sanitize_into(n, true, removed, &mut out)?;
        }
        Some(out)
    }

    fn sanitize_into(&self, n: &SyntaxNode, markup: bool, removed: &HashSet<String>, out: &mut String) -> Option<()> {
        match n.kind() {
            K::Label if markup => {}
            K::Label => {
                if !self.new_targets.contains(&label_name(n)) {
                    return None;
                }
                out.push_str(n.leaf_text());
            }
            K::Ref => {
                let name = ref_name(n);
                if self.new_targets.contains(&name) {
                    out.push_str(&n.full_text());
                } else {
                    out.push_str(&format!("\\@{name}"));
                }
            }
            K::Hash if !self.eval_deleted => return None,
            K::Error => return None,
            K::Ident | K::MathIdent if removed.contains(n.leaf_text().as_str()) => return None,
            _ if n.children().len() == 0 => out.push_str(n.leaf_text()),
            _ => {
                let m = n.kind() == K::Markup;
                for c in n.children() {
                    self.sanitize_into(c, m, removed, out)?;
                }
            }
        }
        Some(())
    }
}

struct Section {
    file: String,
    level: usize,
    target: Option<String>,
    /// Typst expression.
    title: String,
    deleted: bool,
}

struct Writer<'a> {
    ctx: &'a Ctx,
    entry: String,
    /// The file being written and the old file it is compared with.
    file: String,
    old_file: String,
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
    /// Indentation of the list item body being written.
    indent: String,
    /// Names deleted code in this file must not use: defined in the old
    /// file or project but not in the new one.
    removed: HashSet<String>,
}

impl<'a> Writer<'a> {
    fn new(ctx: &'a Ctx, entry: &str) -> Self {
        Writer {
            ctx,
            entry: entry.into(),
            file: entry.into(),
            old_file: String::new(),
            out: String::new(),
            entry_out: String::new(),
            files: vec![],
            done: HashSet::new(),
            top: true,
            summary_at: None,
            sections: vec![],
            current: None,
            changes: vec![],
            indent: String::new(),
            removed: HashSet::new(),
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
        if path != self.entry && old_src == new_src {
            return;
        }
        let out = std::mem::take(&mut self.out);
        let file = std::mem::replace(&mut self.file, path.into());
        let old_file = std::mem::replace(&mut self.old_file, old_path);
        let indent = std::mem::take(&mut self.indent);
        let (o, n) = (parse(old_src), parse(new_src));
        let (mut removed, kept) = (definitions(&o), definitions(&n));
        removed.retain(|d| !kept.contains(d));
        removed.extend(ctx.removed_defs.iter().cloned());
        let removed = std::mem::replace(&mut self.removed, removed);
        let children = |r: &SyntaxNode| r.children().cloned().collect::<Vec<_>>();
        self.docs(&Doc::new(&children(&o)), &Doc::new(&children(&n)));
        let marked = std::mem::replace(&mut self.out, out);
        self.file = file;
        self.old_file = old_file;
        self.indent = indent;
        self.removed = removed;
        if path == self.entry {
            self.entry_out = marked;
        } else {
            let marked = format!("{}{marked}", ctx.import());
            self.files.push((path.into(), marked));
        }
    }

    /// Mark the file a new `#include` block includes, here, so that its
    /// sections come in reading order.
    fn included(&mut self, b: &Block) {
        if let Some(path) = include_target(b).and_then(|t| resolve(&self.file, &t)) {
            self.file(&path);
        }
    }

    fn record(&mut self, kind: ChangeKind, text: String) {
        self.changes.push(Change {
            file: self.file.clone(),
            section: self.current.clone(),
            kind,
            text,
        });
    }

    /// Emit `body` wrapped as a change and record it.
    fn mark(&mut self, kind: ChangeKind, args: &str, body: String) {
        let p = &self.ctx.prefix;
        self.out.push_str(&format!("#{p}-{}({args})[{body}];", kind.name()));
        self.record(kind, body);
    }

    /// The entry's output source: the preamble import and the marked
    /// document with the summary at the place [`Self::summary_here`] chose.
    fn finish(&self, old_label: &str, new_label: &str) -> String {
        let count = |f: &dyn Fn(&Change) -> bool| self.changes.iter().filter(|c| f(c)).count();
        let front = |file: &str, title: String| Section {
            file: file.into(),
            level: 1,
            target: None,
            title,
            deleted: false,
        };
        // Changes outside sections: before the entry's first heading, and in
        // files that are not read in order (imported, or included by code).
        let others = self
            .files
            .iter()
            .map(|(p, _)| front(p, format!("[Definitions in #raw({}), marked where used]", typst_str(p))));
        let all: Vec<Section> = std::iter::once(front(&self.entry, "[Before the first section]".into()))
            .chain(others)
            .collect();
        let (first, rest) = all.split_at(1);
        let mut entries = String::new();
        for s in first.iter().chain(&self.sections).chain(rest) {
            let here = |c: &Change| c.section == s.target && (s.target.is_some() || c.file == s.file);
            let n = if s.deleted { 0 } else { count(&here) };
            if n == 0 && !s.deleted {
                continue;
            }
            let unmarked = !s.deleted && count(&|c| here(c) && c.kind == ChangeKind::Unmarked) > 0;
            let target = s.target.as_ref().map_or("none".to_string(), |l| format!("<{l}>"));
            entries.push_str(&format!(
                "  (file: {}, level: {}, target: {target}, title: {}, count: {n}, deleted: {}, unmarked: {unmarked}),\n",
                typst_str(&s.file),
                s.level,
                s.title,
                s.deleted,
            ));
        }
        let at = self.summary_at.unwrap_or(self.entry_out.len());
        let mut out = self.entry_out.clone();
        let p = &self.ctx.prefix;
        let summary = format!(
            "{}#{p}-summary((\n  old: {}, new: {}, ins: {}, del: {},\n  entries: (\n{entries}  ),\n))\n#set page(foreground: {p}-bars)\n",
            if out[..at].ends_with('\n') || at == 0 { "" } else { "\n" },
            typst_str(old_label),
            typst_str(new_label),
            count(&|c| c.kind == ChangeKind::Ins),
            count(&|c| c.kind == ChangeKind::Del),
        );
        out.insert_str(at, &summary);
        format!("{}{out}", self.ctx.import())
    }

    /// The summary goes at the top level of the entry, before anything
    /// other than the rules and definitions it starts with.
    fn summary_here(&mut self) {
        if self.top && self.file == self.entry && self.summary_at.is_none() {
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

    fn docs(&mut self, old: &Doc, new: &Doc) {
        for op in capture_diff_slices(Algorithm::Patience, &old.keys(), &new.keys()) {
            let (o, n) = (op.old_range(), op.new_range());
            match op {
                DiffOp::Equal { .. } => {
                    for (i, j) in o.zip(n) {
                        self.pair(&old.blocks[i], &new.blocks[j], true);
                    }
                }
                _ => self.region(&old.blocks[o], &new.blocks[n]),
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
            let best = (p..old.len().min(p + 32))
                .filter(|&q| old[q].kind == nb.kind && old[q].frame() == nb.frame())
                .map(|q| (q, similarity(&owords[q], &nwords)))
                .filter(|&(_, s)| s >= 0.4)
                .max_by(|a, b| a.1.total_cmp(&b.1));
            match best {
                Some((q, _)) => {
                    self.unpaired(&old[p..q], &pending);
                    pending.clear();
                    self.pair(&old[q], nb, false);
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

    fn heading_start(&mut self, nb: &Block) -> String {
        let level = nb.heading_level();
        if level > 2 || !self.top {
            return String::new();
        }
        let (target, extra) = match nb.label() {
            Some(l) => (l, String::new()),
            None => {
                let l = format!("{}-sec-{}", self.ctx.prefix, self.sections.len());
                (l.clone(), format!(" <{l}>"))
            }
        };
        let title: String = nb
            .toks()
            .iter()
            .filter(|t| t.kind != TK::Comment)
            .map(|t| t.text.as_str())
            .collect();
        self.sections.push(Section {
            file: self.file.clone(),
            level,
            target: Some(target.clone()),
            title: format!("[{}]", title.trim()),
            deleted: false,
        });
        self.current = Some(target);
        extra
    }

    fn pair(&mut self, ob: &Block, nb: &Block, equal: bool) {
        self.begin_new(nb);
        let extra = if nb.kind == Kind::Heading {
            self.heading_start(nb)
        } else {
            String::new()
        };
        if nb.kind == Kind::Setup {
            let (ot, nt) = (ob.toks(), nb.toks());
            let diffed = !equal && ot.len() == 1 && nt.len() == 1 && self.code(&ot[0], &nt[0]);
            if !diffed {
                self.out.push_str(&nb.text());
                if !equal && !is_import(nb) {
                    self.record(ChangeKind::Unmarked, nb.text());
                }
            }
            self.included(nb);
            return;
        }
        if equal {
            self.out.push_str(&nb.text());
            self.out.push_str(&extra);
            return;
        }
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
        let outer = std::mem::replace(&mut self.indent, indent);
        self.docs(&odoc, &ndoc);
        self.indent = outer;
        self.out.push_str(&suffix);
        self.out.push_str(&extra);
    }

    fn inserted(&mut self, nb: &Block) {
        self.begin_new(nb);
        if nb.kind == Kind::Setup {
            let at = self.out.len();
            if is_import(nb) || !self.ctx.content_blocks || !self.inserted_code(&nb.nodes) {
                self.out.truncate(at);
                self.out.push_str(&nb.text());
                if !is_import(nb) {
                    self.record(ChangeKind::Unmarked, nb.text());
                }
            }
            self.included(nb);
            return;
        }
        let extra = if nb.kind == Kind::Heading {
            self.heading_start(nb)
        } else {
            String::new()
        };
        let (prefix, nodes, suffix) = nb.parts();
        self.out.push_str(&prefix);
        let body = self.content(&tokenize(&nodes), ChangeKind::Ins);
        self.mark(ChangeKind::Ins, "block: true", body);
        self.out.push_str(&suffix);
        self.out.push_str(&extra);
    }

    fn deleted(&mut self, ob: &Block) {
        if ob.kind == Kind::Setup {
            // A deleted include of a file that is gone shows its content,
            // through a root-relative path.
            let path = include_target(ob).and_then(|t| resolve(&self.old_file, &t));
            match path {
                Some(p) if !self.ctx.pairs.values().any(|o| *o == p) && source(&self.ctx.old_files, &p).is_some() => {
                    let root = parse(&format!("#include {}", typst_str(&format!("/{p}"))));
                    self.deleted(&Block {
                        kind: Kind::Para,
                        gap: String::new(),
                        nodes: root.children().cloned().collect(),
                        key: String::new(),
                    });
                }
                _ if !is_import(ob) => self.record(ChangeKind::Unmarked, ob.text()),
                _ => {}
            }
            return;
        }
        let body = self.content(&tokenize(&ob.parts().1), ChangeKind::Del);
        let mut args = "block: true".to_string();
        if ob.kind == Kind::Heading {
            let level = ob.heading_level();
            if level <= 2 && self.top {
                self.sections.push(Section {
                    file: self.file.clone(),
                    level,
                    target: None,
                    title: format!("[{body}]"),
                    deleted: true,
                });
            }
            args = format!("heading: {level}");
        }
        self.summary_here();
        // An indented continuation line, not a block or item of its own: it
        // must not end a list, number an item or add a paragraph break that
        // the new version lacks.
        self.out.push_str(&format!("\n{}  ", self.indent));
        self.mark(ChangeKind::Del, &args, body);
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
                    let line_start = s.rsplit('\n').next().unwrap().trim().is_empty();
                    let mut text = t.text.replace('[', "\\[").replace(']', "\\]");
                    let digits = text.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
                    if line_start && text.starts_with(['-', '+', '=', '/']) {
                        text.insert(0, '\\');
                    } else if line_start && digits > 0 && text[digits..].starts_with('.') {
                        text.insert(digits, '\\');
                    }
                    s.push_str(&text);
                }
                TK::Atom => match side {
                    ChangeKind::Del => match self.ctx.sanitize(&t.nodes, &self.removed) {
                        Some(src) => s.push_str(&src),
                        None => s.push_str(&format!("#raw({});", typst_str(&collapse_ws(&t.text)))),
                    },
                    _ => s.push_str(&t.text),
                },
            }
        }
        // A line break would escape the closing bracket.
        if s.ends_with('\\') {
            s.push(' ');
        }
        s
    }

    fn diff_tokens(&mut self, old: &[Tok], new: &[Tok]) {
        let ok: Vec<&str> = old.iter().map(|t| t.key.as_str()).collect();
        let nk: Vec<&str> = new.iter().map(|t| t.key.as_str()).collect();
        // Segments: (old range, new range, equal).
        let mut segs: Vec<(Range<usize>, Range<usize>, bool)> = vec![];
        for op in capture_diff_slices(Algorithm::Myers, &ok, &nk) {
            let eq = matches!(op, DiffOp::Equal { .. });
            let (o, n) = (op.old_range(), op.new_range());
            match segs.last_mut() {
                Some(last) if !last.2 && !eq => {
                    last.0.end = o.end;
                    last.1.end = n.end;
                }
                _ => segs.push((o, n, eq)),
            }
        }
        // Absorb a kept run between two changes when it is at most one
        // short word, or much shorter than the changes around it: whole
        // phrases replaced read better than alternating fragments.
        let count = |toks: &[Tok]| toks.iter().filter(|t| matches!(t.kind, TK::Word | TK::Atom)).count();
        let size = |r: &(Range<usize>, Range<usize>, bool)| count(&old[r.0.clone()]).max(count(&new[r.1.clone()]));
        let mut merged: Vec<(Range<usize>, Range<usize>, bool)> = vec![];
        let mut i = 0;
        while i < segs.len() {
            let s = segs[i].clone();
            let kept = &new[s.1.clone()];
            let short = kept.iter().all(|t| t.kind != TK::Atom)
                && match count(kept) {
                    0 => true,
                    1 => kept.iter().all(|t| t.kind != TK::Word || t.text.chars().count() <= 4),
                    n => i > 0 && i + 1 < segs.len() && 2 * n < size(merged.last().unwrap()).min(size(&segs[i + 1])),
                };
            if s.2 && i > 0 && i + 1 < segs.len() && short {
                let next = segs[i + 1].clone();
                let last = merged.last_mut().unwrap();
                last.0.end = next.0.end;
                last.1.end = next.1.end;
                i += 2;
                continue;
            }
            match merged.last_mut() {
                Some(last) if !last.2 && !s.2 => {
                    last.0.end = s.0.end;
                    last.1.end = s.1.end;
                }
                _ => merged.push(s),
            }
            i += 1;
        }
        for (o, n, eq) in merged {
            if eq {
                for (ot, nt) in old[o].iter().zip(&new[n]) {
                    if ot.text == nt.text || nt.kind != TK::Atom {
                        self.out.push_str(&nt.text);
                    } else if !self.code(ot, nt) {
                        self.change(std::slice::from_ref(ot), std::slice::from_ref(nt));
                    }
                }
            } else {
                self.change(&old[o], &new[n]);
            }
        }
    }

    /// Write code `n` with the changes inside its content blocks against
    /// `o` marked. Writes nothing and returns false if they differ outside
    /// content blocks, or if diffing inside code is off.
    fn code(&mut self, o: &Tok, n: &Tok) -> bool {
        self.ctx.content_blocks && o.key == n.key && self.code_nodes(&o.nodes, &n.nodes)
    }

    fn code_nodes(&mut self, o: &[SyntaxNode], n: &[SyntaxNode]) -> bool {
        let (at, count) = (self.out.len(), self.changes.len());
        let ok = o.len() == n.len() && o.iter().zip(n).all(|(a, b)| self.code_node(a, b));
        if !ok {
            self.out.truncate(at);
            self.changes.truncate(count);
        }
        ok
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
                    self.mark(ChangeKind::Ins, "", self.content(core, ChangeKind::Ins));
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

    fn code_node(&mut self, o: &SyntaxNode, n: &SyntaxNode) -> bool {
        if o.kind() != n.kind() {
            return false;
        }
        if n.kind() == K::ContentBlock {
            let (om, nm) = (markup(o), markup(n));
            let top = std::mem::replace(&mut self.top, false);
            self.out.push('[');
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
            self.out.push(']');
            self.top = top;
            return true;
        }
        if n.children().len() == 0 {
            let same = o.leaf_text() == n.leaf_text() || n.kind() == K::Space;
            if same {
                self.out.push_str(n.leaf_text());
            }
            return same;
        }
        self.code_nodes(
            &o.children().cloned().collect::<Vec<_>>(),
            &n.children().cloned().collect::<Vec<_>>(),
        )
    }

    /// A changed run: old struck, new highlighted. The new version's
    /// whitespace stays outside the marks; a space the new version lacks
    /// belongs to the deletion.
    fn change(&mut self, old: &[Tok], new: &[Tok]) {
        let (olead, ocore, otrail) = trim_spaces(old);
        let (nlead, ncore, ntrail) = trim_spaces(new);
        self.out.push_str(&nlead);
        if !ocore.is_empty() {
            let pad = |o: &str, n: &str| if o.is_empty() || !n.is_empty() { "" } else { " " };
            let body = self.content(ocore, ChangeKind::Del);
            let body = format!("{}{body}{}", pad(&olead, &nlead), pad(&otrail, &ntrail));
            self.mark(ChangeKind::Del, block_arg(ocore), body);
        }
        if !ncore.is_empty() {
            self.mark(ChangeKind::Ins, block_arg(ncore), self.content(ncore, ChangeKind::Ins));
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

/// The nodes of an item body before its first nested item, and the rest
/// starting at the whitespace before that item.
fn split_nested(body: &[SyntaxNode]) -> (&[SyntaxNode], &[SyntaxNode]) {
    let at = body.iter().position(|n| is_item(n.kind())).map_or(body.len(), |i| {
        if i > 0 && body[i - 1].kind() == K::Space {
            i - 1
        } else {
            i
        }
    });
    body.split_at(at)
}

/// The markup inside a content block.
fn markup(block: &SyntaxNode) -> Vec<SyntaxNode> {
    block
        .children()
        .find(|c| c.kind() == K::Markup)
        .map_or(vec![], |m| m.children().cloned().collect())
}

/// Leading whitespace, the tokens between, and trailing whitespace.
fn trim_spaces(toks: &[Tok]) -> (String, &[Tok], String) {
    let is_space = |t: &Tok| matches!(t.kind, TK::Space | TK::Comment);
    let start = toks.iter().position(|t| !is_space(t)).unwrap_or(toks.len());
    let end = toks.iter().rposition(|t| !is_space(t)).map_or(start, |e| e + 1);
    let text = |ts: &[Tok]| {
        ts.iter()
            .filter(|t| t.kind == TK::Space)
            .map(|t| t.text.as_str())
            .collect()
    };
    (text(&toks[..start]), &toks[start..end], text(&toks[end..]))
}

/// Source text with whitespace collapsed and the inside of every content
/// block elided: code expressions with the same skeleton differ only in
/// markup.
fn skeleton(nodes: &[SyntaxNode]) -> String {
    fn into(n: &SyntaxNode, s: &mut String) {
        match n.kind() {
            K::ContentBlock => s.push_str("[]"),
            _ if n.children().len() == 0 => s.push_str(n.leaf_text()),
            _ => n.children().for_each(|c| into(c, s)),
        }
    }
    let mut s = String::new();
    nodes.iter().for_each(|n| into(n, &mut s));
    collapse_ws(&s)
}

/// The words of tokens for [`similarity`]; code counts with the words of
/// its source.
fn words(toks: &[Tok]) -> Vec<String> {
    toks.iter()
        .flat_map(|t| match t.kind {
            TK::Space => vec![],
            TK::Atom => split_words(&t.text)
                .into_iter()
                .filter(|w| !w.trim().is_empty())
                .map(str::to_string)
                .collect(),
            _ => vec![t.key.trim_matches(|c: char| !c.is_alphanumeric()).to_string()],
        })
        .collect()
}

/// Fraction of shared words.
fn similarity(ka: &[String], kb: &[String]) -> f64 {
    if ka.is_empty() && kb.is_empty() {
        return 1.0;
    }
    let same: usize = capture_diff_slices(Algorithm::Myers, ka, kb)
        .iter()
        .filter_map(|op| match op {
            DiffOp::Equal { len, .. } => Some(*len),
            _ => None,
        })
        .sum();
    2.0 * same as f64 / (ka.len() + kb.len()) as f64
}
