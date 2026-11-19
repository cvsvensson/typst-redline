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

use typst_syntax::ast::AstNode;
use typst_syntax::{SyntaxKind, SyntaxNode, ast, parse};

pub use side::{Anchor, Pair, Placed, PlacedKind, Side, composite, pair_pages};

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
    /// The files of the new version that the comparison marks, or lists if
    /// they change.
    pub checked: Vec<String>,
    /// Each marked file of the new version with the old file it is compared
    /// with, as (old, new) paths.
    pub pairs: Vec<(String, String)>,
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
    /// For a change that is not marked, the old code, empty if it is new;
    /// for moved text at its new place, a line that names it.
    pub old: String,
    /// Continues the change before it at the same place, as the insertion
    /// that replaces a deletion does: the summary counts them as one.
    pub joined: bool,
    /// For a change that is not marked, or a mark that shows only a bar,
    /// what it changes, in words.
    pub what: Vec<What>,
}

/// A part of a change of code that is not marked, described for a reader.
#[derive(Clone, Debug, PartialEq)]
pub enum What {
    /// The argument `name` changed from the code `old` to `new`, `None`
    /// where it is not given; a value too long to read in a line is `…`.
    Arg {
        name: String,
        old: Option<String>,
        new: Option<String>,
    },
    /// The file the code reads at this root-relative path changed.
    File(String),
    /// A change said in a sentence.
    Note(String),
}

impl What {
    /// The dictionary the preamble takes: `(file: path)`, or `(name: name,
    /// old: value, new: value)` with each value `none` where not given,
    /// `(code: code)`, or for strings `(text: pieces)`, the pieces `(text,
    /// changed)`.
    fn to_typst(&self) -> String {
        let (name, old, new) = match self {
            What::File(p) => return format!("(file: {})", mark::typst_str(p)),
            What::Note(n) => return format!("(note: {})", mark::typst_str(n)),
            What::Arg { name, old, new } => (name, old, new),
        };
        let string = |code: &str| {
            let root = typst_syntax::parse_code(code);
            let mut exprs = root.children().filter_map(|c| c.cast::<ast::Expr>());
            match (exprs.next(), exprs.next()) {
                (Some(ast::Expr::Str(s)), None) => Some(s.get().to_string()),
                _ => None,
            }
        };
        let code = |v: &Option<String>| {
            v.as_ref()
                .map_or("none".into(), |c| format!("(code: {})", mark::typst_str(c)))
        };
        let (old, new) = match (old.as_deref().and_then(string), new.as_deref().and_then(string)) {
            (Some(o), Some(n)) => {
                let (ow, nw) = (syntax::split_words(&o), syntax::split_words(&n));
                let (mut op, mut np) = (String::new(), String::new());
                for s in mark::segments(&ow, &nw) {
                    // Spaces at the edges of a change stay unmarked.
                    let piece = |ws: &[&str]| {
                        let t = ws.concat();
                        let core = t.trim();
                        let (lead, trail) = (&t[..t.len() - t.trim_start().len()], &t[t.trim_end().len()..]);
                        [(lead, false), (core, !s.equal), (trail, false)]
                            .iter()
                            .filter(|(p, _)| !p.is_empty())
                            .map(|(p, c)| format!("({}, {c}),", mark::typst_str(p)))
                            .collect::<String>()
                    };
                    if !s.old.is_empty() {
                        op.push_str(&piece(&ow[s.old.clone()]));
                    }
                    if !s.new.is_empty() {
                        np.push_str(&piece(&nw[s.new.clone()]));
                    }
                }
                (format!("(text: ({op}))"), format!("(text: ({np}))"))
            }
            _ => (code(old), code(new)),
        };
        format!("(name: {}, old: {old}, new: {new})", mark::typst_str(name))
    }
}

