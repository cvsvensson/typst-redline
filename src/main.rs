mod tree;

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};

use tree::{Tree, git, output};
use typst_redline::{
    Anchor, Commit, History, OLD_DIR, Options, PREAMBLE, PREAMBLE_PATH, Placed, Redline, Side, Version, composite,
    project_files, redline,
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
  --typst <path>        typst executable (default: typst)
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
    each_commit: bool,
    typst_args: Vec<String>,
}

enum Parsed {
    Run(Args),
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
        each_commit: false,
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
            "--each-commit" => a.each_commit = true,
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
    if a.root.is_none() {
        a.root = std::env::var_os("TYPST_ROOT")
            .filter(|r| !r.is_empty())
            .map(PathBuf::from);
        a.root_env = a.root.is_some();
    }
    if a.each_commit && a.files.len() != 1 {
        return Err("--each-commit works with one file, not with two".into());
    }
    match (a.files.len(), &a.since, &a.to) {
        (1, _, _) | (2, None, None) => Ok(Parsed::Run(a)),
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

fn specs<'a>(a: &'a Args) -> Result<(Spec, Spec), String> {
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
    let since = a.since.as_deref().unwrap_or("HEAD");
    // As in git, an omitted end of a range is HEAD.
    let head = |r: &'a str| if r.is_empty() { "HEAD" } else { r };
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
    let mut old = spec(file, Some(&old), root, old.clone())?;
    if old.tree.read(&old.entry).is_none()
        && let Some(path) = renamed(&old, to)
    {
        old.entry = path;
    }
    Ok((old, new))
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

/// Each level is coarser than the one before: marks inside code can break
/// functions that inspect their body, and deleted code can fail (the old
/// version it is evaluated in fails on its own).
const LADDER: [(Options, &str); 3] = [
    (
        Options {
            eval_deleted: true,
            content_blocks: true,
        },
        "",
    ),
    (
        Options {
            eval_deleted: false,
            content_blocks: true,
        },
        "deleted code is shown by name",
    ),
    (
        Options {
            eval_deleted: false,
            content_blocks: false,
        },
        "deleted code is shown by name and changed code is marked whole",
    ),
];

fn run(a: Args) -> Result<PathBuf, String> {
    let preamble = match &a.preamble {
        Some(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let needs = [
                (a.side_by_side, "redline-side-by-side", "the side-by-side layout"),
                (a.each_commit, "redline-step-style", "--each-commit"),
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
    let (old, new) = specs(&a).map_err(|e| match a.root_env && e.contains("the root") {
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
    let text = |spec: &Spec, path: &str| String::from_utf8(spec.tree.read(path)?).ok();
    let (read_old, read_new) = (|p: &str| text(&old, p), |p: &str| text(&new, p));
    let version = |spec: &Spec, read| Version {
        label: spec.label.clone(),
        entry: spec.entry.clone(),
        read,
    };
    let (old_v, new_v) = (version(&old, &read_old), version(&new, &read_new));
    let files = [project_files(&old_v), project_files(&new_v)].concat();
    let log = log(&old, &new, &files)?;
    let step_trees = match &log {
        Some(log) if a.each_commit => step_trees(log, &old, &new)?,
        _ if a.each_commit => {
            return Err("--each-commit needs two git revisions, or a revision and the working tree".into());
        }
        _ => vec![],
    };
    // A step before the entry was renamed has it under its old name.
    let step_entry = |t: &Tree| {
        if t.read(&new.entry).is_some() {
            new.entry.clone()
        } else {
            old.entry.clone()
        }
    };
    let step_reads: Vec<_> = step_trees
        .iter()
        .map(|t| move |p: &str| String::from_utf8(t.read(p)?).ok())
        .collect();
    let history = History {
        steps: a.each_commit.then(|| {
            step_trees
                .iter()
                .zip(&step_reads)
                .map(|(t, read)| Version {
                    label: String::new(),
                    entry: step_entry(t),
                    read,
                })
                .collect()
        }),
        ..log.map(|l| l.history).unwrap_or_default()
    };

    let mut result = redline(&old_v, &new_v, LADDER[0].0, &history);
    for w in &result.warnings {
        eprintln!("warning: {w}");
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
    let side = |(root, entry, side, pdf): (&Path, &String, &str, &str)| {
        let input = format!("redline-side={side}");
        typst(
            &a,
            root,
            "compile",
            &[&"--input", &input, &root.join(entry), &dir.join(pdf)],
        )
    };
    for (i, (opts, note)) in LADDER.into_iter().enumerate() {
        if i > 0 {
            result = redline(&old_v, &new_v, opts, &history);
        }
        write(dir, &result.files)?;
        let out = if !a.side_by_side {
            typst(&a, dir, "compile", &[&dir.join(&new.entry), &tmp])?
        } else {
            let back_history = History {
                steps: history.steps.clone(),
                swapped: true,
                ..History::default()
            };
            let back = redline(&new_v, &old_v, opts, &back_history);
            if i == 0 {
                stage(&old_root, &new, &old, &back)?;
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
            if a.side_by_side {
                compose(&a, dir, sides, (&old.label, &new.label), &result, &history, &tmp)?;
            } else if ["foreground", "background"]
                .iter()
                .all(|w| result.files.iter().any(|(_, s)| s.contains(w)))
            {
                // Bars are lost only where the document sets both, and asking
                // where costs another compile. The summary page lists such
                // pages in any case.
                let args: [&dyn AsRef<OsStr>; 5] = [
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
    let changed = |args: &[&str]| -> Result<bool, String> {
        Ok(!output(git(repo).args(args).arg("--").args(&paths))?.is_empty())
    };
    let (to, uncommitted) = match &new.tree {
        Tree::Git {
            repo: r,
            prefix: p,
            rev,
        } if r == repo && p == prefix => match rev {
            Some(to) => (to.as_str(), false),
            None => ("HEAD", changed(&["diff", "--cached", "--name-only", "HEAD"])?),
        },
        Tree::Dir(root) if same_dir(root, &repo.join(prefix)) => {
            ("HEAD", changed(&["status", "--porcelain", "--untracked-files=no"])?)
        }
        _ => return Ok(None),
    };
    let out = output(
        git(repo)
            .args([
                "log",
                "--first-parent",
                "--reverse",
                "--date=short",
                "--format=%H%x1f%h%x1f%ad%x1f%s",
                &format!("{from}..{to}"),
                "--",
            ])
            .args(&paths),
    )?;
    let (mut hashes, mut commits) = (vec![], vec![]);
    for line in String::from_utf8_lossy(&out).lines() {
        let f: Vec<&str> = line.splitn(4, '\x1f').collect();
        if let [full, hash, date, subject] = f[..] {
            hashes.push(full.to_string());
            commits.push(Commit {
                hash: hash.into(),
                date: date.into(),
                subject: subject.into(),
            });
        }
    }
    Ok(Some(Log {
        to: to.into(),
        hashes,
        history: History {
            commits,
            uncommitted,
            ..History::default()
        },
    }))
}

/// Each change is coloured by its step: a commit, or the uncommitted
/// changes. The trees after each step but the last, which is the new
/// version.
fn step_trees(log: &Log, old: &Spec, new: &Spec) -> Result<Vec<Tree>, String> {
    const MAX: usize = 6;
    let Tree::Git {
        repo,
        prefix,
        rev: Some(from),
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
            "--each-commit needs {} to be an ancestor of {}; to start where they diverge, give --since {}...{}",
            old.label, new.label, old.label, log.to
        ));
    }
    let h = &log.history;
    let steps = h.commits.len() + usize::from(h.uncommitted);
    if steps == 0 {
        return Err(format!(
            "no commits from {} to {} change the document, so --each-commit has nothing to colour",
            old.label, new.label
        ));
    }
    if steps > MAX {
        return Err(format!(
            "--each-commit colours at most {MAX} commits, but {steps} commits from {} to {} change the document{}; give a narrower range, such as --since {}",
            old.label,
            new.label,
            if h.uncommitted {
                " (counting the uncommitted changes)"
            } else {
                ""
            },
            h.commits[steps - MAX - 1].hash
        ));
    }
    Ok(log.hashes[..steps - 1]
        .iter()
        .map(|rev| Tree::Git {
            repo: repo.clone(),
            prefix: prefix.clone(),
            rev: Some(rev.clone()),
        })
        .collect())
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
/// side, pdf), and compile the composite that shows them to `to`.
fn compose(
    a: &Args,
    dir: &Path,
    sides: [(&Path, &String, &str, &str); 2],
    labels: (&str, &str),
    result: &Redline,
    history: &History,
    to: &Path,
) -> Result<(), String> {
    type Layout = (usize, Vec<usize>, Vec<Placed>, Vec<Anchor>);
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
        Ok((*pages, unbarred.to_vec(), changes, headings))
    };
    let ((old_pages, old_unbarred, old, old_headings), (new_pages, new_unbarred, new, new_headings)) =
        (layout(sides[0], labels.0)?, layout(sides[1], labels.1)?);
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
        &result.changes,
        &result.bib,
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
    match run(args) {
        Ok(pdf) => {
            println!("{}", pdf.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
