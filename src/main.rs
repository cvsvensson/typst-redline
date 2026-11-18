mod tree;

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};

use tree::{Tree, git, output};
use typst_redline::{
    Anchor, Change, ChangeKind, Commit, History, Moves, OLD_DIR, Options, PREAMBLE, PREAMBLE_PATH, Placed, Redline,
    Side, Version, composite, preamble_takes_moves, project_files, redline,
};

const USAGE: &str = "\
typst-redline: mark changes between two versions of a Typst document

Usage:
  typst-redline <file.typ> [--since <rev>] [--to <rev>] [options] [-- <typst args>]
  typst-redline <old> <new> [options] [-- <typst args>]

With one file, the old version is the file at git revision --since (default
HEAD) and the new version is the working copy, or the file at --to. --since
A..B is --since A --to B; --since A...B compares the merge base of A and B
with B. With two files, each is a path or REV:PATH (:PATH for the index).

--mark-reviewed records HEAD as reviewed for the file, in a local git ref
under refs/typst-redline/reviewed/, and makes no PDF. --since reviewed then
compares with that commit, unless git already resolves `reviewed` as a branch,
tag or other revision, which then takes its place.

Options:
  -o, --output <pdf>    Output PDF (default: <name>.redline.pdf next to the new file)
  --root <dir>          Project root (default: TYPST_ROOT, else the directory
                        of each file)
  --preamble <file>     Replace the built-in change styling
  --layout <layout>     inline (default): the new version with changes marked;
                        side-by-side: changed pages of the old and the new
                        version next to each other (needs Typst 0.14 or later)
  --each-commit         Colour each change by the commit that made it, for at
                        most 6 commits between the two versions (one file
                        only)
  --steps <revs|n>      Colour each change by the step that made it: the steps
                        end at the given revisions, separated by commas, and
                        at the new version. With a number n from 2 to 6, the
                        tool splits the commits, and then the uncommitted
                        changes, into n steps that change about the same
                        amount of the document (one file only)
  --no-moves            Show moved text as deleted at its old place and
                        inserted at its new place
  --typst <path>        typst executable (default: typst)
  --mark-reviewed       Record HEAD as reviewed for the file (see above)
  --keep-typ            Keep the generated project and print its path on stderr
  -V, --version         Print the version
  -h, --help            Print this help
Arguments after -- are passed to `typst compile`.
";

struct Args {
    files: Vec<String>,
    since: Option<String>,
    to: Option<String>,
    output: Option<PathBuf>,
    root: Option<PathBuf>,
    /// The root comes from `TYPST_ROOT`.
    root_env: bool,
    preamble: Option<PathBuf>,
    typst: String,
    keep: bool,
    side_by_side: bool,
    steps: Option<StepArg>,
    mark_reviewed: bool,
    no_moves: bool,
    typst_args: Vec<String>,
}

/// How changes are grouped into steps, each marked in its own colour.
enum StepArg {
    EachCommit,
    /// This many steps that change about the same amount.
    Count(usize),
    /// The revisions where steps end, but the last.
    Revs(Vec<String>),
}

impl StepArg {
    fn parse(v: &str) -> Result<StepArg, String> {
        // Git abbreviates hashes to at least four digits.
        if v.len() < 4 && v.chars().all(|c| c.is_ascii_digit()) {
            return match v.parse() {
                Ok(n @ 2..=MAX_STEPS) => Ok(StepArg::Count(n)),
                _ => Err(format!(
                    "--steps takes a number of steps from 2 to {MAX_STEPS}, or revisions separated by commas"
                )),
            };
        }
        let revs: Vec<String> = v.split(',').map(|r| r.trim().to_string()).collect();
        if revs.iter().any(String::is_empty) {
            return Err(format!("--steps {v}: an empty revision"));
        }
        Ok(StepArg::Revs(revs))
    }

    fn option(&self) -> &'static str {
        match self {
            StepArg::EachCommit => "--each-commit",
            _ => "--steps",
        }
    }
}

/// The most steps the preamble has colours for.
const MAX_STEPS: usize = 6;

enum Parsed {
    Run(Box<Args>),
    Print(String),
}

fn parse_args() -> Result<Parsed, String> {
    let mut a = Args {
        files: vec![],
        since: None,
        to: None,
        output: None,
        root: None,
        root_env: false,
        preamble: None,
        typst: "typst".into(),
        keep: false,
        side_by_side: false,
        steps: None,
        mark_reviewed: false,
        no_moves: false,
        typst_args: vec![],
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut val = || it.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "-h" | "--help" => return Ok(Parsed::Print(USAGE.into())),
            "-V" | "--version" => {
                let version = format!("typst-redline {}\n", env!("CARGO_PKG_VERSION"));
                return Ok(Parsed::Print(version));
            }
            "--since" => a.since = Some(val()?),
            "--to" => a.to = Some(val()?),
            "-o" | "--output" => a.output = Some(val()?.into()),
            "--root" => a.root = Some(val()?.into()),
            "--preamble" => a.preamble = Some(val()?.into()),
            "--typst" => a.typst = val()?,
            "--keep-typ" => a.keep = true,
            "--each-commit" | "--steps" => {
                if a.steps.is_some() {
                    return Err("give one of --each-commit and --steps".into());
                }
                a.steps = Some(match arg.as_str() {
                    "--steps" => StepArg::parse(&val()?)?,
                    _ => StepArg::EachCommit,
                });
            }
            "--mark-reviewed" => a.mark_reviewed = true,
            "--no-moves" => a.no_moves = true,
            "--layout" => {
                a.side_by_side = match val()?.as_str() {
                    "inline" => false,
                    "side-by-side" => true,
                    l => return Err(format!("unknown layout {l}; give inline or side-by-side")),
                }
            }
            "--" => a.typst_args.extend(it.by_ref()),
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => a.files.push(arg),
        }
    }
    if a.to.is_some() && a.since.as_deref().is_some_and(|s| s.contains("..")) {
        return Err("--to cannot be combined with a range in --since".into());
    }
    if a.mark_reviewed && (a.files.len() != 1 || a.since.is_some() || a.to.is_some()) {
        return Err("--mark-reviewed takes one file and no --since or --to".into());
    }
    if a.root.is_none() {
        a.root = std::env::var_os("TYPST_ROOT")
            .filter(|r| !r.is_empty())
            .map(PathBuf::from);
        a.root_env = a.root.is_some();
    }
    if let Some(s) = &a.steps
        && a.files.len() != 1
    {
        return Err(format!("{} works with one file, not with two", s.option()));
    }
    match (a.files.len(), &a.since, &a.to) {
        (1, _, _) | (2, None, None) => Ok(Parsed::Run(Box::new(a))),
        _ => Err("give one file, or two files without --since and --to".into()),
    }
}

