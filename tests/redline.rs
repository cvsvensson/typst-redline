use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use typst_redline::{Change, ChangeKind, Options, PREAMBLE, Version, redline};

const FIXTURES: [&str; 17] = [
    "inline-math",
    "display-label",
    "deleted-label",
    "crlf",
    "sections",
    "nested-list",
    "literal-underscore",
    "line-break",
    "nbsp",
    "list-first",
    "list-marker",
    "enum-deleted",
    "label-moved",
    "cjk",
    "name-collision",
    "punctuation",
    "deleted-first",
];

/// A fixture side: `<side>.typ` as `main.typ`, or the tree `<side>/` with
/// the entry `main.typ`.
fn read_fixture(name: &str, side: &str, path: &str) -> Option<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    let file = if dir.join(side).is_dir() {
        dir.join(side).join(path)
    } else if path == "main.typ" {
        dir.join(format!("{side}.typ"))
    } else {
        return None;
    };
    std::fs::read_to_string(file).ok()
}

struct Run {
    /// The marked entry file.
    body: String,
    /// Every file of the marked project.
    files: Vec<(String, String)>,
    changes: Vec<Change>,
}

impl Run {
    fn file(&self, path: &str) -> &str {
        &self.files.iter().find(|(p, _)| p == path).unwrap().1
    }
}

/// The files of a fixture side: the tree `<side>/`, or `<side>.typ` as
/// `main.typ`.
fn tree(name: &str, side: &str) -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    match std::fs::read_dir(dir.join(side)) {
        Ok(entries) => entries
            .map(|e| {
                let e = e.unwrap();
                let p = e.file_name().into_string().unwrap();
                (p, std::fs::read_to_string(e.path()).unwrap())
            })
            .collect(),
        Err(_) => vec![("main.typ".into(), read_fixture(name, side, "main.typ").unwrap())],
    }
}

fn run_with(name: &str, eval_deleted: bool, content_blocks: bool) -> Run {
    generate(
        name,
        Options {
            preamble: PREAMBLE.into(),
            eval_deleted,
            content_blocks,
        },
    )
}

fn generate(name: &str, opts: Options) -> Run {
    let (read_old, read_new) = (
        |p: &str| read_fixture(name, "old", p),
        |p: &str| read_fixture(name, "new", p),
    );
    let v = |label: &str, read| Version {
        label: label.into(),
        entry: "main.typ".into(),
        read,
    };
    let r = redline(&v("old", &read_old), &v("new", &read_new), &opts);
    // The new tree, the old-only files, then the overlay.
    let mut files = tree(name, "new");
    for f in tree(name, "old") {
        if !files.iter().any(|(q, _)| *q == f.0) {
            files.push(f);
        }
    }
    for (p, c) in r.files {
        files.retain(|(q, _)| *q != p);
        files.push((p, c));
    }
    Run {
        body: files.iter().find(|(p, _)| p == "main.typ").unwrap().1.clone(),
        files,
        changes: r.changes,
    }
}

fn run(name: &str) -> Run {
    run_with(name, true, true)
}

fn body(name: &str) -> String {
    run(name).body
}

#[test]
fn inline_math_is_atomic() {
    let out = body("inline-math");
    assert!(out.contains("[$T_R^((N))$]"), "{out}");
    assert!(out.contains("[$T_R^((N,k))$]"), "{out}");
}

#[test]
fn label_stays_on_changed_equation() {
    let out = body("display-label");
    assert_eq!(out.matches("<eq-energy>").count(), 1, "{out}");
    assert!(out.contains("$ E = m c^2 + p^2 c^2 $ <eq-energy>]"), "{out}");
}

#[test]
fn deleted_labels_and_refs_are_neutralized() {
    let out = body("deleted-label");
    assert!(!out.contains("<eq-old>"), "{out}");
    assert!(!out.contains(" @eq-old"), "{out}");
    assert!(out.contains("\\@eq-old"), "{out}");
    assert!(out.contains("and to @sec-intro"), "{out}");
}

