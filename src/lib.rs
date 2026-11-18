//! Produce a Typst project that shows the new version of a document with
//! the changes against an old version marked.

mod mark;
mod syntax;

use std::collections::HashMap;

use typst_syntax::{SyntaxNode, ast, parse};

use syntax::{most_similar, similarity, tokenize, walk, words};

/// The built-in preamble, which defines the functions the marked files
/// call.
pub const PREAMBLE: &str = include_str!("preamble.typ");

/// Path of the preamble in the output, relative to the project root.
pub const PREAMBLE_PATH: &str = ".redline-preamble.typ";

/// Directory of the output that holds the old version of the project, for
/// deleted content to be evaluated with the old definitions and data.
pub const OLD_DIR: &str = ".redline-old";

/// A version of a project: `read` returns the file at a root-relative path
/// with `/` separators, or `None` if it does not exist; `entry` is the path
/// of the main file.
pub struct Version<'a> {
    pub label: String,
    pub entry: String,
    pub read: &'a dyn Fn(&str) -> Option<String>,
}

#[derive(Clone, Copy)]
pub struct Options {
    /// Evaluate deleted code expressions; otherwise show them as source text.
    pub eval_deleted: bool,
    /// Diff inside the content blocks of code expressions that differ only
    /// there; otherwise such expressions are compared whole.
    pub content_blocks: bool,
}

pub struct Redline {
    /// The marked files, as (root-relative path, content), to lay over the
    /// new version with a preamble at [`PREAMBLE_PATH`]. If one is under
    /// [`OLD_DIR`], the old version's other files belong there too.
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
/// marked in every file the entry includes or imports, and a summary page
/// in the entry.
pub fn redline(old: &Version, new: &Version, opts: Options) -> Redline {
    let mut warnings = vec![];
    let old_files = project(old, &mut warnings);
    let new_files = project(new, &mut warnings);
    let ctx = mark::Ctx::new(old_files, new_files, opts);
    let (files, changes) = mark::write(&ctx, &old.label, &new.label);
    Redline {
        files,
        changes,
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
    let toks = |s: &str| words(&tokenize(&parse(s).children().cloned().collect::<Vec<SyntaxNode>>()));
    for (p, s) in &new[1..] {
        if pairs.contains_key(p) {
            continue;
        }
        let ntoks = toks(s);
        let best = most_similar(
            old.iter()
                .filter(|(q, _)| !pairs.values().any(|o| o == q))
                .map(|(q, os)| (q, similarity(&toks(os), &ntoks))),
        );
        if let Some(q) = best {
            pairs.insert(p.clone(), q.clone());
        }
    }
    pairs
}

/// Strip a byte-order mark and convert CRLF and CR line endings to LF.
fn normalize(s: &str) -> String {
    s.strip_prefix('\u{feff}')
        .unwrap_or(s)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}