/// One side of the comparison: `entry` in `tree`, whose working-tree
/// location is `file`.
struct Spec {
    tree: Tree,
    root: PathBuf,
    entry: String,
    file: PathBuf,
    label: String,
}

/// `file` in the working tree (`rev` = `None`), the index (`Some("")`) or a
/// git revision, in a tree rooted at `root` or the file's directory.
fn spec(file: &Path, rev: Option<&str>, root: Option<&Path>, label: String) -> Result<Spec, String> {
    let file = std::path::absolute(file).map_err(|e| e.to_string())?;
    let root = std::path::absolute(root.unwrap_or(file.parent().unwrap())).map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Err(format!("the root {} is not a directory", root.display()));
    }
    if rev.is_none() && file.is_dir() {
        return Err(format!("{} is a directory", file.display()));
    }
    // Through a symbolic link, such as /var to /private/var on macOS, the
    // root can be written differently from the file's path.
    let canonical = || {
        let dir = std::fs::canonicalize(file.parent()?).ok()?;
        let path = dir.join(file.file_name()?);
        Some(
            path.strip_prefix(std::fs::canonicalize(&root).ok()?)
                .ok()?
                .to_path_buf(),
        )
    };
    let entry = file
        .strip_prefix(&root)
        .map(Path::to_path_buf)
        .ok()
        .or_else(canonical)
        .ok_or_else(|| format!("{} is not inside the root {}", file.display(), root.display()))?
        .iter()
        .map(|c| c.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    let tree = match rev {
        None => Tree::Dir(root.clone()),
        Some(rev) => {
            let out = output(git(&root).args(["rev-parse", "--show-toplevel", "--show-prefix"]))?;
            if !rev.is_empty() {
                output(git(&root).args(["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")]))
                    .map_err(|_| format!("git: no commit {rev}"))?;
            }
            let out = String::from_utf8_lossy(&out);
            let mut lines = out.lines();
            Tree::Git {
                repo: lines.next().unwrap_or_default().into(),
                prefix: lines.next().unwrap_or_default().into(),
                rev: (!rev.is_empty()).then(|| rev.to_string()),
            }
        }
    };
    Ok(Spec {
        tree,
        root,
        entry,
        file,
        label,
    })
}

fn git_text(dir: &Path, args: &[&str]) -> Result<String, String> {
    Ok(String::from_utf8_lossy(&output(git(dir).args(args))?)
        .trim()
        .to_string())
}

/// A path, or `REV:PATH` with PATH relative to the repository root unless it
/// starts with `./` or `../`.
fn spec_arg(arg: &str, root: Option<&Path>) -> Result<Spec, String> {
    let Some((rev, path)) = arg.split_once(':').filter(|_| !Path::new(arg).exists()) else {
        return spec(Path::new(arg), None, root, arg.into());
    };
    let base = if path.starts_with("./") || path.starts_with("../") {
        PathBuf::from(".")
    } else {
        git_text(Path::new("."), &["rev-parse", "--show-toplevel"])?.into()
    };
    let label = if rev.is_empty() { "index" } else { rev };
    spec(&base.join(path), Some(rev), root, label.into())
}

fn specs(a: &Args, reviewed: Option<&str>) -> Result<(Spec, Spec), String> {
    let root = a.root.as_deref();
    if a.files.len() == 2 {
        let (mut old, mut new) = (spec_arg(&a.files[0], root)?, spec_arg(&a.files[1], root)?);
        if a.files.iter().all(|f| f == &old.label || f == &new.label) {
            (old.label, new.label) = short_labels(&old.label, &new.label);
        }
        return Ok((old, new));
    }
    let file = std::path::absolute(&a.files[0]).map_err(|e| e.to_string())?;
    let file = file.as_path();
    let since = reviewed.or(a.since.as_deref()).unwrap_or("HEAD");
    // As in git, an omitted end of a range is HEAD.
    fn head(r: &str) -> &str {
        if r.is_empty() { "HEAD" } else { r }
    }
    let (old, to) = match (since.split_once("..."), since.split_once("..")) {
        (Some((x, y)), _) => {
            let base = git_text(file.parent().unwrap(), &["merge-base", head(x), head(y)])?;
            (base, Some(head(y)))
        }
        (None, Some((x, y))) => (head(x).to_string(), Some(head(y))),
        _ => (since.to_string(), a.to.as_deref()),
    };
    let new = match to {
        Some(to) => spec(file, Some(to), root, to.into())?,
        None => spec(file, None, root, "working tree".into())?,
    };
    let label = if reviewed.is_some() { "reviewed" } else { &old };
    let mut old = spec(file, Some(&old), root, label.into())?;
    if old.tree.read(&old.entry).is_none()
        && let Some(path) = renamed(&old, to)
    {
        old.entry = path;
    }
    Ok((old, new))
}

/// The ref that records the commit marked as reviewed for the file at
/// `path` from the repository's top level. Every character but ASCII letters,
/// digits, `-`, `_` and `/` is percent-encoded, which keeps the names
/// distinct and valid for git.
fn reviewed_ref(path: &str) -> String {
    let mut name = String::from("refs/typst-redline/reviewed/");
    for c in path.chars() {
        if c.is_ascii_alphanumeric() || "-_/".contains(c) {
            name.push(c);
        } else {
            name.extend(c.to_string().bytes().map(|b| format!("%{b:02X}")));
        }
    }
    name
}

/// The file's path from the repository's top level as git records it, which
/// on a case-insensitive filesystem can differ in case from `file`; `None` if
/// git does not track it.
fn git_path(file: &Path) -> Result<Option<String>, String> {
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    let spec = format!(":(icase,literal){name}");
    let out = output(git(file.parent().unwrap()).args(["ls-files", "--full-name", "-z", "--", &spec]))?;
    let out = String::from_utf8_lossy(&out);
    let paths: Vec<&str> = out.split('\0').filter(|p| !p.is_empty()).collect();
    let exact = paths.iter().find(|p| p.rsplit('/').next() == Some(&*name));
    Ok(exact.or(paths.first()).map(|p| p.to_string()))
}

