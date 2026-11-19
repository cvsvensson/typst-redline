//! What is compared: a file as a sequence of blocks (paragraphs, headings,
//! items, setup code), and a block body as a sequence of tokens.

use std::collections::HashSet;

use similar::{Algorithm, DiffOp, capture_diff_slices};
use typst_syntax::{Span, SyntaxKind as K, SyntaxNode, ast};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Kind {
    Setup,
    Heading,
    Item,
    #[default]
    Para,
}

#[derive(Clone)]
pub struct Block {
    pub kind: Kind,
    /// Whitespace between the previous block and this one.
    pub gap: String,
    pub nodes: Vec<SyntaxNode>,
    pub key: String,
}

impl Block {
    pub fn text(&self) -> String {
        text(&self.nodes)
    }

    /// Text before the body markup (`= `, `- `, `/ Term: `), the body nodes
    /// and the trailing text (label) of a heading or item; for a paragraph,
    /// all nodes.
    pub fn parts(&self) -> (String, Vec<SyntaxNode>, String) {
        let head = &self.nodes[0];
        let body = matches!(self.kind, Kind::Heading | Kind::Item)
            .then(|| head.children().rposition(|c| c.kind() == K::Markup))
            .flatten();
        let Some(i) = body else {
            return (String::new(), self.nodes.clone(), String::new());
        };
        let children: Vec<SyntaxNode> = head.children().cloned().collect();
        (
            text(&children[..i]),
            children[i].children().cloned().collect(),
            text(&self.nodes[1..]),
        )
    }

    pub fn toks(&self) -> Vec<Tok> {
        tokenize(&self.parts().1)
    }

    /// The text before the body of a term item split around its term: the
    /// marker, the term's markup and the colon.
    pub fn term(&self) -> Option<(String, Vec<SyntaxNode>, String)> {
        let head = self.nodes.first().filter(|n| n.kind() == K::TermItem)?;
        let children: Vec<SyntaxNode> = head.children().cloned().collect();
        let i = children.iter().position(|c| c.kind() == K::Markup)?;
        let j = children
            .iter()
            .rposition(|c| c.kind() == K::Markup)
            .filter(|&j| j > i)?;
        Some((
            text(&children[..i]),
            children[i].children().cloned().collect(),
            text(&children[i + 1..j]),
        ))
    }

    /// The marker before the body; blocks that differ in it are not
    /// compared word by word, so that the change is marked. The term of a
    /// term item is compared as text, and a label, which does not show, is
    /// compared apart.
    pub fn frame(&self) -> String {
        let (prefix, _, _) = self.parts();
        collapse_ws(&self.term().map_or(prefix, |(marker, _, colon)| marker + &colon))
    }

    pub fn label(&self) -> Option<String> {
        self.nodes.iter().find(|n| n.kind() == K::Label).map(label_name)
    }

    pub fn heading_level(&self) -> usize {
        self.nodes[0]
            .children()
            .find(|c| c.kind() == K::HeadingMarker)
            .map_or(1, |m| m.len())
    }

    /// The literal path of an `#include` block.
    pub fn include_target(&self) -> Option<String> {
        match self.nodes.get(1)?.cast::<ast::ModuleInclude>()?.source() {
            ast::Expr::Str(s) => Some(s.get().to_string()),
            _ => None,
        }
    }

    /// A `#pagebreak()` or `#colbreak()` block.
    pub fn is_break(&self) -> bool {
        self.nodes.get(1).is_some_and(|n| {
            n.kind() == K::FuncCall
                && n.children()
                    .next()
                    .is_some_and(|f| matches!(f.leaf_text().as_str(), "pagebreak" | "colbreak"))
        })
    }

    /// An `#import` or `#include` block: its changes show in the files it
    /// names, not in the line itself.
    pub fn is_import(&self) -> bool {
        self.nodes
            .get(1)
            .is_some_and(|n| matches!(n.kind(), K::ModuleImport | K::ModuleInclude))
    }
}

#[derive(Clone)]
pub struct Doc {
    pub blocks: Vec<Block>,
    pub trailing: String,
}

#[derive(Default)]
struct DocBuilder {
    blocks: Vec<Block>,
    /// The block being built, of kind `kind`.
    cur: Vec<SyntaxNode>,
    kind: Kind,
    /// Whitespace after `cur` that goes into it if the block continues.
    ws: Vec<SyntaxNode>,
    gap: String,
}

