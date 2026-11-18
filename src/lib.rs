//! Produce a Typst document that shows the new version of a file with the
//! changes against an old version marked.

use std::collections::HashSet;

use similar::{Algorithm, DiffOp, capture_diff_slices};
use typst_syntax::{SyntaxKind as K, SyntaxNode, parse};

pub const PREAMBLE: &str = include_str!("preamble.typ");

/// Strip a byte-order mark and convert CRLF and CR line endings to LF.
pub fn normalize(s: &str) -> String {
    s.strip_prefix('\u{feff}')
        .unwrap_or(s)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

/// Typst source of `new` with the changes against `old` marked, preceded by
/// `preamble` and a summary page.
pub fn redline(old: &str, new: &str, preamble: &str) -> String {
    let (old, new) = (normalize(old), normalize(new));
    let (old_root, new_root) = (parse(&old), parse(&new));
    let old_doc = Doc::new(&old_root);
    let new_doc = Doc::new(&new_root);
    let ctx = Ctx {
        new_targets: targets(&new_root),
        removed_defs: definitions(&old_root)
            .difference(&definitions(&new_root))
            .cloned()
            .collect(),
    };
    let mut w = Writer::new(&ctx);
    let ops = capture_diff_slices(Algorithm::Patience, &old_doc.keys(), &new_doc.keys());
    for op in ops {
        let (o, n) = (op.old_range(), op.new_range());
        match op {
            DiffOp::Equal { .. } => {
                for (i, j) in o.zip(n) {
                    w.pair(&old_doc.blocks[i], &new_doc.blocks[j], true);
                }
            }
            _ => w.region(&old_doc.blocks[o], &new_doc.blocks[n]),
        }
    }
    w.out.push_str(&new_doc.trailing);
    w.finish(preamble)
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

    /// Text before the body markup (`= `, `- `), the body tokens and the
    /// trailing text (label) of a heading or item; for a paragraph, all tokens.
    fn parts(&self) -> (String, Vec<Tok>, String) {
        let Some(head) = self.head() else {
            return (String::new(), tokenize(&self.nodes), String::new());
        };
        let mut prefix = String::new();
        let mut body = None;
        for c in head.children() {
            if c.kind() == K::Markup && body.is_none() {
                body = Some(c);
            } else if body.is_none() {
                prefix.push_str(&c.full_text());
            }
        }
        let suffix: String = self.nodes[1..].iter().map(|n| n.full_text().to_string()).collect();
        match body {
            Some(b) => (prefix, tokenize(&b.children().cloned().collect::<Vec<_>>()), suffix),
            None => (String::new(), tokenize(&self.nodes), String::new()),
        }
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
    fn new(root: &SyntaxNode) -> Self {
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
        let children: Vec<SyntaxNode> = root.children().cloned().collect();
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
                        K::ListItem | K::EnumItem => k = Kind::Item,
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
                        text: piece.to_string(),
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
                atom.key = collapse_ws(&atom.text);
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
                    key: collapse_ws(&text),
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

/// Split text into runs of word characters, runs of whitespace, and single
/// other characters.
fn split_words(s: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut start = 0;
    let class = |c: char| {
        if c.is_alphanumeric() {
            0
        } else if c.is_whitespace() {
            1
        } else {
            2
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
                    "pagebreak" | "colbreak" | "bibliography" | "outline" | "v"
                )
        }),
        _ => false,
    }
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
    new_targets: HashSet<String>,
    removed_defs: HashSet<String>,
}

impl Ctx {
    /// Source for deleted content: labels removed, references to unknown
    /// targets turned into text. Returns `None` if the code cannot be kept live
    /// (it uses removed definitions or deleted labels).
    fn sanitize(&self, nodes: &[SyntaxNode]) -> Option<String> {
        let mut out = String::new();
        for n in nodes {
            self.sanitize_into(n, true, &mut out)?;
        }
        Some(out)
    }

    fn sanitize_into(&self, n: &SyntaxNode, markup: bool, out: &mut String) -> Option<()> {
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
            K::Ident | K::MathIdent if self.removed_defs.contains(n.leaf_text().as_str()) => return None,
            _ if n.children().len() == 0 => out.push_str(n.leaf_text()),
            _ => {
                let m = n.kind() == K::Markup;
                for c in n.children() {
                    self.sanitize_into(c, m, out)?;
                }
            }
        }
        Some(())
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Ins,
    Del,
}

impl Side {
    fn name(self) -> &'static str {
        match self {
            Side::Ins => "ins",
            Side::Del => "del",
        }
    }
}

struct Section {
    level: usize,
    target: Option<String>,
    title: String,
    count: usize,
    deleted: bool,
}

struct Writer<'a> {
    ctx: &'a Ctx,
    out: String,
    summary_at: Option<usize>,
    sections: Vec<Section>,
    current: Option<usize>,
    front: usize,
    n_ins: usize,
    n_del: usize,
}