/// The commit marked as reviewed for the file, if `--since reviewed` refers
/// to it: when git resolves no revision named `reviewed`.
fn since_reviewed(a: &Args) -> Result<Option<String>, String> {
    if a.files.len() != 1 || a.since.as_deref() != Some("reviewed") {
        return Ok(None);
    }
    let file = std::path::absolute(&a.files[0]).map_err(|e| e.to_string())?;
    let dir = file.parent().unwrap();
    let marked = git_path(&file)?.and_then(|p| {
        let commit = format!("{}^{{commit}}", reviewed_ref(&p));
        git_text(dir, &["rev-parse", "--verify", "--quiet", &commit]).ok()
    });
    if git_text(dir, &["rev-parse", "--verify", "--quiet", "reviewed^{commit}"]).is_ok() {
        if marked.is_some() {
            let name = git_text(dir, &["rev-parse", "--symbolic-full-name", "reviewed"]).unwrap_or_default();
            let name = if name.is_empty() {
                "the revision reviewed"
            } else {
                &name
            };
            eprintln!("note: --since reviewed uses {name}; the commit marked with --mark-reviewed is ignored");
        }
        return Ok(None);
    }
    marked.map(Some).ok_or_else(|| {
        format!(
            "no commit is marked as reviewed for {0}; mark the last commit with: typst-redline {0} --mark-reviewed",
            a.files[0]
        )
    })
}