impl Change {
    /// The fields of the summary's entry for a change that is not marked.
    pub(crate) fn code_fields(&self) -> String {
        let what: String = self.what.iter().map(|w| w.to_typst() + ",").collect();
        format!(
            "old: {}, new: {}, what: ({what})",
            mark::typst_str(&self.old),
            mark::typst_str(&self.text)
        )
    }
    /// The change as one that is not marked, with its old and new text.
    pub fn unmarked(&self) -> Change {
        let (old, text) = match self.kind {
            ChangeKind::Ins => (String::new(), self.text.clone()),
            ChangeKind::Del => (self.text.clone(), String::new()),
            ChangeKind::Moved => (String::new(), self.old.clone()),
            ChangeKind::Unmarked => (self.old.clone(), self.text.clone()),
        };
        Change {
            kind: ChangeKind::Unmarked,
            text,
            old,
            ..self.clone()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ChangeKind {
    Ins,
    Del,
    /// A change the output does not show.
    Unmarked,
    /// Moved text, recorded at its old and at its new place, with the
    /// number of the move as its text; at the new place `old` names it.
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
/// in the entry. `reads` are the files the new document read when it was
/// compiled, if known: the changed ones are listed, or marked if they are
/// Typst files that a computed path includes or imports.
pub fn redline(
    old: &Version,
    new: &Version,
    opts: Options,
    history: &History,
    moves: Moves,
    reads: Option<&[String]>,
) -> Redline {
    let mut warnings = vec![];
    let modules = modules(new, reads);
    let modules = reads.map(|_| modules.as_slice());
    let mut old_files = project(old, &mut warnings, modules);
    let new_files = project(new, &mut warnings, modules);
    let unknown = reads.is_none();
    let reads = reads.unwrap_or_default();
    let (mut bib, explained) = bibliography(old, &old_files, new, &new_files);
    bib.files = changed_reads(old, &old_files, new, &new_files, reads);
    bib.files.retain(|f| !explained.contains(f));
    bib.unchecked = unknown && reads_computed_paths(&new_files);
    let mut checked = literal_reads(&new_files);
    checked.extend(new_files.iter().map(|(p, _)| p.clone()));
    checked.extend(reads.iter().cloned());
    let pinned = pin_reads(&mut old_files, old, new, &new_files);
    bib.files.retain(|p| pinned.iter().all(|(_, _, q)| q != p));
    let steps = history.steps.as_ref().map(|vs| {
        vs.iter()
            .map(|v| {
                let mut files = project(v, &mut vec![], modules);
                pin_reads(&mut files, v, new, &new_files);
                files
            })
            .collect()
    });
    let ctx = mark::Ctx::new(old_files, new_files, steps, history.swapped, pinned, opts, moves);
    let (files, changes) = mark::write(&ctx, &old.label, &new.label, &bib, history);
    Redline {
        files,
        changes,
        bib,
        warnings,
        moves: ctx.moves(),
        checked: checked.into_iter().collect(),
        pairs: ctx.pairs.iter().map(|(n, o)| (o.clone(), n.clone())).collect(),
    }
}

/// The root-relative paths of the files the document reads: the entry, the
/// files it includes or imports, directly or not, as for [`redline`] with
/// `reads`, then the files it reads by a literal path, its bibliography
/// files and `reads`.
pub fn project_files(v: &Version, reads: Option<&[String]>) -> Vec<String> {
    let modules = modules(v, reads);
    let files = project(v, &mut vec![], reads.map(|_| modules.as_slice()));
    let mut other = literal_reads(&files);
    other.extend(bib_paths(&files));
    other.extend(reads.unwrap_or_default().iter().cloned());
    let mut paths: Vec<String> = files.into_iter().map(|(p, _)| p).collect();
    other.retain(|p| !paths.contains(p));
    paths.extend(other);
    paths
}

/// The Typst files among `reads` that no call reads by a literal path: those
/// a computed path may include or import.
fn modules(new: &Version, reads: Option<&[String]>) -> Vec<String> {
    let Some(reads) = reads.filter(|r| !r.is_empty()) else {
        return vec![];
    };
    let literal = literal_reads(&project(new, &mut vec![], None));
    reads
        .iter()
        .filter(|p| p.ends_with(".typ") && !literal.contains(*p))
        .cloned()
        .collect()
}

/// The entry and the files it includes or imports, directly or not, with
/// their sources. A missing entry is empty. Files are found by a literal
/// path, or, if a file includes or imports a computed path, among `modules`:
/// the Typst files the document is known to read otherwise. If a `read` call
/// takes a computed path, those may be read as text and are left out.
/// Without `modules`, a computed include is warned about.
fn project(v: &Version, warnings: &mut Vec<String>, modules: Option<&[String]>) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = vec![];
    let mut queue = vec![v.entry.clone()];
    let (mut computed, mut computed_read) = (false, false);
    loop {
        let Some(path) = queue.pop() else {
            match modules {
                Some(m) if std::mem::take(&mut computed) && !computed_read => {
                    queue.extend(m.iter().rev().filter(|p| files.iter().all(|(q, _)| q != *p)).cloned());
                    continue;
                }
                _ => break,
            }
        };
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
            if let Some(call) = n.cast::<ast::FuncCall>()
                && let ast::Expr::Ident(f) = call.callee()
                && f.as_str() == "read"
            {
                computed_read |= !matches!(call.args().items().next(), Some(ast::Arg::Pos(ast::Expr::Str(_))));
            }
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
                _ => {
                    computed = true;
                    (include && modules.is_none())
                        .then(|| format!("{path}: the included path is computed, so changes inside it are not marked"))
                }
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
/// the document reads and that differ. `unchecked` if the document reads
/// files through a computed path and which files it read is not known.
#[derive(Default)]
pub struct Bib {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
    pub files: Vec<String>,
    pub unchecked: bool,
    /// The entries of the keys above that have an author and a year, by
    /// key, named by them.
    pub names: Vec<(String, String)>,
}

impl Bib {
    /// The dictionary the preamble takes.
    pub(crate) fn to_typst(&self) -> String {
        let keys = |ks: &[String]| format!("({})", ks.iter().map(|k| mark::typst_str(k) + ",").collect::<String>());
        format!(
            "(added: {}, removed: {}, changed: {}, files: {}, unchecked: {}, names: ({}))",
            keys(&self.added),
            keys(&self.removed),
            keys(&self.changed),
            keys(&self.files),
            self.unchecked,
            match self.names.as_slice() {
                [] => ":".into(),
                names => names
                    .iter()
                    .map(|(k, n)| format!("{}: {},", mark::typst_str(k), mark::typst_str(n)))
                    .collect::<String>(),
            }
        )
    }
}

/// The bibliography files that `bibliography` calls name by literal paths.
fn bib_paths(files: &[(String, String)]) -> Vec<String> {
    let mut paths = vec![];
    for (path, src) in files {
        walk(&parse(src), &mut |n| {
            if n.kind() == SyntaxKind::FuncCall
                && n.children()
                    .next()
                    .is_some_and(|f| f.leaf_text().as_str() == "bibliography")
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
        });
    }
    paths
}

/// The changes of the bibliography, and the bibliography files whose
/// changes they hold.
fn bibliography(
    old: &Version,
    old_files: &[(String, String)],
    new: &Version,
    new_files: &[(String, String)],
) -> (Bib, Vec<String>) {
    // The bibliography files a version names, and the references in it that
    // are not to one of its labels. Keys found in no entry are left out,
    // such as labels that code makes.
    let scan = |files: &[(String, String)]| {
        let (mut refs, mut labels) = (BTreeSet::new(), HashSet::new());
        for (_, src) in files {
            walk(&parse(src), &mut |n| match n.kind() {
                SyntaxKind::Ref => drop(refs.insert(ref_name(n))),
                SyntaxKind::Label => drop(labels.insert(label_name(n))),
                _ => {}
            });
        }
        refs.retain(|r| !labels.contains(r));
        (bib_paths(files), refs)
    };
    let ((old_paths, old_keys), (new_paths, new_keys)) = (scan(old_files), scan(new_files));
    if new_paths.is_empty() {
        return (Bib::default(), vec![]);
    }
    let entries = |v: &Version, paths: &[String]| -> HashMap<String, String> {
        paths
            .iter()
            .filter_map(|p| Some(bib_entries(p, &text(v, p)?)))
            .flatten()
            .collect()
    };
    let (old_entries, new_entries) = (entries(old, &old_paths), entries(new, &new_paths));
    let mut bib = Bib {
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
            .filter(|k| {
                old_entries
                    .get(*k)
                    .is_some_and(|e| Some(collapse_ws(e)) != new_entries.get(*k).map(|n| collapse_ws(n)))
            })
            .cloned()
            .collect(),
        ..Bib::default()
    };
    let named = |entries: &HashMap<String, String>, k: &String| Some((k.clone(), bib_name(entries.get(k)?)?));
    bib.names = (bib.added.iter().chain(&bib.changed))
        .filter_map(|k| named(&new_entries, k))
        .chain(bib.removed.iter().filter_map(|k| named(&old_entries, k)))
        .collect();
    // A bibliography file whose changes are all in the entries listed is not
    // listed as a changed file.
    let listed: HashSet<&String> = bib.added.iter().chain(&bib.removed).chain(&bib.changed).collect();
    let rest = |v: &Version, p: &str| -> Option<BTreeSet<(String, String)>> {
        let entries = bib_entries(p, &text(v, p)?);
        Some(
            entries
                .into_iter()
                .filter(|(k, _)| !listed.contains(k))
                .map(|(k, e)| (k, collapse_ws(&e)))
                .collect(),
        )
    };
    let explained = new_paths
        .iter()
        .filter(|p| old_paths.contains(p) && rest(old, p).is_some_and(|r| Some(r) == rest(new, p)))
        .cloned()
        .collect();
    (bib, explained)
}

const READERS: [&str; 9] = ["image", "read", "json", "csv", "yaml", "toml", "xml", "cbor", "plugin"];

/// Whether a file includes a computed path, imports one built by an
/// expression, or passes one to a call such as `json`. An import of a name
/// is left out: the name usually stands for a module, not a path.
fn reads_computed_paths(files: &[(String, String)]) -> bool {
    let mut computed = false;
    for (_, src) in files {
        walk(&parse(src), &mut |n| {
            let path = if let Some(i) = n.cast::<ast::ModuleInclude>() {
                Some(i.source())
            } else if let Some(i) = n.cast::<ast::ModuleImport>() {
                Some(i.source()).filter(|s| !matches!(s, ast::Expr::Ident(_) | ast::Expr::FieldAccess(_)))
            } else if let Some(call) = n.cast::<ast::FuncCall>()
                && let ast::Expr::Ident(f) = call.callee()
                && READERS.contains(&f.as_str())
            {
                match call.args().items().next() {
                    Some(ast::Arg::Pos(e)) => Some(e),
                    _ => None,
                }
            } else {
                None
            };
            computed |= path.is_some_and(|p| !matches!(p, ast::Expr::Str(_)));
        });
    }
    computed
}

/// The paths that calls such as `image("fig.png")` or `json("data.json")`
/// read.
fn literal_reads(files: &[(String, String)]) -> BTreeSet<String> {
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
}

/// The files that differ between the versions, among those that both
/// versions read by a literal path and `reads`, leaving out the marked files.
/// A call that only one version makes is a change of the text and marked
/// there.
fn changed_reads(
    old: &Version,
    old_files: &[(String, String)],
    new: &Version,
    new_files: &[(String, String)],
    reads: &[String],
) -> Vec<String> {
    let mut paths = &literal_reads(old_files) & &literal_reads(new_files);
    paths.extend(reads.iter().cloned());
    paths
        .into_iter()
        .filter(|p| new_files.iter().all(|(q, _)| q != p) && content(old, p) != content(new, p))
        .collect()
}

/// A file's bytes; text with line endings normalized, as git can check out
/// text with other line endings than it stores.
fn content(v: &Version, p: &str) -> Option<Vec<u8>> {
    (v.read)(p).map(|b| match String::from_utf8(b) {
        Ok(s) => normalize(&s).into_bytes(),
        Err(e) => e.into_bytes(),
    })
}

/// Rewrites, in the files of version `v`, each literal path that a call
/// such as `image` reads and whose file differs from the one in `new` into
/// a path from the root under [`OLD_DIR`]: the call then differs from the
/// new one, so that it is compared, and marked, as changed, and evaluated
/// it reads the old file. Returns the (rewritten literal, original literal,
/// root-relative path) of each.
fn pin_reads(
    files: &mut [(String, String)],
    v: &Version,
    new: &Version,
    new_files: &[(String, String)],
) -> Vec<(String, String, String)> {
    let mut pinned = vec![];
    for (path, src) in files.iter_mut() {
        let root = parse(src);
        let mut pins: HashMap<*const SyntaxNode, String> = HashMap::new();
        walk(&root, &mut |n| {
            if let Some(call) = n.cast::<ast::FuncCall>()
                && let ast::Expr::Ident(f) = call.callee()
                && READERS.contains(&f.as_str())
                && let Some(ast::Arg::Pos(ast::Expr::Str(lit))) = call.args().items().next()
                && let Some(p) = resolve(path, &lit.get())
                && new_files.iter().all(|(q, _)| *q != p)
                && content(v, &p).is_some_and(|c| Some(c) != content(new, &p))
            {
                let pin = mark::typst_str(&format!("/{OLD_DIR}/{p}"));
                let entry = (pin.clone(), lit.to_untyped().leaf_text().to_string(), p);
                if !pinned.contains(&entry) {
                    pinned.push(entry);
                }
                pins.insert(lit.to_untyped() as *const _, pin);
            }
        });
        if pins.is_empty() {
            continue;
        }
        fn into(n: &SyntaxNode, pins: &HashMap<*const SyntaxNode, String>, out: &mut String) {
            match pins.get(&(n as *const _)) {
                Some(pin) => out.push_str(pin),
                None if n.children().len() == 0 => out.push_str(n.leaf_text()),
                None => n.children().for_each(|c| into(c, pins, out)),
            }
        }
        let mut out = String::new();
        into(&root, &pins, &mut out);
        *src = out;
    }
    pinned
}

/// A file as text with a byte-order mark and CR line endings removed.
fn text(v: &Version, path: &str) -> Option<String> {
    String::from_utf8((v.read)(path)?).ok().map(|s| normalize(&s))
}

/// The entries of a BibTeX or Hayagriva file by key, as they are written: a
/// BibTeX entry starts at a line `@type{key,`, a Hayagriva entry at an
/// unindented line `key:`.
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
                text.push('\n');
                text.push_str(line);
            }
            _ => {}
        }
    }
    out
}

/// A bibliography entry named by its authors' family names and its year,
/// such as "Aguado 2017", "Aguado and Lee 2017" or "Aguado et al. 2017".
fn bib_name(entry: &str) -> Option<String> {
    let yaml = !entry.starts_with('@');
    let authors: Vec<String> = if yaml {
        yaml_field(entry, "author")?
    } else {
        bibtex_field(entry, "author")?
            .split(" and ")
            .map(str::to_string)
            .collect()
    };
    let date = if yaml {
        yaml_field(entry, "date")?.join(" ")
    } else {
        bibtex_field(entry, "year").or_else(|| bibtex_field(entry, "date"))?
    };
    let year: String = date.split(|c: char| !c.is_ascii_digit()).find(|d| d.len() == 4)?.into();
    let family = |a: &String| {
        let a = a.trim_matches(['"', '\'', ' ']).replace(['{', '}'], "");
        match a.split_once(',') {
            Some((family, _)) => family.trim().to_string(),
            None => a.split_whitespace().last().unwrap_or_default().to_string(),
        }
    };
    let names = match authors.as_slice() {
        [a] => family(a),
        [a, b] => format!("{} and {}", family(a), family(b)),
        [a, ..] => format!("{} et al.", family(a)),
        [] => return None,
    };
    (!names.is_empty()).then(|| format!("{names} {year}"))
}

/// The value of the field `name = {...}`, `name = "..."` or `name = value` of
/// a BibTeX entry.
fn bibtex_field(entry: &str, name: &str) -> Option<String> {
    let lower = entry.to_ascii_lowercase();
    lower.match_indices(name).find_map(|(i, _)| {
        if lower[..i].ends_with(|c: char| c.is_alphanumeric()) {
            return None;
        }
        let rest = entry[i + name.len()..].trim_start().strip_prefix('=')?.trim_start();
        if let Some(inner) = rest.strip_prefix('{') {
            let mut depth = 0;
            let end = inner.find(|c| {
                depth += match c {
                    '{' => 1,
                    '}' => -1,
                    _ => 0,
                };
                depth < 0
            })?;
            Some(inner[..end].to_string())
        } else if let Some(inner) = rest.strip_prefix('"') {
            Some(inner.split('"').next()?.to_string())
        } else {
            Some(rest.split([',', '}', '\n']).next()?.trim().to_string())
        }
    })
}

/// The values of the field `name` of a Hayagriva entry: one value, a list
/// in brackets or a list of lines starting with `-`.
fn yaml_field(entry: &str, name: &str) -> Option<Vec<String>> {
    let mut lines = entry.lines().skip(1);
    let first = lines
        .find_map(|l| l.trim_start().strip_prefix(name)?.strip_prefix(':'))?
        .trim();
    let values: Vec<String> = if first.is_empty() {
        lines
            .map_while(|l| l.trim_start().strip_prefix('-'))
            .map(|v| v.trim().to_string())
            .collect()
    } else if let Some(list) = first.strip_prefix('[') {
        list.trim_end_matches(']')
            .split("\",")
            .map(|v| v.trim().to_string())
            .collect()
    } else {
        vec![first.to_string()]
    };
    Some(values)
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