impl<'a> Writer<'a> {
    fn new(ctx: &'a Ctx) -> Self {
        Writer {
            ctx,
            out: String::new(),
            summary_at: None,
            sections: vec![],
            current: None,
            front: 0,
            n_ins: 0,
            n_del: 0,
        }
    }

    fn count(&mut self, side: Side) {
        match side {
            Side::Ins => self.n_ins += 1,
            Side::Del => self.n_del += 1,
        }
        match self.current {
            Some(s) => self.sections[s].count += 1,
            None => self.front += 1,
        }
    }

    fn finish(self, preamble: &str) -> String {
        let mut entries = String::new();
        if self.front > 0 {
            entries.push_str(&format!(
                "  (1, none, [Before the first section], {}, false),\n",
                self.front
            ));
        }
        for s in self.sections.iter().filter(|s| s.count > 0 || s.deleted) {
            let target = s.target.as_ref().map_or("none".to_string(), |l| format!("<{l}>"));
            entries.push_str(&format!(
                "  ({}, {target}, [{}], {}, {}),\n",
                s.level,
                s.title.trim(),
                s.count,
                s.deleted
            ));
        }
        let summary = format!("#redline-summary({}, {}, (\n{entries}))\n", self.n_ins, self.n_del);
        let at = self.summary_at.unwrap_or(self.out.len());
        let mut out = self.out;
        out.insert_str(at, &summary);
        format!("{preamble}\n{out}")
    }

    /// Emit a new block; the summary goes before the first non-setup block.
    fn begin_new(&mut self, b: &Block) {
        if b.kind != Kind::Setup && self.summary_at.is_none() {
            let gap = b.gap.trim_end_matches(|c: char| c != '\n');
            self.out.push_str(gap);
            self.summary_at = Some(self.out.len());
            self.out.push_str(&b.gap[gap.len()..]);
        } else {
            self.out.push_str(&b.gap);
        }
    }