/// Record HEAD as reviewed for the file and say so.
fn mark_reviewed(a: &Args) -> Result<String, String> {
    let file = &a.files[0];
    let path = std::path::absolute(file).map_err(|e| e.to_string())?;
    let dir = path.parent().unwrap();
    let head = git_text(dir, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    let name = git_path(&path)?
        .filter(|p| {
            git(dir)
                .args(["cat-file", "-e", &format!("{head}:{p}")])
                .output()
                .is_ok_and(|o| o.status.success())
        })
        .ok_or_else(|| format!("{file} is not in the last commit, so there is nothing to mark as reviewed"))?;
    let refname = reviewed_ref(&name);
    output(git(dir).args(["update-ref", "-m", "typst-redline --mark-reviewed", &refname, &head]))?;
    // The document's files at HEAD, and the file itself in case git records
    // it in a different case than given.
    let spec = spec(&path, Some(&head), a.root.as_deref(), String::new())?;
    let read = |p: &str| spec.tree.read(p);
    let version = Version {
        label: String::new(),
        entry: spec.entry.clone(),
        read: &read,
    };
    let Tree::Git { repo, prefix, .. } = &spec.tree else {
        unreachable!()
    };
    let files = project_files(&version).into_iter().map(|f| format!("{prefix}{f}"));
    let status = ["status", "--porcelain", "--untracked-files=no", "--"];
    if !output(git(repo).args(status).arg(&name).args(files))?.is_empty() {
        eprintln!("note: the document has uncommitted changes, which the mark does not cover");
    }
    let short = git_text(dir, &["rev-parse", "--short", &head])?;
    Ok(format!("marked {short} as reviewed for {file} in {refname}"))
}

/// Two paths labelled by as many of their last parts as tell them apart.
fn short_labels(a: &str, b: &str) -> (String, String) {
    fn parts(l: &str) -> Vec<&str> {
        l.split(['/', '\\']).filter(|p| !p.is_empty()).rev().collect()
    }
    let (a, b) = (parts(a), parts(b));
    let k = (1..a.len().max(b.len())).find(|&k| a[..k.min(a.len())] != b[..k.min(b.len())]);
    let short = |p: &[&str]| {
        p[..k.unwrap_or(p.len()).min(p.len())]
            .iter()
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("/")
    };
    (short(&a), short(&b))
}

/// The path of the old version's entry if git sees it renamed in the new one.
fn renamed(old: &Spec, to: Option<&str>) -> Option<String> {
    let Tree::Git { repo, prefix, rev } = &old.tree else {
        return None;
    };
    let mut cmd = git(repo);
    cmd.args(["diff", "-M", "--name-status", "-z", rev.as_deref()?]);
    cmd.args(to);
    let out = String::from_utf8(output(&mut cmd).ok()?).ok()?;
    let fields: Vec<&str> = out.split('\0').collect();
    let target = format!("{prefix}{}", old.entry);
    fields
        .windows(3)
        .find(|w| w[0].starts_with('R') && w[2] == target)
        .and_then(|w| w[1].strip_prefix(prefix.as_str()))
        .map(str::to_string)
}

/// Tried in turn until one compiles: a frame cannot hold content that sets
/// the page, marks inside code can break functions that inspect their body,
/// and deleted code can fail (the old version it is evaluated in fails on
/// its own). The first flag keeps state updates in deleted content, which
/// some code needs but which can change what reads the state after it; the
/// second says whether the preamble frames content computed at layout.
const LADDER: [(Options, bool, bool, &str); 6] = [
    (
        Options {
            eval_deleted: true,
            content_blocks: true,
        },
        false,
        true,
        "",
    ),
    (
        Options {
            eval_deleted: true,
            content_blocks: true,
        },
        true,
        true,
        "state updates in deleted content are kept",
    ),
    (
        Options {
            eval_deleted: true,
            content_blocks: true,
        },
        false,
        false,
        "changed content computed at layout is not framed",
    ),
    (
        Options {
            eval_deleted: false,
            content_blocks: true,
        },
        false,
        false,
        "changed content computed at layout is not framed and deleted code is shown by name",
    ),
    (
        Options {
            eval_deleted: true,
            content_blocks: false,
        },
        false,
        false,
        "changed content computed at layout is not framed and changed code is marked whole",
    ),
    (
        Options {
            eval_deleted: false,
            content_blocks: false,
        },
        false,
        false,
        "changed content computed at layout is not framed, deleted code is shown by name and changed code is marked whole",
    ),
];

/// Make the PDF; `reviewed` is the commit `--since reviewed` stands for.
fn run(a: Args, reviewed: Option<&str>) -> Result<PathBuf, String> {
    let preamble = match &a.preamble {
        Some(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let needs = [
                (a.side_by_side, "redline-side-by-side", "the side-by-side layout"),
                (
                    a.steps.is_some(),
                    "redline-step-style",
                    a.steps.as_ref().map_or("", StepArg::option),
                ),
            ];
            if let Some((_, name, what)) = needs.iter().find(|(on, name, _)| *on && !text.contains(name)) {
                return Err(format!(
                    "the preamble {} does not define {name}, which {what} needs",
                    p.display()
                ));
            }
            text
        }
        None => PREAMBLE.to_string(),
    };
    if a.side_by_side
        && let Some(v) = typst_version(&a.typst)
        && minor(&v) < (0, 14)
    {
        return Err(format!(
            "the side-by-side layout needs Typst 0.14 or later, but {} is Typst {v}",
            a.typst
        ));
    }
    let (old, new) = specs(&a, reviewed).map_err(|e| match a.root_env && e.contains("the root") {
        true => format!("{e} (set by TYPST_ROOT)"),
        false => e,
    })?;
    if new.tree.read(&new.entry).is_none() {
        return Err(match new.tree {
            Tree::Dir(_) => format!("{} does not exist", new.file.display()),
            Tree::Git { .. } => format!("{} does not exist in {}", new.entry, new.label),
        });
    }
    let pdf = output_path(&a, &old, &new)?;
    let (read_old, read_new) = (|p: &str| old.tree.read(p), |p: &str| new.tree.read(p));
    let version = |spec: &Spec, read| Version {
        label: spec.label.clone(),
        entry: spec.entry.clone(),
        read,
    };
    let (old_v, new_v) = (version(&old, &read_old), version(&new, &read_new));
    let files = [project_files(&old_v), project_files(&new_v)].concat();
    let log = log(&old, &new, &files)?;
    let plan = match (&a.steps, &log) {
        (None, _) => Plan::default(),
        (Some(s), Some(log)) => plan_steps(s, log, &old, &new)?,
        (Some(s), None) => {
            return Err(format!(
                "{} needs two git revisions, or a revision and the working tree",
                s.option()
            ));
        }
    };
    let step_trees: Vec<Tree> = plan
        .ends
        .iter()
        .map(|(hash, _)| match &old.tree {
            Tree::Git { repo, prefix, .. } => Tree::Git {
                repo: repo.clone(),
                prefix: prefix.clone(),
                rev: Some(hash.clone()),
            },
            Tree::Dir(_) => unreachable!(),
        })
        .collect();
    // A step before the entry was renamed has it under its old name.
    let step_entry = |t: &Tree| {
        if t.read(&new.entry).is_some() {
            new.entry.clone()
        } else {
            old.entry.clone()
        }
    };
    let step_reads: Vec<_> = step_trees.iter().map(|t| move |p: &str| t.read(p)).collect();
    let history = History {
        steps: a.steps.is_some().then(|| {
            step_trees
                .iter()
                .zip(&step_reads)
                .zip(&plan.ends)
                .map(|((t, read), (_, label))| Version {
                    label: label.clone(),
                    entry: step_entry(t),
                    read,
                })
                .collect()
        }),
        step_commits: plan.commits,
        ..log.map(|l| l.history).unwrap_or_default()
    };

    let moves = if a.no_moves { Moves::Off } else { Moves::Find };
    let mut result = redline(&old_v, &new_v, LADDER[0].0, &history, moves);
    for w in &result.warnings {
        eprintln!("warning: {w}");
    }
    if let Some(p) = &a.preamble
        && !result.moves.is_empty()
        && !preamble_takes_moves(&preamble)
    {
        return Err(format!(
            "the preamble {} does not take the move argument of redline-ins and redline-del, which moved text needs",
            p.display()
        ));
    }
    let staging = Staging::new(a.keep);
    let dir = &staging.dir;
    stage(dir, &old, &new, &result)?;
    write(dir, &[(PREAMBLE_PATH.into(), preamble.clone())])?;
    // typst writes next to the staging directory, so that an output that
    // cannot be written is reported as such and is never left half written.
    let tmp = dir.with_extension(pdf.extension().unwrap_or("pdf".as_ref()));
    let mut first_error = String::new();
    let mut stderr = String::new();
    // The old side of the side-by-side layout is the old version marked
    // against the new one, staged with the old version at the root, so that
    // it is laid out and numbered as the old document is.
    let old_root = dir.join(OLD_SIDE);
    let sides = [
        (old_root.as_path(), &old.entry, "old", OLD_PDF),
        (dir.as_path(), &new.entry, "new", NEW_PDF),
    ];
    let mut a = a;
    let mut back = None;
    for (i, (opts, keep_states, frame_computed, note)) in LADDER.into_iter().enumerate() {
        // The side-by-side layout shows no deleted content.
        if keep_states && a.side_by_side {
            continue;
        }
        let states = format!("redline-states={}", if keep_states { "keep" } else { "drop" });
        if i > 0 {
            result = redline(&old_v, &new_v, opts, &history, moves);
            if LADDER[i - 1].2 && !frame_computed {
                a.typst_args
                    .extend(["--input".into(), "redline-frame-computed=false".into()]);
            }
        }
        let side = |(root, entry, side, pdf): (&Path, &String, &str, &str)| {
            let input = format!("redline-side={side}");
            typst(
                &a,
                root,
                "compile",
                &[&"--input", &input, &root.join(entry), &dir.join(pdf)],
            )
        };
        write(dir, &result.files)?;
        let out = if !a.side_by_side {
            typst(&a, dir, "compile", &[&"--input", &states, &dir.join(&new.entry), &tmp])?
        } else {
            let back_history = History {
                steps: history.steps.clone(),
                step_commits: history.step_commits.clone(),
                swapped: true,
                ..History::default()
            };
            let back = back.insert(redline(
                &new_v,
                &old_v,
                opts,
                &back_history,
                Moves::Reverse(&result.moves),
            ));
            if i == 0 {
                stage(&old_root, &new, &old, back)?;
                write(&old_root, &[(PREAMBLE_PATH.into(), preamble.clone())])?;
            }
            write(&old_root, &back.files)?;
            let new_side = side(sides[1])?;
            if new_side.status.success() {
                side(sides[0])?
            } else {
                new_side
            }
        };
        stderr = String::from_utf8_lossy(&out.stderr).to_string();
        if out.status.success() {
            eprint!("{stderr}");
            if !note.is_empty() {
                eprintln!("note: {note}, because the full proof failed to compile ({first_error})");
            }
            if let Some(back) = &back {
                compose(
                    &a,
                    dir,
                    sides,
                    (&old.label, &new.label),
                    [back, &result],
                    &history,
                    &tmp,
                )?;
            } else if ["foreground", "background"]
                .iter()
                .all(|w| result.files.iter().any(|(_, s)| s.contains(w)))
            {
                // Bars are lost only where the document sets both, and asking
                // where costs another compile. The summary page lists such
                // pages in any case.
                let args: [&dyn AsRef<OsStr>; 7] = [
                    &"--input",
                    &states,
                    &dir.join(&new.entry),
                    &"<redline-unbarred>",
                    &"--field",
                    &"value",
                    &"--one",
                ];
                let out = typst(&a, dir, "query", &args)?;
                warn_unbarred(&ints(&out.stdout).unwrap_or_default(), &pdf.display().to_string());
            }
            let written = std::fs::copy(&tmp, &pdf);
            let _ = std::fs::remove_file(&tmp);
            written.map_err(|e| format!("cannot write {}: {e}", pdf.display()))?;
            return Ok(pdf);
        }
        if first_error.is_empty() {
            first_error = stderr
                .lines()
                .find(|l| l.starts_with("error:"))
                .unwrap_or_default()
                .into();
        }
    }
    eprint!("{stderr}");
    if let Some(hint) = version_hint(&a.typst) {
        eprintln!("{hint}");
    }
    let not_found = a.root.is_none() && stderr.contains("file not found");
    if not_found {
        eprintln!(
            "hint: paths starting with / resolve against the directory of the file; set another root with --root"
        );
    }
    if (not_found || stderr.contains("project root"))
        && let Some(top) = repository(&new).filter(|top| !same_dir(top, &new.root))
    {
        eprintln!("hint: to use the repository as the root, add --root {}", top.display());
    }
    let hint = if a.keep {
        ""
    } else {
        " (rerun with --keep-typ to inspect the generated source)"
    };
    Err(format!("typst compile failed{hint}"))
}

/// The top level of the git repository that holds a version, if any.
fn repository(spec: &Spec) -> Option<PathBuf> {
    match &spec.tree {
        Tree::Git { repo, .. } => Some(repo.clone()),
        Tree::Dir(root) => git_text(root, &["rev-parse", "--show-toplevel"])
            .ok()
            .map(PathBuf::from),
    }
}

/// The commits from the old version to the new one that touch the project
/// `files`, oldest first, with their full hashes; `None` unless the old
/// version is a git revision and the new one a later revision, the index or
/// the working tree of the same project. Merged branches count as their
/// merge commit.
struct Log {
    /// The new version's revision; HEAD for the index or the working tree.
    to: String,
    hashes: Vec<String>,
    /// The lines each commit, and then the uncommitted changes if any, add
    /// and remove in the project files.
    weights: Vec<u64>,
    history: History<'static>,
}

fn log(old: &Spec, new: &Spec, files: &[String]) -> Result<Option<Log>, String> {
    let Tree::Git {
        repo,
        prefix,
        rev: Some(from),
    } = &old.tree
    else {
        return Ok(None);
    };
    let paths: Vec<String> = files.iter().map(|f| format!("{prefix}{f}")).collect();
    let numstat = |args: &[&str]| -> Result<String, String> {
        let out = output(git(repo).args(args).arg("--").args(&paths))?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    };
    let (to, uncommitted) = match &new.tree {
        Tree::Git {
            repo: r,
            prefix: p,
            rev,
        } if r == repo && p == prefix => match rev {
            Some(to) => (to.as_str(), None),
            None => ("HEAD", Some(numstat(&["diff", "--cached", "--numstat", "HEAD"])?)),
        },
        Tree::Dir(root) if same_dir(root, &repo.join(prefix)) => {
            ("HEAD", Some(numstat(&["diff", "--numstat", "HEAD"])?))
        }
        _ => return Ok(None),
    };
    let uncommitted = uncommitted.filter(|s| !s.is_empty());
    let out = output(
        git(repo)
            .args([
                "log",
                "--first-parent",
                "--reverse",
                "--date=short",
                "--numstat",
                "--format=%x1e%H%x1f%h%x1f%ad%x1f%s",
                &format!("{from}..{to}"),
                "--",
            ])
            .args(&paths),
    )?;
    let (mut hashes, mut weights, mut commits) = (vec![], vec![], vec![]);
    for record in String::from_utf8_lossy(&out).split('\x1e').skip(1) {
        let (head, stat) = record.split_once('\n').unwrap_or((record, ""));
        let f: Vec<&str> = head.splitn(4, '\x1f').collect();
        if let [full, hash, date, subject] = f[..] {
            hashes.push(full.to_string());
            weights.push(numstat_weight(stat));
            commits.push(Commit {
                hash: hash.into(),
                date: date.into(),
                subject: subject.into(),
            });
        }
    }
    weights.extend(uncommitted.as_deref().map(numstat_weight));
    Ok(Some(Log {
        to: to.into(),
        hashes,
        weights,
        history: History {
            commits,
            uncommitted: uncommitted.is_some(),
            ..History::default()
        },
    }))
}

/// The lines that `git diff --numstat` output adds and removes; a binary
/// file, which has "-" for both, counts as one.
fn numstat_weight(stat: &str) -> u64 {
    stat.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some((f.next()?, f.next()?))
        })
        .map(|(add, del)| match (add.parse::<u64>(), del.parse::<u64>()) {
            (Ok(a), Ok(d)) => a + d,
            _ => 1,
        })
        .sum()
}

