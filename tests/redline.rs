use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use typst_redline::{
    Bib, Change, ChangeKind, OLD_DIR, Options, PREAMBLE, PREAMBLE_PATH, Pair, Placed, Redline, Side, Version,
    composite, pair_pages, redline,
};

const FULL: Options = Options {
    eval_deleted: true,
    content_blocks: true,
};

/// Deleted code shown by name instead of evaluated.
const BY_NAME: Options = Options {
    eval_deleted: false,
    content_blocks: true,
};

fn fixtures() -> impl Iterator<Item = String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
}

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

fn generate(name: &str, preamble: &str, opts: Options) -> Run {
    let (read_old, read_new) = (
        |p: &str| read_fixture(name, "old", p),
        |p: &str| read_fixture(name, "new", p),
    );
    let v = |label: &str, read| Version {
        label: label.into(),
        entry: "main.typ".into(),
        read,
    };
    let r = redline(&v("old", &read_old), &v("new", &read_new), opts);
    // The new tree, the old-only files, the old tree if asked for, then the
    // overlay.
    let mut files = tree(name, "new");
    for f in tree(name, "old") {
        if !files.iter().any(|(q, _)| *q == f.0) {
            files.push(f);
        }
    }
    if r.files.iter().any(|(p, _)| p.starts_with(OLD_DIR)) {
        files.extend(
            tree(name, "old")
                .into_iter()
                .map(|(p, c)| (format!("{OLD_DIR}/{p}"), c)),
        );
    }
    for (p, c) in r.files.into_iter().chain([(PREAMBLE_PATH.into(), preamble.into())]) {
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
    generate(name, PREAMBLE, FULL)
}

fn body(name: &str) -> String {
    run(name).body
}

/// The redline of two one-file versions.
fn redline_of(old: &str, new: &str) -> Redline {
    let (old, new) = (old.to_string(), new.to_string());
    let v = |read| Version {
        label: "v".into(),
        entry: "main.typ".into(),
        read,
    };
    redline(
        &v(&move |_: &str| Some(old.clone())),
        &v(&move |_: &str| Some(new.clone())),
        FULL,
    )
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
    assert!(out.contains("#emph(\"eq-old\")"), "{out}");
    assert!(out.contains("and to @sec-intro"), "{out}");
}

#[test]
fn line_endings_are_not_changes() {
    let r = run("crlf");
    assert!(r.changes.is_empty(), "{}", r.body);
}

#[test]
fn changes_stay_inside_nested_items() {
    let out = body("nested-list");
    assert!(out.contains("\n  - Nested one #redline-ins"), "{out}");
    assert!(
        out.contains("\n  - Nested two\n    #redline-del(block: true)[#sym.bullet Nested gone];\n- Third item"),
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
        out.contains("extension#redline-del()[ is given in closed form;];#redline-ins()[, the Procrustes map,]; see"),
        "{out}"
    );
}

#[test]
fn unspaced_scripts_are_compared_by_character() {
    let out = body("cjk");
    assert!(out.contains("日本語#redline-ins()[の];テキスト"), "{out}");
}

#[test]
fn replaced_blocks_alternate_with_their_replacements() {
    let out = body("interleaved");
    let at = |s: &str| out.find(s).unwrap();
    assert!(
        at("First old") < at("One wholly") && at("One wholly") < at("Second old") && at("Second old") < at("Another"),
        "{out}"
    );
}

#[test]
fn inserted_figure_is_marked_as_a_block() {
    let out = body("inserted-figure");
    assert!(out.contains("#redline-ins(block: true)[#figure("), "{out}");
}

#[test]
fn changed_punctuation_is_marked_with_its_word() {
    let out = body("punctuation-word");
    assert!(
        out.contains("#redline-del(gap: true)[holds,];#redline-ins()[holds;]"),
        "{out}"
    );
}

#[test]
fn numbers_and_hyphenated_words_are_whole_words() {
    let out = body("tokens");
    assert!(out.contains("[2.0];#redline-ins()[2.5]"), "{out}");
    assert!(out.contains("[ground states];#redline-ins()[ground-state]"), "{out}");
}

#[test]
fn deleted_punctuation_stays_next_to_its_word() {
    let out = body("deleted-punctuation");
    assert!(
        out.contains("extension#redline-del()[, the map, minimizes a cost]; #redline-ins()[is given"),
        "{out}"
    );
}

#[test]
fn paragraphs_inserted_inside_a_run_are_marked_as_blocks() {
    let out = body("inserted-paragraphs");
    assert!(
        out.contains("#redline-ins(block: true)[A whole new paragraph.]"),
        "{out}"
    );
}

#[test]
fn emphasised_text_is_compared_word_by_word() {
    let out = body("emphasis");
    assert!(
        out.contains("_emphasised #redline-del(gap: true)[old];#redline-ins()[new]; words_"),
        "{out}"
    );
}

#[test]
fn figure_gaining_a_label_is_listed_not_replaced() {
    let r = run("figure-label");
    assert!(
        !r.body.contains("#redline-ins(") && !r.body.contains("#redline-del("),
        "{}",
        r.body
    );
    assert!(r.changes.iter().all(|c| c.kind == ChangeKind::Unmarked), "{}", r.body);
}

#[test]
fn deleted_code_sees_the_old_definitions_and_files() {
    let Some(dir) = compile("old-definitions-query", &run("old-definitions").files) else {
        return;
    };
    let out = Command::new("typst")
        .args(["query", "--root"])
        .arg(&*dir)
        .arg(dir.join("main.typ"))
        .args(["metadata", "--field", "value"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("\"oldold\""), "{out:?}");
}

#[test]
fn deleted_code_that_is_not_evaluated_is_shown_by_name() {
    let out = generate("old-definitions", PREAMBLE, BY_NAME).body;
    assert!(out.contains("[figure]") && !out.contains("rect(v)"), "{out}");
}

#[test]
fn heading_with_a_new_label_is_not_listed_as_deleted() {
    let out = body("heading-label");
    assert!(!out.contains("deleted: true"), "{out}");
}

#[test]
fn page_break_inserted_with_a_slide_is_part_of_it() {
    let r = run("inserted-slide");
    assert!(r.changes.iter().all(|c| c.kind != ChangeKind::Unmarked), "{}", r.body);
    let lone = redline_of("A\n\nB\n", "A\n#pagebreak()\nB\n");
    assert!(lone.changes.iter().any(|c| c.kind == ChangeKind::Unmarked));
}

#[test]
fn edit_after_a_deleted_heading_counts_with_the_section_before() {
    let r = redline_of(
        "= A\nalpha\n= B\nbeta\n\ngamma text here.\n",
        "= A\nalpha\n\nbeta\n\ngamma text changed.\n",
    );
    let section = |text: &str| {
        r.changes
            .iter()
            .find(|c| c.text.contains(text))
            .unwrap()
            .section
            .clone()
    };
    assert!(
        section("changed").is_some() && section("changed") != section("B"),
        "{:?}",
        r.changes
    );
}

#[test]
fn paragraphs_under_a_renamed_heading_are_compared_word_by_word() {
    let para = |w: &str| format!("The gap closes at the sweet spot of the {w} chain.\n");
    let r = redline_of(
        &format!("= Results\n\n{}", para("short")),
        &format!("= Numerical findings\n\n{}", para("long")),
    );
    assert!(r.changes.iter().all(|c| !c.text.contains("gap")), "{:?}", r.changes);
}

#[test]
fn section_replacing_another_is_not_compared_with_it() {
    let r = run("replaced-section");
    let whole = |kind, text: &str| r.changes.iter().any(|c| c.kind == kind && c.text.contains(text));
    assert!(
        whole(ChangeKind::Del, "Zero-bias peak at the sweet spot")
            && whole(ChangeKind::Ins, "Second sweet spot appears"),
        "{}",
        r.body
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
    // Typst 0.11 knows no other page template than `{n}`.
    let dir = compile_to(name, files, "page-{n}.png")?;
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
        std::fs::create_dir_all(dir.join(p).parent().unwrap()).unwrap();
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
    Some(query(&dir, "<probe>"))
}

/// The values of the elements with `label` in the compiled project in `dir`.
fn query(dir: &Path, label: &str) -> String {
    let out = Command::new("typst")
        .args(["query", "--root"])
        .arg(dir)
        .arg(dir.join("main.typ"))
        .args([label, "--field", "value"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).split_whitespace().collect()
}

#[test]
fn output_compiles() {
    // Evaluated, the deleted code of `deleted-code` fails, as it should.
    for name in fixtures().filter(|n| n != "deleted-code") {
        compile(&name, &run(&name).files);
    }
}

#[test]
fn template_applied_by_a_show_rule_sees_the_content() {
    compile("template-body", &run("template-body").files);
}

#[test]
fn deleted_code_that_would_fail_compiles_when_shown_by_name() {
    compile("deleted-code", &generate("deleted-code", PREAMBLE, BY_NAME).files);
}

/// Renders insertions as plain content and deletions as nothing.
const PLAIN: &str = "#let redline-ins(body, ..args) = body
#let redline-del(..args) = none
#let redline-summary(info) = none
#let redline-bars(body) = body
";

#[test]
fn proof_without_deletions_is_the_new_document() {
    for name in fixtures() {
        let r = generate(&name, PLAIN, BY_NAME);
        assert!(name == "crlf" || !r.changes.is_empty(), "{name}: no change found");
        let Some(proof) = render(&format!("{name}-proof"), &r.files) else {
            return;
        };
        let new = render(&format!("{name}-new"), &tree(&name, "new")).unwrap();
        assert!(!new.is_empty() && proof == new, "{name}:\n{}", r.body);
    }
}

const WORD_CHANGE: &str = "#redline-del(gap: true)[alpha";

#[test]
fn change_inside_included_file_is_marked() {
    let r = run("include-change");
    assert!(r.file("part.typ").contains(WORD_CHANGE), "{}", r.file("part.typ"));
    assert!(r.changes.iter().all(|c| c.file == "part.typ"));
}

#[test]
fn deleted_include_is_shown_struck() {
    let r = run("include-deleted");
    assert!(
        r.body
            .contains("redline-del(block: true)[#include \"/.redline-old/gone.typ\"]"),
        "{}",
        r.body
    );
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
    assert!(r.body.contains("#redline-del(gap: true)[red];"), "{}", r.body);
}

#[test]
fn changed_code_argument_is_replaced_in_place() {
    let r = run("changed-argument");
    assert_eq!(r.body.matches("#grid(").count(), 1, "{}", r.body);
    assert!(r.body.contains("#redline-del(gap: true)[old];"), "{}", r.body);
    assert!(
        r.body.contains("#redline-ins(block: true)[#rect(width: 2cm)]"),
        "{}",
        r.body
    );
}

#[test]
fn frame_of_a_grid_with_fractional_columns_is_as_wide_as_the_text() {
    let Some(dir) = compile("grid-frame", &run("grid-frame").files) else {
        return;
    };
    let out = Command::new("typst")
        .args(["query", "--root"])
        .arg(&*dir)
        .arg(dir.join("main.typ"))
        .args(["<probe>", "--field", "value"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("283.4"), "{out:?}");
}

#[test]
fn frame_of_a_relative_width_element_is_as_wide_as_the_element() {
    if let Some(v) = probe("relative-frame") {
        assert!(v.contains("141.7"), "{v}");
    }
}

#[test]
fn changed_shape_in_a_figure_is_crossed_out() {
    if let Some(v) = probe("changed-shape") {
        // Lines of the cross have an end point, others have none.
        assert!(v.contains("[\""), "{v}");
    }
}

#[test]
fn changed_content_variable_in_imported_file_is_marked() {
    let r = run("imported-content");
    assert!(r.file("defs.typ").contains(WORD_CHANGE), "{}", r.file("defs.typ"));
}

#[test]
fn changed_code_in_imported_file_is_listed_as_not_marked() {
    let r = run("imported-code");
    let changes: Vec<_> = r
        .changes
        .iter()
        .map(|c| (c.file.as_str(), c.kind, c.text.as_str()))
        .collect();
    assert_eq!(changes, [("defs.typ", ChangeKind::Unmarked, "#let f(x) = x + 2")]);
    assert!(r.body.contains("\"#let f(x) = x + 2\""), "{}", r.body);
}

#[test]
fn deleted_footnote_does_not_step_the_counter() {
    if let Some(v) = probe("deleted-footnote") {
        assert_eq!(v, "[1]");
    }
}

#[test]
fn deleted_state_and_counter_updates_have_no_effect() {
    if let Some(v) = probe("deleted-update") {
        assert_eq!(v, "[[0,1]]");
    }
}

#[test]
fn citation_added_to_a_group_marks_the_group() {
    let r = run("citation-group");
    assert!(r.body.contains("#redline-ins()[@a @b]"), "{}", r.body);
}

#[test]
fn new_gone_and_edited_bibliography_entries_are_listed() {
    let r = run("bib-entries");
    assert!(
        r.body
            .contains("bib: (added: (\"c\",), removed: (\"b\",), changed: (\"a\",))"),
        "{}",
        r.body
    );
}

#[test]
fn page_foregrounds_of_the_document_are_kept_with_the_bars() {
    let Some(dir) = compile("page-foreground", &run("page-foreground").files) else {
        return;
    };
    assert!(query(&dir, "<probe>").contains("\"draft\""));
    assert_eq!(query(&dir, "<redline-unbarred>"), "[[]]");
}

#[test]
fn computed_include_path_is_reported() {
    let read = |_: &str| Some("#include \"a\" + \".typ\"\n".to_string());
    let v = Version {
        label: "v".into(),
        entry: "main.typ".into(),
        read: &read,
    };
    assert_eq!(redline(&v, &v, FULL).warnings.len(), 1);
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
    assert!(src.contains(&format!("#redline-del(gap: true)[{old}")), "{src}");
    assert!(src.contains(&format!("#redline-ins()[{new}")), "{src}");
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
    assert!(out.status.success() && !stderr.contains("shown by name"), "{stderr}");
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
    let printed = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    assert_eq!(
        printed.canonicalize().unwrap(),
        dir.join("new.redline.pdf").canonicalize().unwrap()
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
fn to_with_a_range_is_a_usage_error() {
    let dir = temp_dir("to-range");
    let out = cli(&dir, &["main.typ", "--since", "a..b", "--to", "c"]);
    assert_eq!(out.status.code(), Some(2));
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

#[test]
fn side_by_side_pairs_pages_with_the_same_changes() {
    let c = |ins, id, page| Placed { ins, id, page };
    let old = [
        c(false, 1, 1),
        c(true, 2, 1),
        c(true, 2, 2),
        c(false, 3, 3),
        c(false, 3, 4),
        c(true, 9, 6),
    ];
    let new = [
        c(false, 1, 1),
        c(true, 2, 2),
        c(true, 2, 2),
        c(false, 3, 4),
        c(false, 3, 5),
    ];
    let (pairs, unpaired) = pair_pages(&old, &new, (&[], &[]));
    let pair = |old: &[usize], new: &[usize], steps_old: &[usize], steps_new: &[usize]| Pair {
        old: old.to_vec(),
        new: new.to_vec(),
        steps_old: steps_old.to_vec(),
        steps_new: steps_new.to_vec(),
    };
    assert_eq!(
        pairs,
        [
            pair(&[1, 2], &[1, 2], &[], &[]),
            pair(&[4], &[5], &[3], &[4]),
            pair(&[6], &[], &[], &[])
        ]
    );
    assert_eq!(unpaired, [(true, c(true, 9, 6))]);
}

#[test]
fn side_by_side_pairs_changes_written_differently() {
    let c = |ins, id, page| Placed { ins, id, page };
    let old = [c(true, 5, 1), c(true, 6, 2), c(true, 7, 3), c(false, 1, 4)];
    let new = [
        c(true, 8, 1),
        c(true, 9, 2),
        c(true, 7, 3),
        c(false, 2, 4),
        c(false, 3, 5),
    ];
    let (pairs, unpaired) = pair_pages(&old, &new, (&[], &[]));
    let pages: Vec<_> = pairs.iter().map(|p| (p.old.clone(), p.new.clone())).collect();
    assert_eq!(
        pages,
        [
            (vec![1], vec![1]),
            (vec![2], vec![2]),
            (vec![3], vec![3]),
            (vec![4], vec![4, 5])
        ]
    );
    assert!(unpaired.is_empty());
}

#[test]
fn side_by_side_shows_an_inserted_or_deleted_page_without_a_counterpart() {
    let c = |ins, id, page| Placed { ins, id, page };
    let pages = |ins, anchors| {
        let (pairs, _) = pair_pages(&[c(ins, 7, 3)], &[c(ins, 7, 3)], anchors);
        pairs.iter().map(|p| (p.old.clone(), p.new.clone())).collect::<Vec<_>>()
    };
    // Page 3 of the new version is inserted; the old version marks its
    // place at the top of page 3, whose heading is on page 4 of the new.
    assert_eq!(pages(true, (&[(1, 3)], &[(1, 4)])), [(vec![], vec![3])]);
    // And the other way round for a deleted page.
    assert_eq!(pages(false, (&[(1, 4)], &[(1, 3)])), [(vec![3], vec![])]);
}

#[test]
fn side_by_side_summary_lists_bibliography_changes() {
    let side = Side {
        label: "v",
        pdf: "v.pdf",
        pages: 1,
        unbarred: &[],
        changes: &[],
        headings: &[],
    };
    let bib = Bib {
        added: vec!["c".into()],
        ..Bib::default()
    };
    assert!(composite(&side, &side, &[], &bib).contains("\"c\""));
}

/// Whether the typst on PATH is older than the side-by-side layout needs.
fn typst_before_0_14() -> bool {
    let version = Command::new("typst").arg("--version").output().unwrap().stdout;
    let version = String::from_utf8_lossy(&version);
    ["0.11.", "0.12.", "0.13."].iter().any(|v| version.contains(v))
}

#[test]
fn side_by_side_old_pages_are_the_old_document() {
    if !typst_available() || typst_before_0_14() {
        return;
    }
    let dir = temp_dir("side-by-side-old");
    std::fs::write(
        dir.join("old.typ"),
        "#set page(width: 8cm, height: 6cm)\nThe old word.\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("new.typ"),
        "#set page(width: 12cm, height: 6cm)\nThe new word.\n",
    )
    .unwrap();
    let out = cli(
        &dir,
        &["old.typ", "new.typ", "-o", "out.pdf", "--layout", "side-by-side"],
    );
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let pdf = String::from_utf8_lossy(&std::fs::read(dir.join("out.pdf")).unwrap()).to_string();
    let widest = pdf
        .split("/MediaBox")
        .skip(1)
        .filter_map(|s| {
            s.trim_start()
                .strip_prefix('[')?
                .split_whitespace()
                .nth(2)?
                .parse::<f64>()
                .ok()
        })
        .fold(0.0, f64::max);
    // An 8cm page next to a 12cm one, with less than 4cm around them.
    assert!(widest > 0.0 && widest < 24.0 / 2.54 * 72.0, "{widest}");
}

#[test]
fn side_by_side_shows_changed_pages() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("side-by-side");
    let doc =
        |word: &str| format!("#set page(height: 6cm)\nOne.\n#pagebreak()\nThe {word} two.\n#pagebreak()\nThree.\n");
    std::fs::write(dir.join("old.typ"), doc("old")).unwrap();
    std::fs::write(dir.join("new.typ"), doc("new")).unwrap();
    let out = cli(
        &dir,
        &["old.typ", "new.typ", "-o", "out.pdf", "--layout", "side-by-side"],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    if typst_before_0_14() {
        assert!(
            !out.status.success() && stderr.contains("Typst 0.14 or later"),
            "{stderr}"
        );
        return;
    }
    assert!(out.status.success(), "{stderr}");
    let pdf =
        String::from_utf8_lossy(&std::fs::read(dir.join("out.pdf")).unwrap()).replace("/Type /Page", "/Type/Page");
    let pages = pdf.matches("/Type/Page").count() - pdf.matches("/Type/Pages").count();
    // The summary and the one changed pair.
    assert_eq!(pages, 2);
}

#[test]
fn side_by_side_needs_the_preamble_function() {
    let dir = temp_dir("side-by-side-preamble");
    std::fs::write(dir.join("old.typ"), "Old.").unwrap();
    std::fs::write(dir.join("new.typ"), "New.").unwrap();
    let inline_only = PREAMBLE.replace("redline-side-by-side", "other");
    std::fs::write(dir.join("p.typ"), inline_only).unwrap();
    let out = cli(
        &dir,
        &["old.typ", "new.typ", "--preamble", "p.typ", "--layout", "side-by-side"],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.code() == Some(1) && stderr.contains("does not define redline-side-by-side"),
        "{stderr}"
    );
}

#[test]
fn typst_root_sets_the_root() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("typst-root");
    std::fs::create_dir(dir.join("doc")).unwrap();
    std::fs::write(dir.join("shared.typ"), "Shared.\n").unwrap();
    std::fs::write(dir.join("doc/old.typ"), "#include \"/shared.typ\"\nOld.\n").unwrap();
    std::fs::write(dir.join("doc/new.typ"), "#include \"/shared.typ\"\nNew.\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_typst-redline"))
        .current_dir(dir.join("doc"))
        .env("TYPST_ROOT", &*dir)
        .args(["old.typ", "new.typ"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn root_from_typst_root_is_named_in_errors() {
    let dir = temp_dir("outside-typst-root");
    std::fs::create_dir(dir.join("root")).unwrap();
    std::fs::write(dir.join("old.typ"), "Old.").unwrap();
    std::fs::write(dir.join("new.typ"), "New.").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_typst-redline"))
        .current_dir(&*dir)
        .env("TYPST_ROOT", dir.join("root"))
        .args(["old.typ", "new.typ"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not inside the root") && stderr.contains("TYPST_ROOT"),
        "{stderr}"
    );
}

#[test]
fn file_outside_the_root_suggests_the_repository_as_root() {
    if !typst_available() {
        return;
    }
    let dir = repo("outside-root", &["alpha"]);
    std::fs::write(dir.join("shared.typ"), "Shared.\n").unwrap();
    std::fs::write(
        dir.join("doc/main.typ"),
        format!("#include \"../shared.typ\"\n{}", doc("beta")),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_typst-redline"))
        .current_dir(&*dir)
        .env_remove("TYPST_ROOT")
        .args(["doc/main.typ"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let top = git(&dir, &["rev-parse", "--show-toplevel"]);
    assert!(stderr.contains(&format!("--root {top}")), "{stderr}");
}
