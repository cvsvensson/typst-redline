use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        match self {
            Tree::Dir(root) => std::fs::read(root.join(path)).ok(),
            Tree::Git { .. } => {
                let mut data = None;
                self.cat(&[path], |_, d| {
                    data = Some(d);
                    Ok(())
                })
                .ok()?;
                data
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
            let mut header = String::new();
            out.read_line(&mut header)?;
            // "<oid> <type> <size>", or "<name> missing".
            let Ok(size) = header.trim_end().rsplit(' ').next().unwrap().parse::<usize>() else {
                continue;
            };
            let mut data = vec![0; size + 1];
            out.read_exact(&mut data)?;
            data.pop();
            f(p, data)?;
        }
        writer.join().unwrap()?;
        child.wait()?;
        Ok(())
    }
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