/// The steps changes are coloured by: the revision each step but the last
/// ends at, as (full hash, label), and the number of commits of the log in
/// each step if steps group commits (empty if each commit is a step).
#[derive(Default)]
struct Plan {
    ends: Vec<(String, String)>,
    commits: Vec<usize>,
}

fn plan_steps(arg: &StepArg, log: &Log, old: &Spec, new: &Spec) -> Result<Plan, String> {
    let opt = arg.option();
    let Tree::Git {
        repo, rev: Some(from), ..
    } = &old.tree
    else {
        unreachable!()
    };
    let ancestor = git(repo)
        .args(["merge-base", "--is-ancestor", from, &log.to])
        .status()
        .is_ok_and(|s| s.success());
    if !ancestor {
        return Err(format!(
            "{opt} needs {} to be an ancestor of {}; to start where they diverge, give --since {}...{}",
            old.label, new.label, old.label, log.to
        ));
    }
    let h = &log.history;
    let n = h.commits.len();
    if n + usize::from(h.uncommitted) == 0 {
        return Err(format!(
            "no commits from {} to {} change the document, so {opt} has nothing to colour",
            old.label, new.label
        ));
    }
    let at = |cuts: &[usize]| {
        cuts.iter()
            .map(|&i| (log.hashes[i].clone(), h.commits[i].hash.clone()))
            .collect()
    };
    match arg {
        StepArg::EachCommit => {
            let steps = n + usize::from(h.uncommitted);
            if steps > MAX_STEPS {
                return Err(format!(
                    "--each-commit colours at most {MAX_STEPS} steps, but {steps} commits from {} to {} change the document{}; give a narrower range, such as --since {}, or fewer steps with --steps",
                    old.label,
                    new.label,
                    if h.uncommitted {
                        " (counting the uncommitted changes)"
                    } else {
                        ""
                    },
                    h.commits[steps - MAX_STEPS - 1].hash
                ));
            }
            Ok(Plan {
                ends: at(&(0..steps - 1).collect::<Vec<_>>()),
                commits: vec![],
            })
        }
        StepArg::Count(k) => {
            // The uncommitted changes, if any, are the last of the units
            // that steps group.
            if log.weights.len() < *k {
                let s = if n == 1 { "" } else { "s" };
                let what = match (n, h.uncommitted) {
                    (0, _) => "the uncommitted changes".to_string(),
                    (_, false) => format!("{n} commit{s}"),
                    (_, true) => format!("{n} commit{s} and the uncommitted changes"),
                };
                let so = if log.weights.len() == 1 {
                    "there is one step"
                } else {
                    "each is a step"
                };
                eprintln!(
                    "note: from {} to {} the document is changed only by {what}, so {so}",
                    old.label, new.label
                );
            }
            let cuts = balanced_cuts(&log.weights, *k);
            let bounds: Vec<usize> = [0].into_iter().chain(cuts.iter().map(|c| c + 1)).chain([n]).collect();
            Ok(Plan {
                ends: at(&cuts),
                commits: bounds.windows(2).map(|w| w[1] - w[0]).collect(),
            })
        }
        StepArg::Revs(revs) => rev_steps(revs, log, old, new),
    }
}

