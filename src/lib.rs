//! Produce a Typst project that shows the new version of a document with
//! the changes against an old version marked.

mod align;
mod blame;
mod mark;
mod moves;
mod side;
mod syntax;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Range;

use typst_syntax::{SyntaxKind, SyntaxNode, ast, parse};

pub use side::{Anchor, Pair, Placed, Side, composite, pair_pages};

use syntax::{collapse_ws, label_name, most_similar, ref_name, similarity, tokenize, walk, words};

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
#[derive(Clone)]
pub struct Version<'a> {
    pub label: String,
    pub entry: String,
    pub read: &'a dyn Fn(&str) -> Option<Vec<u8>>,
}

#[derive(Clone, Copy)]
pub struct Options {
    /// Evaluate deleted code expressions; otherwise show them as source text.
    pub eval_deleted: bool,
    /// Diff inside the content blocks of code expressions that differ only
    /// there; otherwise such expressions are compared whole.
    pub content_blocks: bool,
}

/// A commit as the summary lists it.
pub struct Commit {
    pub hash: String,
    pub date: String,
    pub subject: String,
}

/// How the new version came about from the old one, for the summary.
#[derive(Default)]
pub struct History<'a> {
    /// The commits from the old version to the new one, oldest first.
    pub commits: Vec<Commit>,
    /// The new version has changes beyond the last commit.
    pub uncommitted: bool,
    /// To mark each change in the colour of the step that made it: the
    /// version after each step, labelled by its revision, the last one (the
    /// new version) left out.
    pub steps: Option<Vec<Version<'a>>>,
    /// If steps group commits: the number of commits in each step, the
    /// uncommitted changes being part of the last. Empty if each commit, and
    /// the uncommitted changes, is a step of its own.
    pub step_commits: Vec<usize>,
    /// The two versions are swapped: the steps lead from the new version to
    /// the old one. Each change still takes the step that made it.
    pub swapped: bool,
}

pub struct Redline {
    /// The marked files, as (root-relative path, content), to lay over the
    /// new version with a preamble at [`PREAMBLE_PATH`]. If one is under
    /// [`OLD_DIR`], the old version's other files belong there too.
    pub files: Vec<(String, String)>,
    pub changes: Vec<Change>,
    pub bib: Bib,
    pub warnings: Vec<String>,
    pub moves: Vec<Move>,
}

/// Top-level blocks moved together: the blocks `old.1` of the old file at
/// path `old.0` became the blocks `new.1` of the new file `new.0`.
#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    pub old: (String, Range<usize>),
    pub new: (String, Range<usize>),
    /// The same for the move in both directions.
    pub key: u64,
}

/// The moves a comparison marks.
#[derive(Clone, Copy)]
pub enum Moves<'a> {
    /// None: moved text is deleted at one place and inserted at the other.
    Off,
    Find,
    /// The moves of the comparison in the other direction, so that both
    /// show the same moves.
    Reverse(&'a [Move]),
}

#[derive(Clone, Debug)]
pub struct Change {
    pub file: String,
    /// Label of the enclosing section, `None` before the first section.
    pub section: Option<String>,
    pub kind: ChangeKind,
    /// Typst markup of the changed content; for a change that is not
    /// marked, the new code, empty if it was deleted.
    pub text: String,
    /// For a change that is not marked, the old code, empty if it is new.
    pub old: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ChangeKind {
    Ins,
    Del,
    /// A change the output does not show.
    Unmarked,
    /// Moved text, recorded at its old and at its new place, with the
    /// number of the move as its text.
    Moved,
}

impl ChangeKind {
    fn name(self) -> &'static str {
        match self {
            ChangeKind::Ins => "ins",
            ChangeKind::Del => "del",
            ChangeKind::Unmarked => "unmarked",
            ChangeKind::Moved => "moved",
        }
    }
}

/// The new version of the project with the changes against the old version
/// marked in every file the entry includes or imports, and a summary page
/// in the entry.
pub fn redline(old: &Version, new: &Version, opts: Options, history: &History, moves: Moves) -> Redline {
    let mut warnings = vec![];
    let old_files = project(old, &mut warnings);
    let new_files = project(new, &mut warnings);
    let mut bib = bibliography(old, &old_files, new, &new_files);
    bib.files = changed_reads(old, &old_files, new, &new_files);
    let steps = history
        .steps
        .as_ref()
        .map(|vs| vs.iter().map(|v| project(v, &mut vec![])).collect());
    let ctx = mark::Ctx::new(old_files, new_files, steps, history.swapped, opts, moves);
    let (files, changes) = mark::write(&ctx, &old.label, &new.label, &bib, history);
    Redline {
        files,
        changes,
        bib,
        warnings,
        moves: ctx.moves(),
    }
}