impl DocBuilder {
    fn flush(&mut self) {
        if self.cur.is_empty() {
            return;
        }
        let nodes = std::mem::take(&mut self.cur);
        let key = format!("{:?}:{}", self.kind, spaced(&nodes));
        self.blocks.push(Block {
            kind: self.kind,
            gap: std::mem::take(&mut self.gap),
            nodes,
            key,
        });
        self.gap = text(&std::mem::take(&mut self.ws));
    }
}

impl Doc {
    pub fn new(children: &[SyntaxNode]) -> Self {
        let mut b = DocBuilder::default();
        let mut i = 0;
        while i < children.len() {
            let n = &children[i];
            match n.kind() {
                K::Parbreak => {
                    b.flush();
                    b.gap.push_str(&n.full_text());
                }
                K::Space | K::LineComment | K::BlockComment => {
                    if b.cur.is_empty() {
                        b.gap.push_str(&n.full_text());
                    } else if b.kind != Kind::Para && n.full_text().contains('\n') {
                        b.flush();
                        b.gap.push_str(&n.full_text());
                    } else {
                        b.ws.push(n.clone());
                    }
                }
                K::Label if !b.cur.is_empty() => {
                    b.cur.append(&mut b.ws);
                    b.cur.push(n.clone());
                }
                _ => {
                    let unit = unit(children, &mut i);
                    let kind = match n.kind() {
                        K::Heading => Kind::Heading,
                        k if is_item(k) => Kind::Item,
                        _ if unit.get(1).is_some_and(is_setup) => Kind::Setup,
                        _ => Kind::Para,
                    };
                    if kind != Kind::Para || b.kind != Kind::Para {
                        b.flush();
                    }
                    b.kind = kind;
                    b.cur.append(&mut b.ws);
                    b.cur.extend(unit);
                }
            }
            i += 1;
        }
        b.flush();
        Doc {
            blocks: b.blocks,
            trailing: b.gap,
        }
    }

    pub fn keys(&self) -> Vec<&str> {
        self.blocks.iter().map(|b| b.key.as_str()).collect()
    }
}

/// The node at `*i`, or a `#` with the expression after it and a semicolon
/// ending it; `*i` is left at its last node.
fn unit(nodes: &[SyntaxNode], i: &mut usize) -> Vec<SyntaxNode> {
    let mut unit = vec![nodes[*i].clone()];
    if nodes[*i].kind() == K::Hash && *i + 1 < nodes.len() {
        *i += 1;
        unit.push(nodes[*i].clone());
        if nodes.get(*i + 1).is_some_and(|c| c.kind() == K::Semicolon) {
            *i += 1;
            unit.push(nodes[*i].clone());
        }
    }
    unit
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TK {
    Word,
    Space,
    Atom,
    Comment,
}

#[derive(Clone, Debug)]
pub struct Tok {
    pub kind: TK,
    pub key: String,
    pub text: String,
    pub nodes: Vec<SyntaxNode>,
    /// Display equation, raw block or code alone on its line: laid out as a
    /// block.
    pub block: bool,
    /// Where its source starts: a node and an offset into it.
    pub at: (Span, usize),
}

impl Tok {
    fn plain(kind: TK, key: &str, text: String, at: (Span, usize)) -> Self {
        Tok {
            kind,
            key: key.into(),
            text,
            nodes: vec![],
            block: false,
            at,
        }
    }

    /// The length of its source.
    pub fn source_len(&self) -> usize {
        if self.kind == TK::Word {
            self.key.len()
        } else {
            self.text.len()
        }
    }
}

pub fn tokenize(nodes: &[SyntaxNode]) -> Vec<Tok> {
    let mut toks: Vec<Tok> = vec![];
    let mut i = 0;
    while i < nodes.len() {
        let n = &nodes[i];
        match n.kind() {
            K::Text => {
                let text = n.leaf_text();
                for piece in split_words(text) {
                    let at = (n.span(), piece.as_ptr() as usize - text.as_ptr() as usize);
                    toks.push(if piece.chars().all(char::is_whitespace) {
                        Tok::plain(TK::Space, " ", piece.into(), at)
                    } else {
                        // Literal inside a word, but markup next to a wrapper.
                        Tok::plain(TK::Word, piece, piece.replace('_', "\\_").replace('*', "\\*"), at)
                    });
                }
            }
            K::Space => toks.push(Tok::plain(TK::Space, " ", n.full_text().into(), (n.span(), 0))),
            K::Parbreak => toks.push(Tok::plain(TK::Space, "\n\n", n.full_text().into(), (n.span(), 0))),
            K::LineComment | K::BlockComment => {
                toks.push(Tok::plain(TK::Comment, "//", n.full_text().into(), (n.span(), 0)))
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
                let line_start = toks
                    .iter()
                    .nth_back(1)
                    .is_none_or(|t| t.kind == TK::Space && t.text.contains('\n'));
                let line_end = nodes.get(i + 1).is_none_or(|c| c.full_text().starts_with('\n'));
                let atom = toks.last_mut().unwrap();
                for s in spaces.into_iter().rev() {
                    atom.text.push_str(&s.text);
                    atom.nodes.push(SyntaxNode::leaf(K::Space, s.text));
                }
                atom.text.push_str(&n.full_text());
                atom.nodes.push(n.clone());
                atom.key = skeleton(&atom.nodes);
                // Code alone on its line but for its label.
                atom.block |= atom.nodes[0].kind() == K::Hash && line_start && line_end;
            }
            // Typst lays out citations separated by spaces as one group, in
            // the style of the first, so they are compared as one token.
            K::Ref if toks.len() >= 2 && toks[toks.len() - 1].key == " " && is_refs(&toks[toks.len() - 2]) => {
                let space = toks.pop().unwrap();
                let refs = toks.last_mut().unwrap();
                refs.text.push_str(&space.text);
                refs.text.push_str(&n.full_text());
                refs.nodes.push(SyntaxNode::leaf(K::Space, space.text));
                refs.nodes.push(n.clone());
                refs.key = skeleton(&refs.nodes);
            }
            _ => {
                let unit = unit(nodes, &mut i);
                let line_start = toks.last().is_none_or(|t| t.kind == TK::Space && t.text.contains('\n'));
                let line_end = nodes.get(i + 1).is_none_or(|c| c.full_text().starts_with('\n'));
                let display = n.kind() == K::Equation && n.children().nth(1).is_some_and(|c| c.kind() == K::Space)
                    || n.cast::<ast::Raw>().is_some_and(|r| r.block());
                toks.push(Tok {
                    kind: TK::Atom,
                    key: skeleton(&unit),
                    text: text(&unit),
                    block: display || (n.kind() == K::Hash && line_start && line_end),
                    at: (n.span(), 0),
                    nodes: unit,
                });
            }
        }
        i += 1;
    }
    toks
}

fn is_refs(t: &Tok) -> bool {
    t.kind == TK::Atom && t.nodes.iter().all(|n| matches!(n.kind(), K::Ref | K::Space))
}

/// Chinese and Japanese characters and punctuation, which Typst spaces or
/// compresses against the characters next to them.
pub fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0x20000..=0x3FFFF)
}