/// The steps of `--steps` given as the revisions where they end.
fn rev_steps(revs: &[String], log: &Log, old: &Spec, new: &Spec) -> Result<Plan, String> {
    let Tree::Git {
        repo, rev: Some(from), ..
    } = &old.tree
    else {
        unreachable!()
    };
    if revs.len() >= MAX_STEPS {
        return Err(format!(
            "--steps makes at most {MAX_STEPS} steps, so it takes at most {} revisions",
            MAX_STEPS - 1
        ));
    }
    let path = git_text(
        repo,
        &[
            "rev-list",
            "--first-parent",
            "--reverse",
            &format!("{from}..{}", log.to),
        ],
    )?;
    let path: Vec<&str> = path.lines().collect();
    // With the working tree or the index as the new version, its last
    // commit is a step boundary before it.
    let new_is_rev = matches!(new.tree, Tree::Git { rev: Some(_), .. });
    let inside = path.len().saturating_sub(usize::from(new_is_rev));
    let mut ends: Vec<(usize, &String)> = vec![];
    for rev in revs {
        let hash = git_text(
            repo,
            &["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")],
        )
        .map_err(|_| format!("--steps: git: no commit {rev}"))?;
        if new_is_rev && path.last() == Some(&hash.as_str()) {
            return Err(format!(
                "--steps: {rev} is the new version {}, where the last step already ends; give only the revisions before it",
                new.label
            ));
        }
        let p = path[..inside].iter().position(|c| *c == hash).ok_or_else(|| {
            format!(
                "--steps: {rev} is not a commit between {} and {} on the first-parent path",
                old.label, new.label
            )
        })?;
        if let Some(&(q, prev)) = ends.last() {
            if p == q && rev == prev {
                return Err(format!("--steps: {rev} is given twice"));
            }
            if p == q {
                return Err(format!("--steps: {rev} and {prev} are the same commit"));
            }
            if p < q {
                return Err(format!(
                    "--steps: {rev} comes before {prev}; give the revisions oldest first"
                ));
            }
        }
        ends.push((p, rev));
    }
    let mut commits = vec![0; ends.len() + 1];
    for hash in &log.hashes {
        let p = path.iter().position(|c| c == hash).unwrap_or(path.len());
        commits[ends.iter().filter(|(q, _)| *q < p).count()] += 1;
    }
    Ok(Plan {
        ends: ends
            .iter()
            .map(|(p, rev)| (path[*p].to_string(), rev.to_string()))
            .collect(),
        commits,
    })
}

/// Where to cut commits of the given weights into `n` steps: the index of
/// the last commit of each step but the last. The cuts make the heaviest step
/// as light as possible, then the steps as even as possible (least sum of
/// squares), then come as early as possible. Fewer commits than `n` are a
/// step each.
fn balanced_cuts(weights: &[u64], n: usize) -> Vec<usize> {
    let m = weights.len();
    if m <= n {
        return (0..m.saturating_sub(1)).collect();
    }
    let mut sum = vec![0u64; m + 1];
    for (i, w) in weights.iter().enumerate() {
        sum[i + 1] = sum[i] + w;
    }
    // For commits i.. in k steps no heavier than `limit`: the least sum of
    // squares and the end of the first step, the earliest among equals.
    let best = |limit: u64| {
        let mut t = vec![vec![None::<(u64, usize)>; n + 1]; m + 1];
        for i in (0..m).rev() {
            let w = sum[m] - sum[i];
            t[i][1] = (w <= limit).then_some((w * w, m));
            for k in 2..=n.min(m - i) {
                for j in i + 1..=m - (k - 1) {
                    let w = sum[j] - sum[i];
                    if w > limit {
                        break;
                    }
                    if let Some((rest, _)) = t[j][k - 1]
                        && t[i][k].is_none_or(|(c, _)| w * w + rest < c)
                    {
                        t[i][k] = Some((w * w + rest, j));
                    }
                }
            }
        }
        t
    };
    // Filling steps greedily up to `limit` takes the fewest steps; with more
    // commits than steps, any split into fewer can be cut further.
    let fits = |limit: u64| {
        let (mut steps, mut w) = (1, 0);
        for &x in weights {
            if w + x > limit {
                steps += 1;
                w = 0;
            }
            w += x;
        }
        steps <= n
    };
    // The heaviest step is as light as possible at the least limit that
    // admits a split.
    let (mut lo, mut hi) = (weights.iter().copied().max().unwrap_or(0), sum[m]);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if fits(mid) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let t = best(lo);
    let (mut cuts, mut i) = (vec![], 0);
    for k in (2..=n).rev() {
        let j = t[i][k].unwrap().1;
        cuts.push(j - 1);
        i = j;
    }
    cuts
}

