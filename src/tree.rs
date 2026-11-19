use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

/// The files of one version of a project, addressed by root-relative paths
/// with `/` separators.
pub enum Tree {
    Dir(PathBuf),
    /// Files under `prefix` (relative to the toplevel `repo`, empty or ending
    /// in `/`) at `rev`, or in the index if `rev` is `None`.
    Git {
        repo: PathBuf,
        prefix: String,
        rev: Option<String>,
    },
}

impl Tree {
    pub fn list(&self) -> Result<Vec<String>, String> {
        match self {
            Tree::Dir(root) => {
                let mut out = vec![];
                walk(root, "", &mut out).map_err(|e| format!("{}: {e}", root.display()))?;
                Ok(out)
            }
            Tree::Git { repo, prefix, rev } => match rev {
                Some(rev) => ls(repo, prefix, &["ls-tree", "-r", "-z", "--name-only", rev]),
                None => ls(repo, prefix, &["ls-files", "-z"]),
            },
        }
    }

    /// The working-tree files under the root that git does not track (none
    /// for a directory), as a directory tree and their paths.
    pub fn untracked(&self) -> Result<(Tree, Vec<String>), String> {
        match self {
            Tree::Dir(root) => Ok((Tree::Dir(root.clone()), vec![])),
            Tree::Git { repo, prefix, .. } => Ok((
                Tree::Dir(repo.join(prefix)),
                ls(repo, prefix, &["ls-files", "-z", "--others"])?,
            )),
        }
    }

    /// The files under the root-relative folder `dir` ("" for the root).
    pub fn sub(&self, dir: &str) -> Tree {
        let slash = if dir.is_empty() { "" } else { "/" };
        match self {
            Tree::Dir(root) => Tree::Dir(root.join(dir)),
            Tree::Git { repo, prefix, rev } => Tree::Git {
                repo: repo.clone(),
                prefix: format!("{prefix}{dir}{slash}"),
                rev: rev.clone(),
            },
        }
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        match self {
            Tree::Dir(root) => std::fs::read(root.join(path)).ok(),
            Tree::Git { repo, prefix, rev } => {
                let object = format!("{}:{prefix}{path}", rev.as_deref().unwrap_or(""));
                read_object(repo, &object).ok()?
            }
        }
    }

    /// Put the files at `paths` under `dest`, hard-linked where possible.
    pub fn copy(&self, paths: &[&str], dest: &Path) -> Result<(), String> {
        let target = |p: &str| {
            let to = dest.join(p);
            std::fs::create_dir_all(to.parent().unwrap())?;
            Ok::<_, std::io::Error>(to)
        };
        match self {
            Tree::Dir(root) => paths.iter().try_for_each(|p| {
                let (from, to) = (root.join(p), target(p)?);
                std::fs::hard_link(&from, &to).or_else(|_| std::fs::copy(&from, &to).map(drop))
            }),
            Tree::Git { .. } => self.cat(paths, |p, data| std::fs::write(target(p)?, data)),
        }
        .map_err(|e| format!("staging: {e}"))
    }

    /// Run `f` on each existing file in `paths` with its content, read with
    /// one `git cat-file`.
    fn cat(&self, paths: &[&str], mut f: impl FnMut(&str, Vec<u8>) -> std::io::Result<()>) -> std::io::Result<()> {
        let Tree::Git { repo, prefix, rev } = self else {
            unreachable!()
        };
        let mut child = git(repo)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let input: String = paths
            .iter()
            .map(|p| format!("{}:{prefix}{p}\n", rev.as_deref().unwrap_or("")))
            .collect();
        let mut stdin = child.stdin.take().unwrap();
        let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
        let mut out = BufReader::new(child.stdout.take().unwrap());
        for p in paths {
            if let Some(data) = read_batch_entry(&mut out)? {
                f(p, data)?;
            }
        }
        writer.join().unwrap()?;
        child.wait()?;
        Ok(())
    }
}

type Batch = (ChildStdin, BufReader<ChildStdout>);

/// An object such as `rev:path`, `None` if it does not exist. Reads go
/// through one `git cat-file --batch` per repository, because starting git
/// for each file of each version is slow on some systems.
fn read_object(repo: &Path, object: &str) -> std::io::Result<Option<Vec<u8>>> {
    static BATCHES: Mutex<Vec<(PathBuf, Batch)>> = Mutex::new(vec![]);
    let mut batches = BATCHES.lock().unwrap();
    if !batches.iter().any(|(r, _)| r == repo) {
        let mut child = git(repo)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let io = (
            child.stdin.take().unwrap(),
            BufReader::new(child.stdout.take().unwrap()),
        );
        batches.push((repo.to_path_buf(), io));
    }
    let (stdin, out) = &mut batches.iter_mut().find(|(r, _)| r == repo).unwrap().1;
    writeln!(stdin, "{object}")?;
    read_batch_entry(out)
}

/// One entry of `git cat-file --batch` output: "<oid> <type> <size>" and the
/// content, or "<name> missing".
fn read_batch_entry(out: &mut impl BufRead) -> std::io::Result<Option<Vec<u8>>> {
    let mut header = String::new();
    out.read_line(&mut header)?;
    let Ok(size) = header.trim_end().rsplit(' ').next().unwrap().parse::<usize>() else {
        return Ok(None);
    };
    let mut data = vec![0; size + 1];
    out.read_exact(&mut data)?;
    data.pop();
    Ok(Some(data))
}

/// Paths under `prefix` listed by a git command, relative to `prefix`.
fn ls(repo: &Path, prefix: &str, args: &[&str]) -> Result<Vec<String>, String> {
    let mut cmd = git(repo);
    cmd.args(args)
        .arg("--")
        .arg(if prefix.is_empty() { "." } else { prefix });
    Ok(output(&mut cmd)?
        .split(|&b| b == 0)
        .filter_map(|p| String::from_utf8(p.to_vec()).ok())
        .filter_map(|p| p.strip_prefix(prefix).map(str::to_string))
        .filter(|p| !p.is_empty())
        .collect())
}

fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().to_string();
        let path = format!("{rel}{name}");
        if e.file_type()?.is_dir() {
            if name != ".git" {
                walk(&e.path(), &format!("{path}/"), out)?;
            }
        } else {
            out.push(path);
        }
    }
    Ok(())
}

pub fn git(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir);
    cmd
}

/// Stdout of a command that must succeed.
pub fn output(cmd: &mut Command) -> Result<Vec<u8>, String> {
    let out = cmd.output().map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        let msg = msg.trim().trim_start_matches("fatal: ").trim_start_matches("error: ");
        return Err(format!("git: {msg}"));
    }
    Ok(out.stdout)
}