/// The root-relative paths of the entry and the files it includes or
/// imports by a literal path, directly or not.
pub fn project_files(v: &Version) -> Vec<String> {
    project(v, &mut vec![]).into_iter().map(|(p, _)| p).collect()
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
        let Some(src) = text(v, &path) else {
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

/// What changed in the files the document reads, which the output cannot
/// mark: keys cited only in the new or only in the old version, keys cited
/// in the new version whose bibliography entry differs, and other files that
/// both versions read by the same literal path but that differ.
#[derive(Default)]
pub struct Bib {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
    pub files: Vec<String>,
}

impl Bib {
    /// The dictionary the preamble takes.
    pub(crate) fn to_typst(&self) -> String {
        let keys = |ks: &[String]| format!("({})", ks.iter().map(|k| mark::typst_str(k) + ",").collect::<String>());
        format!(
            "(added: {}, removed: {}, changed: {}, files: {})",
            keys(&self.added),
            keys(&self.removed),
            keys(&self.changed),
            keys(&self.files)
        )
    }
}

fn bibliography(old: &Version, old_files: &[(String, String)], new: &Version, new_files: &[(String, String)]) -> Bib {
    // The bibliography files a version names, and the references in it that
    // are not to one of its labels. Keys found in no entry are left out,
    // such as labels that code makes.
    let scan = |files: &[(String, String)]| {
        let (mut paths, mut refs, mut labels) = (vec![], BTreeSet::new(), HashSet::new());
        for (path, src) in files {
            walk(&parse(src), &mut |n| match n.kind() {
                SyntaxKind::Ref => drop(refs.insert(ref_name(n))),
                SyntaxKind::Label => drop(labels.insert(label_name(n))),
                SyntaxKind::FuncCall
                    if n.children()
                        .next()
                        .is_some_and(|f| f.leaf_text().as_str() == "bibliography") =>
                {
                    let args = n.children().filter(|c| c.kind() == SyntaxKind::Args);
                    for a in args
                        .flat_map(|a| a.children())
                        .filter(|a| a.kind() != SyntaxKind::Named)
                    {
                        walk(a, &mut |s| {
                            if let Some(s) = s.cast::<ast::Str>() {
                                paths.extend(resolve(path, &s.get()));
                            }
                        });
                    }
                }
                _ => {}
            });
        }
        refs.retain(|r| !labels.contains(r));
        (paths, refs)
    };
    let ((old_paths, old_keys), (new_paths, new_keys)) = (scan(old_files), scan(new_files));
    if new_paths.is_empty() {
        return Bib::default();
    }
    let entries = |v: &Version, paths: &[String]| -> HashMap<String, String> {
        paths
            .iter()
            .filter_map(|p| Some(bib_entries(p, &text(v, p)?)))
            .flatten()
            .collect()
    };
    let (old_entries, new_entries) = (entries(old, &old_paths), entries(new, &new_paths));
    Bib {
        added: new_keys
            .difference(&old_keys)
            .filter(|k| new_entries.contains_key(*k))
            .cloned()
            .collect(),
        removed: old_keys
            .difference(&new_keys)
            .filter(|k| old_entries.contains_key(*k))
            .cloned()
            .collect(),
        changed: new_keys
            .iter()
            .filter(|k| old_entries.get(*k).is_some_and(|e| Some(e) != new_entries.get(*k)))
            .cloned()
            .collect(),
        files: vec![],
    }
}

/// The paths that calls such as `image("fig.png")` or `json("data.json")`
/// read in both versions, whose files differ. A call that only one version
/// makes is a change of the text and marked there.
fn changed_reads(
    old: &Version,
    old_files: &[(String, String)],
    new: &Version,
    new_files: &[(String, String)],
) -> Vec<String> {
    const READERS: [&str; 9] = ["image", "read", "json", "csv", "yaml", "toml", "xml", "cbor", "plugin"];
    let reads = |files: &[(String, String)]| {
        let mut paths = BTreeSet::new();
        for (path, src) in files {
            walk(&parse(src), &mut |n| {
                if let Some(call) = n.cast::<ast::FuncCall>()
                    && let ast::Expr::Ident(f) = call.callee()
                    && READERS.contains(&f.as_str())
                    && let Some(ast::Arg::Pos(ast::Expr::Str(s))) = call.args().items().next()
                {
                    paths.extend(resolve(path, &s.get()));
                }
            });
        }
        paths
    };
    // Git can check out text with other line endings than it stores.
    let content = |v: &Version, p: &str| {
        (v.read)(p).map(|b| match String::from_utf8(b) {
            Ok(s) => normalize(&s).into_bytes(),
            Err(e) => e.into_bytes(),
        })
    };
    reads(old_files)
        .intersection(&reads(new_files))
        .filter(|p| matches!((content(old, p), content(new, p)), (Some(a), Some(b)) if a != b))
        .cloned()
        .collect()
}

/// A file as text with a byte-order mark and CR line endings removed.
fn text(v: &Version, path: &str) -> Option<String> {
    String::from_utf8((v.read)(path)?).ok().map(|s| normalize(&s))
}

/// The entries of a BibTeX or Hayagriva file by key, with whitespace
/// collapsed: a BibTeX entry starts at a line `@type{key,`, a Hayagriva
/// entry at an unindented line `key:`.
fn bib_entries(path: &str, src: &str) -> Vec<(String, String)> {
    let yaml = path.ends_with(".yml") || path.ends_with(".yaml");
    let mut out: Vec<(String, String)> = vec![];
    for line in src.lines() {
        let key = if yaml {
            Some(line)
                .filter(|l| !l.starts_with([' ', '\t', '#', '-']))
                .and_then(|l| l.split_once(':'))
                .map(|(k, _)| k)
        } else {
            line.strip_prefix('@')
                .and_then(|rest| rest.split_once('{'))
                .map(|(_, k)| k.split(',').next().unwrap_or(k))
        };
        match (key, out.last_mut()) {
            (Some(k), _) => out.push((k.trim().into(), line.into())),
            (None, Some((_, text))) => {
                text.push(' ');
                text.push_str(line);
            }
            _ => {}
        }
    }
    out.into_iter().map(|(k, t)| (k, collapse_ws(&t))).collect()
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

impl History<'_> {
    /// The dictionary the preamble takes, with the latest commits if there
    /// are many and steps do not group them; `None` without commits or
    /// steps. `old` and `new` label the two versions.
    pub(crate) fn to_typst(&self, old: &str, new: &str) -> Option<String> {
        const SHOWN: usize = 10;
        if self.commits.is_empty() && self.steps.is_none() {
            return None;
        }
        let grouped = !self.step_commits.is_empty();
        let earlier = if grouped {
            0
        } else {
            self.commits.len().saturating_sub(SHOWN)
        };
        let commits: String = self.commits[earlier..]
            .iter()
            .map(|c| {
                format!(
                    "(hash: {}, date: {}, subject: {}),",
                    mark::typst_str(&c.hash),
                    mark::typst_str(&c.date),
                    mark::typst_str(&c.subject)
                )
            })
            .collect();
        let groups = match &self.steps {
            Some(steps) if grouped => {
                let ends: Vec<&str> = steps.iter().map(|v| v.label.as_str()).collect();
                let froms = [old].into_iter().chain(ends.iter().copied());
                let tos = ends.iter().copied().chain([new]);
                let groups: String = froms
                    .zip(tos)
                    .zip(&self.step_commits)
                    .map(|((f, t), n)| {
                        format!(
                            "(from: {}, to: {}, commits: {n}),",
                            mark::typst_str(f),
                            mark::typst_str(t)
                        )
                    })
                    .collect();
                format!(", groups: ({groups})")
            }
            _ => String::new(),
        };
        Some(format!(
            "(commits: ({commits}), earlier: {earlier}, uncommitted: {}, steps: {}{groups})",
            self.uncommitted,
            self.steps.is_some()
        ))
    }
}

/// Whether the `redline-ins` and `redline-del` of a preamble take the
/// `move` argument that moved text needs, by name or in an argument sink.
pub fn preamble_takes_moves(src: &str) -> bool {
    let mut takes = HashSet::new();
    walk(&parse(src), &mut |n| {
        let Some(binding) = n.cast::<ast::LetBinding>() else {
            return;
        };
        let (ast::LetBindingKind::Closure(name), Some(ast::Expr::Closure(f))) = (binding.kind(), binding.init()) else {
            return;
        };
        let moves = f.params().children().any(|p| match p {
            ast::Param::Named(a) => a.name().as_str() == "move",
            ast::Param::Spread(_) => true,
            ast::Param::Pos(_) => false,
        });
        if moves {
            takes.insert(name.as_str().to_string());
        }
    });
    ["redline-ins", "redline-del"].iter().all(|f| takes.contains(*f))
}