fn same_dir(a: &Path, b: &Path) -> bool {
    std::fs::canonicalize(a)
        .ok()
        .is_some_and(|a| std::fs::canonicalize(b).ok() == Some(a))
}

/// The output PDF: `-o`, or `<name>.redline.pdf` next to the new file. It
/// must be in an existing directory and must not be an input.
fn output_path(a: &Args, old: &Spec, new: &Spec) -> Result<PathBuf, String> {
    let pdf = match &a.output {
        Some(o) => o.clone(),
        None => {
            let stem = new
                .file
                .file_stem()
                .ok_or(format!("{} has no file name", new.file.display()))?;
            new.file
                .with_file_name(format!("{}.redline.pdf", stem.to_string_lossy()))
        }
    };
    let pdf = std::path::absolute(&pdf).map_err(|e| e.to_string())?;
    if !pdf.parent().is_some_and(Path::is_dir) {
        return Err(format!("the directory of the output {} does not exist", pdf.display()));
    }
    let canonical = |p: &Path| std::fs::canonicalize(p).ok();
    if canonical(&pdf).is_some_and(|p| [&old.file, &new.file].iter().any(|f| canonical(f) == Some(p.clone()))) {
        return Err(format!("the output {} is an input", pdf.display()));
    }
    Ok(pdf)
}

/// The staging directory, removed on drop unless it is to be kept.
struct Staging {
    dir: PathBuf,
    keep: bool,
}