#[test]
fn line_endings_are_not_changes() {
    let r = run("crlf");
    assert!(r.changes.is_empty(), "{}", r.body);
}

#[test]
fn changes_are_recorded() {
    let kinds: Vec<ChangeKind> = run("inline-math").changes.iter().map(|c| c.kind).collect();
    assert!(kinds.contains(&ChangeKind::Ins) && kinds.contains(&ChangeKind::Del));
}

#[test]
fn changes_stay_inside_nested_items() {
    let out = body("nested-list");
    assert!(out.contains("\n  - Nested one #redline-ins"), "{out}");
    assert!(
        out.contains("\n  - Nested two\n    #redline-del(block: true)[Nested gone];\n- Third item"),
        "{out}"
    );
    assert!(out.contains("\n/ Term: A #redline-ins"), "{out}");
}

#[test]
fn summary_comes_first_at_the_top_level() {
    for name in ["list-first", "deleted-first"] {
        let out = body(name);
        let at = out.find("#redline-summary(").unwrap();
        assert!(
            at < out.find("#redline-del(").unwrap() && out[..at].ends_with('\n'),
            "{out}"
        );
    }
}

#[test]
fn spaces_belong_to_the_new_version() {
    let out = body("punctuation");
    assert!(
        out.contains("The #redline-del()[extension is given in closed form;];#redline-ins()[extension, the Procrustes map,]; see"),
        "{out}"
    );
}

#[test]
fn unspaced_scripts_are_compared_by_character() {
    let out = body("cjk");
    assert!(
        out.contains("日本語#redline-ins()[の];テキスト"),
        "{out}"
    );
}

fn typst_available() -> bool {
    let ok = Command::new("typst").arg("--version").output().is_ok();
    if !ok {
        eprintln!("typst not found, skipping");
    }
    ok
}

/// A fresh directory, removed on drop.
struct TempDir(PathBuf);

