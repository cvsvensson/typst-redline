mod tree;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use tree::{Tree, git, output};
use typst_redline::{OLD_DIR, Options, PREAMBLE, PREAMBLE_PATH, Redline, Version, redline};

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
  --root <dir>          Project root (default: the directory of each file)
  --preamble <file>     Replace the built-in change styling
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
    preamble: Option<PathBuf>,
    typst: String,
    keep: bool,
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
        preamble: None,
        typst: "typst".into(),
        keep: false,
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
            "--" => a.typst_args.extend(it.by_ref()),
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => a.files.push(arg),
        }
    }
    if a.to.is_some() && a.since.as_deref().is_some_and(|s| s.contains("..")) {
        return Err("--to cannot be combined with a range in --since".into());
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
    let entry = file
        .strip_prefix(&root)
        .map_err(|_| format!("{} is not inside the root {}", file.display(), root.display()))?
        .iter()
        .map(|c| c.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    let tree = match rev {
        None => Tree::Dir(root),
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
    let (old, new) = specs(&a)?;
    if new.tree.read(&new.entry).is_none() {
        return Err(match new.tree {
            Tree::Dir(_) => format!("{} does not exist", new.file.display()),
            Tree::Git { .. } => format!("{} does not exist in {}", new.entry, new.label),
        });
    }
    let preamble = match &a.preamble {
        Some(p) => std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
        None => PREAMBLE.to_string(),
    };
    let pdf = output_path(&a, &old, &new)?;
    let text = |spec: &Spec, path: &str| String::from_utf8(spec.tree.read(path)?).ok();
    let (read_old, read_new) = (|p: &str| text(&old, p), |p: &str| text(&new, p));
    let version = |spec: &Spec, read| Version {
        label: spec.label.clone(),
        entry: spec.entry.clone(),
        read,
    };
    let (old_v, new_v) = (version(&old, &read_old), version(&new, &read_new));

    let mut result = redline(&old_v, &new_v, LADDER[0].0);
    for w in &result.warnings {
        eprintln!("warning: {w}");
    }
    let staging = Staging::new(a.keep);
    let dir = &staging.dir;
    stage(dir, &old, &new, &result)?;
    write(dir, &[(PREAMBLE_PATH.into(), preamble)])?;
    // typst writes next to the staging directory, so that an output that
    // cannot be written is reported as such and is never left half written.
    let tmp = dir.with_extension(pdf.extension().unwrap_or("pdf".as_ref()));
    let mut first_error = String::new();
    let mut stderr = String::new();
    for (i, (opts, note)) in LADDER.into_iter().enumerate() {
        if i > 0 {
            result = redline(&old_v, &new_v, opts);
        }
        write(dir, &result.files)?;
        let out = Command::new(&a.typst)
            .arg("compile")
            .arg("--root")
            .arg(dir)
            .args(&a.typst_args)
            .arg(dir.join(&new.entry))
            .arg(&tmp)
            .output()
            .map_err(|e| {
                format!(
                    "cannot run {}: {e}; install Typst or give its path with --typst",
                    a.typst
                )
            })?;
        stderr = String::from_utf8_lossy(&out.stderr).to_string();
        if out.status.success() {
            eprint!("{stderr}");
            if !note.is_empty() {
                eprintln!("note: {note}, because the full proof failed to compile ({first_error})");
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
    if a.root.is_none() && stderr.contains("file not found") {
        eprintln!(
            "hint: paths starting with / resolve against the directory of the file; set another root with --root"
        );
    }
    let hint = if a.keep {
        ""
    } else {
        " (rerun with --keep-typ to inspect the generated source)"
    };
    Err(format!("typst compile failed{hint}"))
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

/// A hint if the installed typst differs in minor version from the parser.
fn version_hint(typst: &str) -> Option<String> {
    let out = Command::new(typst).arg("--version").output().ok()?;
    let installed = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)?
        .to_string();
    let parser = include_str!("../Cargo.toml")
        .lines()
        .find_map(|l| l.strip_prefix("typst-syntax = \""))?
        .trim_end_matches('"');
    let minor = |v: &str| v.split('.').take(2).collect::<Vec<_>>().join(".");
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
