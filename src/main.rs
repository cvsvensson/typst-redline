use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
typst-redline: mark changes between two versions of a Typst document

Usage:
  typst-redline <file.typ> --since <rev> [--to <rev>] [options]
  typst-redline <old.typ> <new.typ> [options]

With --since, the old version is <file.typ> at git revision <rev> and the new
version is the working copy (or <file.typ> at --to <rev>).

Options:
  -o, --output <pdf>    Output PDF (default: <name>.redline.pdf next to the new file)
  --root <dir>          Project root, passed to `typst compile --root`
  --preamble <file>     Replace the built-in change styling
  --typst <path>        typst executable (default: typst)
  --keep-typ            Keep the generated .typ file next to the input
";

struct Args {
    files: Vec<PathBuf>,
    since: Option<String>,
    to: Option<String>,
    output: Option<PathBuf>,
    root: Option<PathBuf>,
    preamble: Option<PathBuf>,
    typst: String,
    keep: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        files: vec![],
        since: None,
        to: None,
        output: None,
        root: None,
        preamble: None,
        typst: "typst".into(),
        keep: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut val = || it.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "-h" | "--help" => return Err(String::new()),
            "--since" => a.since = Some(val()?),
            "--to" => a.to = Some(val()?),
            "-o" | "--output" => a.output = Some(val()?.into()),
            "--root" => a.root = Some(val()?.into()),
            "--preamble" => a.preamble = Some(val()?.into()),
            "--typst" => a.typst = val()?,
            "--keep-typ" => a.keep = true,
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => a.files.push(arg.into()),
        }
    }
    match (a.files.len(), &a.since) {
        (1, Some(_)) | (2, None) => Ok(a),
        _ => Err("give one file with --since, or two files".into()),
    }
}

fn git_show(file: &Path, rev: &str) -> Result<String, String> {
    let dir = file
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = file.file_name().ok_or("bad file name")?.to_string_lossy();
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("show")
        .arg(format!("{rev}:./{name}"))
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    String::from_utf8(out.stdout).map_err(|_| "file is not UTF-8".into())
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Deletes the generated source on drop unless asked to keep it.
struct TempFile(PathBuf, bool);

impl Drop for TempFile {
    fn drop(&mut self) {
        if !self.1 {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

fn run(a: Args) -> Result<PathBuf, String> {
    let new_path = a.files.last().unwrap().clone();
    let (old, new) = match &a.since {
        Some(rev) => {
            let new = match &a.to {
                Some(to) => git_show(&new_path, to)?,
                None => read(&new_path)?,
            };
            (git_show(&new_path, rev)?, new)
        }
        None => (read(&a.files[0])?, read(&new_path)?),
    };
    let preamble = match &a.preamble {
        Some(p) => read(p)?,
        None => typst_redline::PREAMBLE.to_string(),
    };
    let src = typst_redline::redline(&old, &new, &preamble);

    // The generated source sits next to the new file so that relative
    // imports, images and the bibliography resolve as in the original.
    let stem = new_path
        .file_stem()
        .ok_or("bad file name")?
        .to_string_lossy()
        .to_string();
    let tmp = new_path.with_file_name(format!("{stem}.redline-tmp.typ"));
    let pdf = a
        .output
        .clone()
        .unwrap_or_else(|| new_path.with_file_name(format!("{stem}.redline.pdf")));
    std::fs::write(&tmp, src).map_err(|e| format!("{}: {e}", tmp.display()))?;
    let _guard = TempFile(tmp.clone(), a.keep);

    let mut cmd = Command::new(&a.typst);
    cmd.arg("compile");
    if let Some(root) = &a.root {
        cmd.arg("--root").arg(root);
    }
    let status = cmd
        .arg(&tmp)
        .arg(&pdf)
        .status()
        .map_err(|e| format!("cannot run {}: {e}", a.typst))?;
    if !status.success() {
        let hint = if a.keep {
            ""
        } else {
            " (rerun with --keep-typ to inspect the generated source)"
        };
        return Err(format!("typst compile failed{hint}"));
    }
    if a.keep {
        eprintln!("kept {}", tmp.display());
    }
    Ok(pdf)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("error: {e}\n");
            }
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