impl Staging {
    fn new(keep: bool) -> Self {
        let dir = std::env::temp_dir().join(format!("typst-redline-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Staging { dir, keep }
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if self.keep {
            eprintln!("kept {}", self.dir.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// Put in `dir` the new version's files, then old-only files so that deleted
/// figures resolve, then files git does not track, which belong to every
/// version; and the same with the old version first under [`OLD_DIR`], if
/// the result uses it. Files the output writes are left out, so that writing
/// them cannot change a source hard-linked into `dir`.
fn stage(dir: &Path, old: &Spec, new: &Spec, result: &Redline) -> Result<(), String> {
    let mut have: HashSet<String> = result.files.iter().map(|(p, _)| p.clone()).collect();
    have.insert(PREAMBLE_PATH.into());
    let (worktree, untracked) = new.tree.untracked()?;
    let (new_files, old_files) = (new.tree.list()?, old.tree.list()?);
    let mut layers = vec![
        (&new.tree, &new_files, ""),
        (&old.tree, &old_files, ""),
        (&worktree, &untracked, ""),
    ];
    if result.files.iter().any(|(p, _)| p.starts_with(OLD_DIR)) {
        layers.extend([
            (&old.tree, &old_files, OLD_DIR),
            (&worktree, &untracked, OLD_DIR),
            (&new.tree, &new_files, OLD_DIR),
        ]);
    }
    for (tree, files, sub) in layers {
        let fresh: Vec<&str> = files
            .iter()
            .filter(|p| {
                have.insert(if sub.is_empty() {
                    p.to_string()
                } else {
                    format!("{sub}/{p}")
                })
            })
            .map(String::as_str)
            .collect();
        tree.copy(&fresh, &dir.join(sub))?;
    }
    Ok(())
}

fn write(dir: &Path, files: &[(String, String)]) -> Result<(), String> {
    for (path, content) in files {
        let to = dir.join(path);
        std::fs::create_dir_all(to.parent().unwrap()).map_err(|e| format!("{}: {e}", to.display()))?;
        std::fs::write(&to, content).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    Ok(())
}

const NEW_PDF: &str = ".redline-new.pdf";
const OLD_PDF: &str = ".redline-old.pdf";
const COMPOSITE: &str = ".redline-side-by-side.typ";
const OLD_SIDE: &str = ".redline-old-side";

/// Run `typst <cmd>` on the project in `dir` with the user's arguments for
/// typst, then `args`.
fn typst(a: &Args, dir: &Path, cmd: &str, args: &[&dyn AsRef<OsStr>]) -> Result<Output, String> {
    Command::new(&a.typst)
        .args([cmd, "--root"])
        .arg(dir)
        .args(&a.typst_args)
        .args(args.iter().map(|a| a.as_ref()))
        .output()
        .map_err(|e| {
            format!(
                "cannot run {}: {e}; install Typst or give its path with --typst",
                a.typst
            )
        })
}

/// Pair the pages of the two compiled sides, each given as (root, entry,
/// side, pdf) with the redline it was made from, and compile the composite
/// that shows them to `to`.
fn compose(
    a: &Args,
    dir: &Path,
    sides: [(&Path, &String, &str, &str); 2],
    labels: (&str, &str),
    results: [&Redline; 2],
    history: &History,
    to: &Path,
) -> Result<(), String> {
    type Layout = (usize, Vec<usize>, Vec<usize>, Vec<Placed>, Vec<Anchor>);
    let layout = |(root, entry, side, _): (&Path, &String, &str, &str), label: &str| -> Result<Layout, String> {
        let input = format!("redline-side={side}");
        let entry = root.join(entry);
        let args: [&dyn AsRef<OsStr>; 7] = [
            &"--input",
            &input,
            &"--field",
            &"value",
            &"--one",
            &entry,
            &"<redline-layout>",
        ];
        let out = typst(a, root, "query", &args)?;
        let nums = ints(&out.stdout);
        let Some([pages, n, rest @ ..]) = nums.as_deref() else {
            return Err(format!(
                "cannot find the pages of the changes; the side-by-side layout needs the change bars that the preamble draws\n{}",
                String::from_utf8_lossy(&out.stderr)
            ));
        };
        let (unbarred, rest) = rest.split_at((*n).min(rest.len()));
        warn_unbarred(unbarred, label);
        let (lost, rest) = match rest {
            [n, rest @ ..] => rest.split_at((*n).min(rest.len())),
            [] => (rest, rest),
        };
        let (mut changes, mut headings) = (vec![], vec![]);
        for &[kind, id, page] in rest.as_chunks::<3>().0 {
            match kind {
                2 => headings.push((id as u64, page)),
                _ => changes.push(Placed {
                    ins: kind == 1,
                    id: id as u64,
                    page,
                }),
            }
        }
        Ok((*pages, unbarred.to_vec(), lost.to_vec(), changes, headings))
    };
    let (
        (old_pages, old_unbarred, old_lost, old, old_headings),
        (new_pages, new_unbarred, new_lost, new, new_headings),
    ) = (layout(sides[0], labels.0)?, layout(sides[1], labels.1)?);
    // A mark that a side did not lay out is listed as not marked, once.
    let mut changes = results[1].changes.clone();
    for (r, lost) in [(results[1], new_lost), (results[0], old_lost)] {
        for c in lost.iter().filter_map(|&i| r.changes.get(i)) {
            let c = c.unmarked();
            let listed = |d: &Change| {
                d.kind == ChangeKind::Unmarked
                    && (d.old == c.text && d.text == c.old || d.old == c.old && d.text == c.text)
            };
            if !changes.iter().any(listed) {
                changes.push(c);
            }
        }
    }
    let (old_pdf, new_pdf) = (format!("/{OLD_PDF}"), format!("/{NEW_PDF}"));
    let source = composite(
        &Side {
            label: labels.0,
            pdf: &old_pdf,
            pages: old_pages,
            unbarred: &old_unbarred,
            changes: &old,
            headings: &old_headings,
        },
        &Side {
            label: labels.1,
            pdf: &new_pdf,
            pages: new_pages,
            unbarred: &new_unbarred,
            changes: &new,
            headings: &new_headings,
        },
        &changes,
        &results[1].bib,
        history,
    );
    write(dir, &[(COMPOSITE.into(), source)])?;
    let out = typst(a, dir, "compile", &[&dir.join(COMPOSITE), &to])?;
    if !out.status.success() {
        return Err(format!(
            "the side-by-side composite failed to compile\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

/// The output of `typst query --field value --one` for a flat array of
/// integers.
fn ints(stdout: &[u8]) -> Option<Vec<usize>> {
    let s = String::from_utf8_lossy(stdout);
    let s = s.trim().strip_prefix('[')?.strip_suffix(']')?.trim();
    if s.is_empty() {
        return Some(vec![]);
    }
    s.split(',').map(|n| n.trim().parse().ok()).collect()
}

fn warn_unbarred(pages: &[usize], label: &str) {
    if !pages.is_empty() {
        let pages: Vec<String> = pages.iter().map(usize::to_string).collect();
        eprintln!(
            "warning: changes on these pages of {label} have no change bars, because the document sets its own page foreground and background there: {}",
            pages.join(", ")
        );
    }
}

/// The version of the typst executable, such as "0.14.2".
fn typst_version(typst: &str) -> Option<String> {
    let out = Command::new(typst).arg("--version").output().ok()?;
    Some(
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .nth(1)?
            .to_string(),
    )
}

/// The major and minor part of a version.
fn minor(v: &str) -> (u32, u32) {
    let mut parts = v.split('.').map(|p| p.parse().unwrap_or(0));
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0))
}

/// A hint if the installed typst differs in minor version from the parser.
fn version_hint(typst: &str) -> Option<String> {
    let installed = typst_version(typst)?;
    let parser = include_str!("../Cargo.toml")
        .lines()
        .find_map(|l| l.strip_prefix("typst-syntax = \""))?
        .trim_end_matches('"');
    (minor(&installed) != minor(parser))
        .then(|| format!("hint: typst {installed} is installed, but typst-redline parses Typst {parser}"))
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Parsed::Run(a)) => a,
        Ok(Parsed::Print(text)) => {
            print!("{text}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: {e}\n");
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let result = if args.mark_reviewed {
        mark_reviewed(&args)
    } else {
        let file = args.files[0].clone();
        since_reviewed(&args).and_then(|reviewed| {
            let pdf = run(*args, reviewed.as_deref())?;
            if reviewed.is_some() {
                eprintln!("hint: to mark the last commit as reviewed, run: typst-redline {file} --mark-reviewed");
            }
            Ok(pdf.display().to_string())
        })
    };
    match result {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{balanced_cuts, reviewed_ref};

    #[test]
    fn cuts_balance_the_weight_of_the_steps() {
        let big_first = [vec![100], vec![1; 9]].concat();
        assert_eq!(balanced_cuts(&big_first, 3), [0, 4]);
        assert_eq!(balanced_cuts(&[1; 10], 3), [2, 5]);
        assert_eq!(balanced_cuts(&[1, 1, 8, 1, 1], 2), [1]);
        assert_eq!(balanced_cuts(&[5, 5], 3), [0]);
        let many: Vec<u64> = (0..4000).map(|i| i % 7 * 10 + 1).collect();
        assert_eq!(balanced_cuts(&many, 6).len(), 5);
    }

    #[test]
    fn reviewed_refs_are_valid_and_distinct() {
        let paths = [
            "doc/main.typ",
            ".hidden/main.typ",
            "x.lock/main.typ",
            "main.typ.lock",
            "dir./main.typ",
            "a..b.typ",
            "my paper.typ",
            "-x.typ",
            "übung.typ",
            "100%.typ",
            "100%25.typ",
        ];
        let names: Vec<String> = paths.iter().map(|p| reviewed_ref(p)).collect();
        for n in &names {
            let ok = std::process::Command::new("git")
                .args(["check-ref-format", n])
                .status()
                .unwrap();
            assert!(ok.success(), "{n}");
        }
        let distinct: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(distinct.len(), names.len());
    }
}