impl std::ops::Deref for TempDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(name: &str) -> TempDir {
    let dir = std::env::temp_dir().join(format!("typst-redline-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}

/// Compile a marked project; `None` without typst.
fn compile(name: &str, files: &[(String, String)]) -> Option<TempDir> {
    compile_to(name, files, "main.pdf")
}

/// The pages of a project rendered as PNG images.
fn render(name: &str, files: &[(String, String)]) -> Option<Vec<Vec<u8>>> {
    let dir = compile_to(name, files, "page-{0p}.png")?;
    let mut pages: Vec<PathBuf> = std::fs::read_dir(&*dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
        .collect();
    pages.sort();
    Some(pages.iter().map(|p| std::fs::read(p).unwrap()).collect())
}

fn compile_to(name: &str, files: &[(String, String)], output: &str) -> Option<TempDir> {
    if !typst_available() {
        return None;
    }
    let dir = temp_dir(name);
    for (p, c) in files {
        std::fs::write(dir.join(p), c).unwrap();
    }
    let out = Command::new("typst")
        .arg("compile")
        .arg("--root")
        .arg(&*dir)
        .arg(dir.join("main.typ"))
        .arg(dir.join(output))
        .output()
        .unwrap();
    assert!(out.status.success(), "{name}: {}", String::from_utf8_lossy(&out.stderr));
    Some(dir)
}

/// The value of the metadata labelled `<probe>` in a compiled fixture.
fn probe(name: &str) -> Option<String> {
    let dir = compile(name, &run(name).files)?;
    let out = Command::new("typst")
        .args(["query", "--root"])
        .arg(&*dir)
        .arg(dir.join("main.typ"))
        .args(["<probe>", "--field", "value", "--one"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    Some(String::from_utf8_lossy(&out.stdout).split_whitespace().collect())
}

#[test]
fn output_compiles() {
    for name in FIXTURES {
        compile(name, &run(name).files);
    }
}

/// Renders insertions as plain content and deletions as nothing.
const PLAIN: &str = "#let redline-ins(body, ..args) = body
#let redline-del(..args) = none
#let redline-summary(info) = none
#let redline-bars = none
";

#[test]
fn proof_without_deletions_is_the_new_document() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for e in std::fs::read_dir(dir).unwrap() {
        let name = e.unwrap().file_name().into_string().unwrap();
        let opts = Options {
            preamble: PLAIN.into(),
            eval_deleted: false,
            content_blocks: true,
        };
        let r = generate(&name, opts);
        assert!(name == "crlf" || !r.changes.is_empty(), "{name}: no change found");
        let Some(proof) = render(&format!("{name}-proof"), &r.files) else {
            return;
        };
        let new = render(&format!("{name}-new"), &tree(&name, "new")).unwrap();
        assert!(!new.is_empty() && proof == new, "{name}:\n{}", r.body);
    }
}

#[test]
fn deleted_code_shown_as_source_is_not_evaluated() {
    compile("deleted-code", &run_with("deleted-code", false, true).files);
}

const WORD_CHANGE: &str = "#redline-del()[alpha";

#[test]
fn change_inside_included_file_is_marked() {
    let r = run("include-change");
    assert!(r.file("part.typ").contains(WORD_CHANGE), "{}", r.file("part.typ"));
    assert!(r.changes.iter().all(|c| c.file == "part.typ"));
    compile("include-change", &r.files);
}

#[test]
fn deleted_include_is_shown_struck() {
    let r = run("include-deleted");
    assert!(
        r.body.contains("#redline-del(block: true)[#include \"/gone.typ\"]"),
        "{}",
        r.body
    );
    compile("include-deleted", &r.files);
}

#[test]
fn renamed_included_file_gets_word_marks() {
    let r = run("include-renamed");
    assert!(r.file("b.typ").contains(WORD_CHANGE), "{:?}", r.files);
}

#[test]
fn caption_change_does_not_duplicate_the_figure() {
    let r = run("caption");
    assert_eq!(r.body.matches("#figure(").count(), 1, "{}", r.body);
    assert!(
        r.body.contains("#redline-del()[red];"),
        "{}",
        r.body
    );
    compile("caption", &r.files);
}

#[test]
fn changed_content_variable_in_imported_file_is_marked() {
    let r = run("imported-content");
    assert!(r.file("defs.typ").contains(WORD_CHANGE), "{}", r.file("defs.typ"));
    compile("imported-content", &r.files);
}

#[test]
fn changed_code_in_imported_file_is_listed_as_not_marked() {
    let r = run("imported-code");
    let kinds: Vec<_> = r.changes.iter().map(|c| (c.file.as_str(), c.kind)).collect();
    assert_eq!(kinds, [("defs.typ", ChangeKind::Unmarked)]);
    assert!(r.body.contains("unmarked: true"), "{}", r.body);
}

#[test]
fn deleted_labelled_equation_does_not_break_references() {
    compile("moved-label", &run("moved-label").files);
}

#[test]
fn deleted_footnote_does_not_step_the_counter() {
    if let Some(v) = probe("deleted-footnote") {
        assert_eq!(v, "1");
    }
}

#[test]
fn deleted_state_and_counter_updates_have_no_effect() {
    if let Some(v) = probe("deleted-update") {
        assert_eq!(v, "[0,1]");
    }
}

#[test]
fn deleted_content_sees_the_new_definitions() {
    compile("changed-definition", &run("changed-definition").files);
}

#[test]
fn computed_include_path_is_reported() {
    let read = |_: &str| Some("#include \"a\" + \".typ\"\n".to_string());
    let v = Version {
        label: "v".into(),
        entry: "main.typ".into(),
        read: &read,
    };
    let opts = Options {
        preamble: PREAMBLE.into(),
        eval_deleted: true,
        content_blocks: true,
    };
    assert_eq!(redline(&v, &v, &opts).warnings.len(), 1);
}

// CLI

fn cli(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_typst-redline"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

/// The generated entry file of a successful run with `--keep-typ`.
fn marked(dir: &Path, args: &[&str], entry: &str) -> String {
    let out = cli(dir, &[args, &["--keep-typ"]].concat());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    let kept = stderr.lines().find_map(|l| l.strip_prefix("kept ")).unwrap();
    let src = std::fs::read_to_string(Path::new(kept).join(entry)).unwrap();
    std::fs::remove_dir_all(kept).unwrap();
    src
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").current_dir(dir).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// A repository whose `doc/main.typ` says `word` in commit `c-<word>`.
fn repo(name: &str, words: &[&str]) -> TempDir {
    let dir = temp_dir(name);
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "t"]);
    git(&dir, &["config", "user.email", "t@t"]);
    git(&dir, &["config", "core.autocrlf", "false"]);
    std::fs::create_dir(dir.join("doc")).unwrap();
    for w in words {
        commit(&dir, "doc/main.typ", w);
    }
    dir
}

fn doc(word: &str) -> String {
    format!("= Title\n\nThe first paragraph stays the same.\n\nThe word is {word}.\n\nThe last paragraph stays too.\n")
}

fn commit(dir: &Path, path: &str, word: &str) {
    std::fs::write(dir.join(path), doc(word)).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", word]);
    git(dir, &["tag", &format!("c-{word}")]);
}

fn assert_compared(src: &str, old: &str, new: &str) {
    assert!(
        src.contains(&format!("#redline-del()[{old}")),
        "{src}"
    );
    assert!(
        src.contains(&format!("#redline-ins()[{new}")),
        "{src}"
    );
}

#[test]
fn working_copy_is_compared_with_head_by_default() {
    if !typst_available() {
        return;
    }
    let dir = repo("default-head", &["alpha", "beta"]);
    std::fs::write(dir.join("doc/main.typ"), doc("gamma")).unwrap();
    assert_compared(&marked(&dir.join("doc"), &["main.typ"], "main.typ"), "beta", "gamma");
}

#[test]
fn double_dot_range_compares_two_revisions() {
    if !typst_available() {
        return;
    }
    let dir = repo("range", &["alpha", "beta", "gamma"]);
    let src = marked(&dir, &["doc/main.typ", "--since", "c-alpha..c-beta"], "main.typ");
    assert_compared(&src, "alpha", "beta");
}

#[test]
fn triple_dot_range_starts_at_the_merge_base() {
    if !typst_available() {
        return;
    }
    let dir = repo("merge-base", &["alpha", "beta"]);
    git(&dir, &["checkout", "-q", "-b", "side", "c-alpha"]);
    commit(&dir, "doc/main.typ", "gamma");
    let src = marked(&dir, &["doc/main.typ", "--since", "main...side"], "main.typ");
    assert_compared(&src, "alpha", "gamma");
}

#[test]
fn rev_path_arguments_name_versions() {
    if !typst_available() {
        return;
    }
    let dir = repo("rev-path", &["alpha", "beta"]);
    std::fs::write(dir.join("doc/main.typ"), doc("gamma")).unwrap();
    git(&dir, &["add", "-A"]);
    let src = marked(&dir, &["c-alpha:doc/main.typ", ":doc/main.typ"], "main.typ");
    assert_compared(&src, "alpha", "gamma");
}

#[test]
fn renamed_file_is_compared_with_its_old_name() {
    if !typst_available() {
        return;
    }
    let dir = repo("rename", &["alpha"]);
    git(&dir, &["mv", "doc/main.typ", "doc/paper.typ"]);
    std::fs::write(dir.join("doc/paper.typ"), doc("beta")).unwrap();
    let src = marked(&dir, &["doc/paper.typ"], "paper.typ");
    assert_compared(&src, "alpha", "beta");
}

#[test]
fn files_of_the_to_revision_are_used() {
    if !typst_available() {
        return;
    }
    let dir = repo("to-tree", &["alpha"]);
    std::fs::write(dir.join("doc/main.typ"), "#include \"part.typ\"\n").unwrap();
    std::fs::write(dir.join("doc/part.typ"), "Part.\n").unwrap();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "include"]);
    std::fs::remove_file(dir.join("doc/part.typ")).unwrap();
    let out = cli(&dir, &["doc/main.typ", "--since", "c-alpha", "--to", "HEAD"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn untracked_files_belong_to_every_revision() {
    if !typst_available() {
        return;
    }
    let dir = repo("untracked", &["alpha"]);
    std::fs::write(dir.join("doc/fig.svg"), SVG).unwrap();
    std::fs::write(
        dir.join("doc/main.typ"),
        format!("{}\n#image(\"fig.svg\")\n", doc("beta")),
    )
    .unwrap();
    git(&dir, &["add", "doc/main.typ"]);
    git(&dir, &["commit", "-q", "-m", "figure"]);
    let out = cli(&dir, &["doc/main.typ", "--since", "c-alpha..HEAD"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn code_that_inspects_marked_content_is_marked_whole() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("inspects-body");
    let doc = |w: &str| format!("#let f(b) = upper(b.text)\n\n#f[{w} word]\n");
    std::fs::write(dir.join("old.typ"), doc("alpha")).unwrap();
    std::fs::write(dir.join("new.typ"), doc("beta")).unwrap();
    let out = cli(&dir, &["old.typ", "new.typ"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && stderr.contains("marked whole"), "{stderr}");
}

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#;

#[test]
fn deleted_figure_whose_file_is_gone_is_evaluated() {
    if !typst_available() {
        return;
    }
    let dir = repo("old-asset", &["alpha"]);
    std::fs::write(dir.join("doc/fig.svg"), SVG).unwrap();
    std::fs::write(
        dir.join("doc/main.typ"),
        format!("{}\n#figure(image(\"fig.svg\"))\n", doc("alpha")),
    )
    .unwrap();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "figure"]);
    std::fs::remove_file(dir.join("doc/fig.svg")).unwrap();
    std::fs::write(dir.join("doc/main.typ"), doc("alpha")).unwrap();
    let out = cli(&dir, &["doc/main.typ"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && !stderr.contains("shown as source"), "{stderr}");
}

#[test]
fn only_the_pdf_is_written_next_to_the_sources() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("clean");
    std::fs::write(dir.join("old.typ"), doc("alpha")).unwrap();
    std::fs::write(dir.join("new.typ"), doc("beta")).unwrap();
    let out = cli(&dir, &["old.typ", "new.typ"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        dir.join("new.redline.pdf").display().to_string()
    );
    let mut files: Vec<_> = std::fs::read_dir(&*dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    files.sort();
    assert_eq!(files, ["new.redline.pdf", "new.typ", "old.typ"]);
}

#[test]
fn bad_paths_are_reported_before_compiling() {
    let dir = temp_dir("bad-paths");
    std::fs::write(dir.join("old.typ"), doc("alpha")).unwrap();
    std::fs::write(dir.join("new.typ"), doc("beta")).unwrap();
    for (args, msg) in [
        (&["old.typ", "new.typ", "-o", "new.typ"][..], "is an input"),
        (&["old.typ", "new.typ", "-o", "none/x.pdf"], "does not exist"),
        (&["old.typ", "gone.typ"], "does not exist"),
        (&["old.typ", "new.typ", "--root", "none"], "not a directory"),
    ] {
        let stderr = String::from_utf8_lossy(&cli(&dir, args).stderr).to_string();
        assert!(
            stderr.contains(msg) && !stderr.contains("typst compile"),
            "{args:?}: {stderr}"
        );
    }
    assert_eq!(std::fs::read_to_string(dir.join("new.typ")).unwrap(), doc("beta"));
}

#[test]
fn unknown_revision_is_reported() {
    let dir = repo("unknown-rev", &["alpha"]);
    let out = cli(&dir, &["doc/main.typ", "--since", "nope"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no commit nope") && !stderr.contains("fatal"),
        "{stderr}"
    );
}

#[test]
fn arguments_after_double_dash_go_to_typst() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("pass-through");
    std::fs::write(dir.join("old.typ"), doc("alpha")).unwrap();
    let new = format!(
        "#assert.eq(sys.inputs.at(\"v\", default: none), \"1\")\n{}",
        doc("beta")
    );
    std::fs::write(dir.join("new.typ"), new).unwrap();
    let out = cli(&dir, &["old.typ", "new.typ", "--", "--input", "v=1"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}
