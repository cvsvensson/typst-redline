//! Labels whose target changed while the text that refers to them may not
//! have: the summary lists them, since no mark shows at the reference.

use std::collections::{BTreeSet, HashMap, HashSet};

use typst_syntax::{SyntaxKind as K, SyntaxNode, parse};

use crate::mark::typst_str;
use crate::syntax::{collapse_ws, label_name, ref_name, similarity, split_words, walk};

/// A reference that is the same in both versions: to which label, and where.
pub struct Site {
    pub name: String,
    /// Label of the enclosing section, `None` before the first section or in
    /// a file that is not marked.
    pub section: Option<String>,
    pub file: String,
}

/// The labels that `@name` and `#ref(<name>)` in `nodes` refer to, with
/// citation keys among them.
pub fn refs(nodes: &[SyntaxNode]) -> Vec<String> {
    let mut out = vec![];
    for n in nodes {
        walk(n, &mut |c| match c.kind() {
            K::Ref => out.push(ref_name(c)),
            K::FuncCall if c.children().next().is_some_and(|f| f.leaf_text() == "ref") => {
                let label = c
                    .children()
                    .find(|a| a.kind() == K::Args)
                    .and_then(|a| a.children().find(|x| x.kind() == K::Label));
                out.extend(label.map(label_name));
            }
            _ => {}
        });
    }
    out
}

/// What a label is on: the kind of element and its source, without labels
/// and comments.
struct Target {
    kind: String,
    content: String,
}

fn content(n: &SyntaxNode) -> String {
    let mut s = String::new();
    walk(n, &mut |c| {
        if c.children().len() == 0 && !matches!(c.kind(), K::Label | K::LineComment | K::BlockComment) {
            s.push_str(c.leaf_text());
        }
    });
    collapse_ws(&s)
}

fn kind(n: &SyntaxNode) -> String {
    match n.kind() {
        K::Heading => "section".into(),
        K::Equation => "equation".into(),
        K::FuncCall => match n.children().next() {
            Some(f) if f.kind() == K::Ident => f.leaf_text().to_string(),
            _ => "element".into(),
        },
        _ => "element".into(),
    }
}

/// The labelled elements of a version by label (the first if a label is
/// used twice), and the content of every element that is commonly
/// labelled.
fn scan(files: &[(String, String)]) -> (HashMap<String, Target>, HashSet<String>) {
    fn visit(
        n: &SyntaxNode,
        heading: Option<&SyntaxNode>,
        labels: &mut HashMap<String, Target>,
        all: &mut HashSet<String>,
    ) {
        if matches!(n.kind(), K::Heading | K::Equation | K::FuncCall) {
            all.insert(content(n));
        }
        let children: Vec<&SyntaxNode> = n.children().collect();
        for (i, c) in children.iter().enumerate() {
            if c.kind() == K::Label {
                // A label labels the element before it; one ending a heading's
                // body labels the heading.
                let target = heading.or_else(|| {
                    children[..i]
                        .iter()
                        .rev()
                        .find(|x| !matches!(x.kind(), K::Space | K::LineComment | K::BlockComment))
                        .copied()
                });
                if let Some(t) = target {
                    labels.entry(label_name(c)).or_insert(Target {
                        kind: kind(t),
                        content: content(t),
                    });
                }
            }
            let h = (n.kind() == K::Heading && c.kind() == K::Markup).then_some(n);
            visit(c, h, labels, all);
        }
    }
    let (mut labels, mut all) = (HashMap::new(), HashSet::new());
    for (_, src) in files {
        visit(&parse(src), None, &mut labels, &mut all);
    }
    (labels, all)
}

/// The dictionary the preamble's `redline-labels` takes. `place` gives the
/// Typst expression for where a reference is. A label is on a different
/// element if its old element remains elsewhere, its new one was there
/// before, or the two are not alike; otherwise its element changed.
pub fn report(
    old: &[(String, String)],
    new: &[(String, String)],
    sites: &[Site],
    place: impl Fn(&Site) -> String,
) -> String {
    let ((olabels, oall), (nlabels, nall)) = (scan(old), scan(new));
    let words = |s: &str| -> Vec<String> {
        split_words(s)
            .into_iter()
            .filter(|w| !w.trim().is_empty())
            .map(str::to_string)
            .collect()
    };
    let at = |name: &str| {
        let mut seen = BTreeSet::new();
        sites
            .iter()
            .filter(|s| s.name == name)
            .map(&place)
            .filter(|p| seen.insert(p.clone()))
            .map(|p| p + ",")
            .collect::<String>()
    };
    let mut names: Vec<&String> = nlabels.keys().collect();
    names.sort();
    let (mut moved, mut changed) = (String::new(), String::new());
    for name in &names {
        let (Some(o), n) = (olabels.get(*name), &nlabels[*name]) else {
            continue;
        };
        if o.content == n.content {
            continue;
        }
        let entry = format!(
            "(name: {}, kind: {}, at: ({})),",
            typst_str(name),
            typst_str(&n.kind),
            at(name)
        );
        let other = nall.contains(&o.content) || oall.contains(&n.content);
        if other || similarity(&words(&o.content), &words(&n.content)) < 0.4 {
            moved.push_str(&entry);
        } else if !at(name).is_empty() {
            changed.push_str(&entry);
        }
    }
    let mut removed: BTreeSet<&String> = olabels.keys().filter(|k| !nlabels.contains_key(*k)).collect();
    let mut added: BTreeSet<&String> = nlabels.keys().filter(|k| !olabels.contains_key(*k)).collect();
    // A new label on what an old label was on renames it.
    let mut renamed = String::new();
    for m in added.clone() {
        let from = olabels
            .iter()
            .filter(|(l, t)| t.content == nlabels[m].content && nlabels.get(*l).is_none_or(|n| n.content != t.content))
            .map(|(l, _)| l)
            .min();
        if let Some(l) = from {
            renamed.push_str(&format!("({}, {}),", typst_str(l), typst_str(m)));
            removed.remove(l);
            added.remove(m);
        }
    }
    let list = |ks: &BTreeSet<&String>| ks.iter().map(|k| typst_str(k) + ",").collect::<String>();
    format!(
        "(moved: ({moved}), changed: ({changed}), added: ({}), removed: ({}), renamed: ({renamed}))",
        list(&added),
        list(&removed)
    )
}