/// Split text into runs of word characters, runs of whitespace, and single
/// other characters. The unit is the grapheme cluster: a mark between a
/// letter and its combining marks would draw them apart.
pub fn split_words(s: &str) -> Vec<&str> {
    #[derive(PartialEq, Clone, Copy)]
    enum Class {
        Word,
        Space,
        Single,
        // Runs of CJK punctuation, which Typst compresses as a whole.
        Punct,
    }
    // Scripts written without spaces between words: each character is a word.
    let unspaced = |c: char| {
        matches!(c as u32, 0x0E00..=0x0EFF | 0x1000..=0x109F | 0x1780..=0x17FF | 0x2E80..=0x9FFF
            | 0xF900..=0xFAFF | 0xFF66..=0xFF9F | 0x20000..=0x3FFFF)
    };
    let clusters: Vec<(usize, &str)> = s.grapheme_indices(true).collect();
    let first = |j: usize| clusters[j].1.chars().next().unwrap();
    let mut out = vec![];
    let mut start = 0;
    let mut prev = None;
    for (j, &(i, _)) in clusters.iter().enumerate() {
        let c = first(j);
        // Numbers with a decimal point and hyphenated words are one word.
        let near = |f: &dyn Fn(char) -> bool| {
            j.checked_sub(1).is_some_and(|p| f(first(p))) && j + 1 < clusters.len() && f(first(j + 1))
        };
        let joins = match c {
            '.' | ',' => near(&|c| c.is_ascii_digit()),
            '-' => near(&|c| c.is_alphanumeric() && !unspaced(c)),
            _ => false,
        };
        let class = if joins {
            Class::Word
        } else if matches!(c as u32, 0x3001..=0x303F | 0x30FB | 0xFF00..=0xFF65) && !c.is_alphanumeric() {
            Class::Punct
        } else if unspaced(c) {
            Class::Single
        } else if c.is_alphanumeric() {
            Class::Word
        } else if c.is_whitespace() {
            Class::Space
        } else {
            Class::Single
        };
        if prev.is_some_and(|p| p != class || class == Class::Single) {
            out.push(&s[start..i]);
            start = i;
        }
        prev = Some(class);
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

pub fn is_item(kind: K) -> bool {
    matches!(kind, K::ListItem | K::EnumItem | K::TermItem)
}

pub fn text(nodes: &[SyntaxNode]) -> String {
    nodes.iter().map(|n| n.full_text().to_string()).collect()
}

/// The source of `nodes` with each space node as one space. Paragraph
/// breaks and the whitespace inside raw text and strings are kept: they show.
fn spaced(nodes: &[SyntaxNode]) -> String {
    fn into(n: &SyntaxNode, s: &mut String) {
        match n.kind() {
            K::Space => s.push(' '),
            _ if n.children().len() == 0 => s.push_str(n.leaf_text()),
            _ => n.children().for_each(|c| into(c, s)),
        }
    }
    let mut s = String::new();
    nodes.iter().for_each(|n| into(n, &mut s));
    s
}

pub fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn label_name(n: &SyntaxNode) -> String {
    let t = n.leaf_text();
    t.trim_start_matches('<').trim_end_matches('>').to_string()
}

pub fn ref_name(n: &SyntaxNode) -> String {
    n.children()
        .next()
        .map_or(String::new(), |m| m.leaf_text().trim_start_matches('@').to_string())
}

/// Labels and citation keys that a reference in the output may use: the new
/// version's labels and everything the new version references.
pub fn targets(root: &SyntaxNode) -> HashSet<String> {
    let mut set = HashSet::new();
    walk(root, &mut |n| match n.kind() {
        K::Label => drop(set.insert(label_name(n))),
        K::Ref => drop(set.insert(ref_name(n))),
        _ => {}
    });
    set
}

/// Names bound at the top level of a file.
pub fn definitions(root: &SyntaxNode) -> HashSet<String> {
    top_bindings(root).into_iter().filter_map(|(_, b)| b).collect()
}

/// The names that node `n` itself binds, `None` for a wildcard import, which
/// can bind any name.
pub fn binds(n: &SyntaxNode) -> Vec<Option<String>> {
    let names = |ids: Vec<ast::Ident>| ids.into_iter().map(|i| Some(i.get().to_string())).collect();
    if let Some(b) = n.cast::<ast::LetBinding>() {
        names(b.kind().bindings())
    } else if let Some(f) = n.cast::<ast::ForLoop>() {
        names(f.pattern().bindings())
    } else if let Some(p) = n.cast::<ast::Params>() {
        names(
            p.children()
                .flat_map(|p| match p {
                    ast::Param::Pos(p) => p.bindings(),
                    ast::Param::Named(n) => vec![n.name()],
                    ast::Param::Spread(s) => s.sink_ident().into_iter().collect(),
                })
                .collect(),
        )
    } else if let Some(i) = n.cast::<ast::ModuleImport>() {
        let mut out: Vec<Option<String>> = match i.imports() {
            Some(ast::Imports::Wildcard) => vec![None],
            Some(ast::Imports::Items(items)) => names(items.iter().map(|i| i.bound_name()).collect()),
            None => vec![],
        };
        match i.new_name() {
            Some(name) => out.push(Some(name.get().to_string())),
            None if i.imports().is_none() => out.extend(i.bare_name().ok().map(|n| Some(n.to_string()))),
            None => {}
        }
        out
    } else {
        vec![]
    }
}

/// Whether names bound inside a node of this kind are local to it.
pub fn is_scope(kind: K) -> bool {
    matches!(kind, K::CodeBlock | K::ContentBlock | K::Closure | K::ForLoop)
}

/// The names bound at the top level of a file, outside any block, function
/// or loop, each with the offset of what binds it.
pub fn top_bindings(root: &SyntaxNode) -> Vec<(usize, Option<String>)> {
    fn into(n: &SyntaxNode, at: usize, out: &mut Vec<(usize, Option<String>)>) {
        if is_scope(n.kind()) {
            return;
        }
        out.extend(binds(n).into_iter().map(|b| (at, b)));
        let mut at = at;
        for c in n.children() {
            into(c, at, out);
            at += c.len();
        }
    }
    let mut out = vec![];
    into(root, 0, &mut out);
    out
}

/// The names bound anywhere in `n` but in the nodes with a span in `skip`.
pub fn all_bindings(n: &SyntaxNode, skip: &HashSet<Span>) -> Vec<Option<String>> {
    let mut out = vec![];
    let mut stack = vec![n];
    while let Some(n) = stack.pop() {
        if !skip.contains(&n.span()) {
            out.extend(binds(n));
            stack.extend(n.children());
        }
    }
    out
}

/// The names that code uses as variables and does not bind itself.
pub fn free_names(nodes: &[SyntaxNode]) -> HashSet<String> {
    fn into(n: &SyntaxNode, used: &mut HashSet<String>, bound: &mut HashSet<String>) {
        bound.extend(binds(n).into_iter().flatten());
        // A field and the name of a named argument are not variables.
        let (skip, take) = match n.kind() {
            K::Ident | K::MathIdent => {
                used.insert(n.leaf_text().to_string());
                return;
            }
            K::FieldAccess => (0, 1),
            K::Named => (1, usize::MAX),
            _ => (0, usize::MAX),
        };
        n.children().skip(skip).take(take).for_each(|c| into(c, used, bound));
    }
    let (mut used, mut bound) = (HashSet::new(), HashSet::new());
    nodes.iter().for_each(|n| into(n, &mut used, &mut bound));
    &used - &bound
}

/// The strings that show rules match in text, as `"new version"` in
/// `#show "new version": strong`.
pub fn shown_strings(root: &SyntaxNode) -> HashSet<String> {
    let mut set = HashSet::new();
    walk(root, &mut |n| {
        if let Some(rule) = n.cast::<ast::ShowRule>()
            && let Some(ast::Expr::Str(s)) = rule.selector()
            && !s.get().is_empty()
        {
            set.insert(s.get().to_string());
        }
    });
    set
}

pub fn walk(n: &SyntaxNode, f: &mut impl FnMut(&SyntaxNode)) {
    f(n);
    for c in n.children() {
        walk(c, f);
    }
}

/// Source text with whitespace collapsed, labels and comments dropped and
/// all markup elided: expressions with the same skeleton differ only in
/// markup (of content blocks, emphasis) or in what does not show.
fn skeleton(nodes: &[SyntaxNode]) -> String {
    fn into(n: &SyntaxNode, s: &mut String) {
        match n.kind() {
            K::Markup => s.push('…'),
            K::Label | K::LineComment | K::BlockComment => {}
            _ if n.children().len() == 0 => s.push_str(n.leaf_text()),
            _ => n.children().for_each(|c| into(c, s)),
        }
    }
    let mut s = String::new();
    nodes.iter().for_each(|n| into(n, &mut s));
    collapse_ws(&s)
}

/// The nodes of an item body before its first nested item, and the rest
/// starting at the whitespace before that item.
pub fn split_nested(body: &[SyntaxNode]) -> (&[SyntaxNode], &[SyntaxNode]) {
    let at = body.iter().position(|n| is_item(n.kind())).map_or(body.len(), |i| {
        if i > 0 && matches!(body[i - 1].kind(), K::Space | K::Parbreak) {
            i - 1
        } else {
            i
        }
    });
    body.split_at(at)
}

/// The markup inside a content block.
pub fn markup(block: &SyntaxNode) -> Vec<SyntaxNode> {
    block
        .children()
        .find(|c| c.kind() == K::Markup)
        .map_or(vec![], |m| m.children().cloned().collect())
}

/// Leading whitespace, the tokens between, and trailing whitespace.
pub fn trim_spaces(toks: &[Tok]) -> (String, &[Tok], String) {
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

/// The words of tokens for [`similarity`]; code counts with the words of
/// its source.
pub fn words(toks: &[Tok]) -> Vec<String> {
    toks.iter()
        .flat_map(|t| match t.kind {
            TK::Space => vec![],
            TK::Atom => split_words(&t.text)
                .into_iter()
                .filter(|w| !w.trim().is_empty())
                .map(str::to_string)
                .collect(),
            _ => vec![t.key.clone()],
        })
        .collect()
}

/// Fraction of shared words.
pub fn similarity(ka: &[String], kb: &[String]) -> f64 {
    if ka.is_empty() && kb.is_empty() {
        return 1.0;
    }
    2.0 * shared(ka, kb) as f64 / (ka.len() + kb.len()) as f64
}

/// The number of words that two lists share in order.
pub fn shared(ka: &[String], kb: &[String]) -> usize {
    capture_diff_slices(Algorithm::Myers, ka, kb)
        .iter()
        .filter_map(|op| match op {
            DiffOp::Equal { len, .. } => Some(*len),
            _ => None,
        })
        .sum()
}

/// The most similar of the candidates scored by [`similarity`], if it is
/// similar enough to be a changed version rather than a replacement.
pub fn most_similar<T>(scored: impl Iterator<Item = (T, f64)>) -> Option<T> {
    scored
        .filter(|&(_, s)| s >= 0.4)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(t, _)| t)
}