    /// A run of differing blocks. Similar blocks of the same kind are paired
    /// and diffed word by word; around each pair, deletions come first.
    fn region(&mut self, old: &[Block], new: &[Block]) {
        let mut p = 0;
        let mut pending: Vec<&Block> = vec![];
        for nb in new {
            let best = (nb.kind != Kind::Setup)
                .then(|| {
                    let ntoks = nb.parts().1;
                    (p..old.len())
                        .filter(|&q| {
                            old[q].kind == nb.kind
                                && (nb.kind != Kind::Heading || old[q].heading_level() == nb.heading_level())
                        })
                        .map(|q| (q, similarity(&old[q].parts().1, &ntoks)))
                        .filter(|&(_, s)| s >= 0.4)
                        .max_by(|a, b| a.1.total_cmp(&b.1))
                })
                .flatten();
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

    /// Deleted then inserted blocks, but text before the first heading on
    /// either side stays before the headings.
    fn unpaired(&mut self, old: &[Block], new: &[&Block]) {
        let oh = old.iter().position(|b| b.kind == Kind::Heading).unwrap_or(old.len());
        let nh = new.iter().position(|b| b.kind == Kind::Heading).unwrap_or(new.len());
        old[..oh].iter().for_each(|b| self.deleted(b));
        new[..nh].iter().for_each(|b| self.inserted(b));
        old[oh..].iter().for_each(|b| self.deleted(b));
        new[nh..].iter().for_each(|b| self.inserted(b));
    }

    fn heading_start(&mut self, nb: &Block) -> String {
        let level = nb.heading_level();
        if level > 2 {
            return String::new();
        }
        let (target, extra) = match nb.label() {
            Some(l) => (l, String::new()),
            None => {
                let l = format!("redline-sec-{}", self.sections.len());
                (l.clone(), format!(" <{l}>"))
            }
        };
        let (_, toks, _) = nb.parts();
        let title = toks
            .iter()
            .filter(|t| t.kind != TK::Comment)
            .map(|t| t.text.as_str())
            .collect();
        self.sections.push(Section {
            level,
            target: Some(target),
            title,
            count: 0,
            deleted: false,
        });
        self.current = Some(self.sections.len() - 1);
        extra
    }

    fn pair(&mut self, ob: &Block, nb: &Block, equal: bool) {
        self.begin_new(nb);
        let extra = if nb.kind == Kind::Heading {
            self.heading_start(nb)
        } else {
            String::new()
        };
        if equal || nb.kind == Kind::Setup {
            self.out.push_str(&nb.text());
            self.out.push_str(&extra);
            return;
        }
        let (prefix, ntoks, suffix) = nb.parts();
        let (_, otoks, _) = ob.parts();
        self.out.push_str(&prefix);
        self.diff_tokens(&otoks, &ntoks);
        self.out.push_str(&suffix);
        self.out.push_str(&extra);
    }

    fn inserted(&mut self, nb: &Block) {
        self.begin_new(nb);
        if nb.kind == Kind::Setup {
            self.out.push_str(&nb.text());
            return;
        }
        let extra = if nb.kind == Kind::Heading {
            self.heading_start(nb)
        } else {
            String::new()
        };
        let (prefix, toks, suffix) = nb.parts();
        self.count(Side::Ins);
        self.out.push_str(&prefix);
        let body = self.content(&toks, Side::Ins);
        match nb.kind {
            Kind::Heading => self.out.push_str(&format!("#redline-ins(start: true)[{body}];")),
            _ => self.out.push_str(&format!("#redline-ins-block[{body}];")),
        }
        self.out.push_str(&suffix);
        self.out.push_str(&extra);
    }

    fn deleted(&mut self, ob: &Block) {
        if ob.kind == Kind::Setup {
            return;
        }
        let (prefix, toks, _) = ob.parts();
        let body = self.content(&toks, Side::Del);
        let text = match ob.kind {
            Kind::Heading => {
                let level = ob.heading_level();
                if level <= 2 {
                    self.sections.push(Section {
                        level,
                        target: None,
                        title: body.clone(),
                        count: 0,
                        deleted: true,
                    });
                }
                format!("\n\n#redline-del-heading({level})[{body}];")
            }
            Kind::Item => format!("\n{prefix}#redline-del-block[{body}];"),
            _ => format!("\n\n#redline-del-block[{body}];"),
        };
        self.count(Side::Del);
        self.out.push_str(&text);
        self.out.push('\n');
    }

    /// Markup for tokens placed inside a wrapper's content block.
    fn content(&self, toks: &[Tok], side: Side) -> String {
        let mut s = String::new();
        for t in toks {
            match t.kind {
                TK::Comment => {}
                TK::Space => s.push_str(&t.text),
                TK::Word => {
                    if s.trim().is_empty() && matches!(t.text.as_str(), "-" | "+" | "=" | "/") {
                        s.push('\\');
                    }
                    s.push_str(&t.text.replace('[', "\\[").replace(']', "\\]"));
                }
                TK::Atom => match side {
                    Side::Ins => s.push_str(&t.text),
                    Side::Del => match self.ctx.sanitize(&t.nodes) {
                        Some(src) => s.push_str(&src),
                        None => s.push_str(&format!("#raw({:?});", collapse_ws(&t.text))),
                    },
                },
            }
        }
        s
    }

    fn diff_tokens(&mut self, old: &[Tok], new: &[Tok]) {
        let ok: Vec<&str> = old.iter().map(|t| t.key.as_str()).collect();
        let nk: Vec<&str> = new.iter().map(|t| t.key.as_str()).collect();
        // Segments: (old range, new range, equal).
        let mut segs: Vec<(std::ops::Range<usize>, std::ops::Range<usize>, bool)> = vec![];
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
        // Absorb equal runs without words between two changes.
        let mut merged: Vec<(std::ops::Range<usize>, std::ops::Range<usize>, bool)> = vec![];
        let mut i = 0;
        while i < segs.len() {
            let s = segs[i].clone();
            // Punctuation and at most one short word: merging gives
            // readable chunks instead of alternating fragments.
            let trivial = |r: &std::ops::Range<usize>| {
                let toks = &new[r.clone()];
                let words: Vec<&Tok> = toks
                    .iter()
                    .filter(|t| t.kind == TK::Word && t.text.chars().any(char::is_alphanumeric))
                    .collect();
                toks.iter().all(|t| t.kind != TK::Atom)
                    && (words.is_empty() || (words.len() == 1 && words[0].text.chars().count() <= 3))
            };
            if s.2 && i > 0 && i + 1 < segs.len() && trivial(&s.1) {
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
                for t in &new[n] {
                    self.out.push_str(&t.text);
                }
            } else {
                self.change(&old[o], &new[n]);
            }
        }
    }

    /// A changed run: old struck, new highlighted, whitespace outside the
    /// wrappers.
    fn change(&mut self, old: &[Tok], new: &[Tok]) {
        let (olead, ocore, otrail) = trim_spaces(old);
        let (nlead, ncore, ntrail) = trim_spaces(new);
        let (lead, trail) = if ncore.is_empty() {
            (olead, otrail)
        } else {
            (nlead, ntrail)
        };
        self.out.push_str(&lead);
        if !ocore.is_empty() {
            self.count(Side::Del);
            let w = format!(
                "{}{}",
                self.wrapper(ocore, Side::Del),
                bracket(&self.content(ocore, Side::Del))
            );
            self.out.push_str(&w);
            if !ncore.is_empty() {
                self.out.push(' ');
            }
        }
        if !ncore.is_empty() {
            self.count(Side::Ins);
            let w = format!(
                "{}{}",
                self.wrapper(ncore, Side::Ins),
                bracket(&self.content(ncore, Side::Ins))
            );
            self.out.push_str(&w);
        }
        self.out.push_str(&trail);
    }

    fn wrapper(&self, mid: &[Tok], side: Side) -> String {
        let start = !mid[0].block;
        let end = !mid[mid.len() - 1].block && mid.iter().any(|t| t.text.contains('\n'));
        format!("#redline-{}(start: {start}, end: {end})", side.name())
    }
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

fn bracket(body: &str) -> String {
    format!("[{body}];")
}

fn similarity(a: &[Tok], b: &[Tok]) -> f64 {
    let ka: Vec<&str> = a
        .iter()
        .filter(|t| t.kind != TK::Space)
        .map(|t| t.key.as_str())
        .collect();
    let kb: Vec<&str> = b
        .iter()
        .filter(|t| t.kind != TK::Space)
        .map(|t| t.key.as_str())
        .collect();
    if ka.is_empty() && kb.is_empty() {
        return 1.0;
    }
    let same: usize = capture_diff_slices(Algorithm::Myers, &ka, &kb)
        .iter()
        .filter_map(|op| match op {
            DiffOp::Equal { len, .. } => Some(*len),
            _ => None,
        })
        .sum();
    2.0 * same as f64 / (ka.len() + kb.len()) as f64
}
