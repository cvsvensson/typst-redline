use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use typst_redline::{
    Bib, Change, ChangeKind, History, Moves, OLD_DIR, Options, PREAMBLE, PREAMBLE_PATH, Pair, Placed, Redline, Side,
    Version, composite, pair_pages, redline,
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
    fn file(&self, path: &str) -> String {
        without_ids(&self.files.iter().find(|(p, _)| p == path).unwrap().1)
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
        |p: &str| read_fixture(name, "old", p).map(String::into_bytes),
        |p: &str| read_fixture(name, "new", p).map(String::into_bytes),
    );
    let v = |label: &str, read| Version {
        label: label.into(),
        entry: "main.typ".into(),
        read,
    };
    let r = redline(
        &v("old", &read_old),
        &v("new", &read_new),
        opts,
        &History::default(),
        Moves::Find,
        None,
    );
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
        body: without_ids(&files.iter().find(|(p, _)| p == "main.typ").unwrap().1),
        files,
        changes: r.changes,
    }
}

/// Marked source with the `id` arguments of its marks left out.
fn without_ids(src: &str) -> String {
    let mut out = String::new();
    let mut rest = src;
    while let Some(i) = rest.find("id: ") {
        let digits = rest[i + 4..].bytes().take_while(u8::is_ascii_digit).count();
        let end = i + 4 + digits;
        if digits > 0 && rest[end..].starts_with(')') {
            out.push_str(rest[..i].strip_suffix(", ").unwrap_or(&rest[..i]));
        } else {
            out.push_str(&rest[..end]);
        }
        rest = &rest[end..];
    }
    out + rest
}

fn run(name: &str) -> Run {
    generate(name, PREAMBLE, FULL)
}

fn body(name: &str) -> String {
    run(name).body
}

/// The redline of two one-file versions.
fn redline_of(old: &str, new: &str) -> Redline {
    let (old, new) = (old.as_bytes().to_vec(), new.as_bytes().to_vec());
    let v = |read| Version {
        label: "v".into(),
        entry: "main.typ".into(),
        read,
    };
    redline(
        &v(&move |_: &str| Some(old.clone())),
        &v(&move |_: &str| Some(new.clone())),
        FULL,
        &History::default(),
        Moves::Find,
        None,
    )
}

#[test]
fn equation_turned_into_a_display_equation_is_marked() {
    let r = redline_of("Text $x = 1$ end.\n", "Text $ x = 1 $ end.\n");
    let out = without_ids(&r.files[0].1);
    assert!(out.contains("[$x = 1$]};#redline-ins(block: true)[$ x = 1 $]"), "{out}");
}

#[test]
fn changed_spacing_in_raw_text_is_marked() {
    let r = redline_of("See `a b` here.\n", "See `a  b` here.\n");
    let out = without_ids(&r.files[0].1);
    assert!(out.contains("#redline-ins()[`a  b`]"), "{out}");
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
    let (_summary, doc) = out
        .split_once(
            ",
)) }
",
        )
        .unwrap();
    assert_eq!(doc.matches("<eq-energy>").count(), 1, "{out}");
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
fn summary_and_bars_wrap_the_document_at_the_top_level() {
    for name in ["list-first", "deleted-first"] {
        let out = body(name);
        let at = out
            .find("#show: body => { redline-bars(body); redline-summary(")
            .unwrap();
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
    let caption = query_field(&dir, "figure", "caption");
    assert_eq!(caption.matches(r#""text":"old""#).count(), 2, "{caption}");
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

#[test]
fn call_is_compared_with_the_call_it_resembles() {
    let r = run("same-call-pairing");
    assert!(
        matches!(&r.changes[..], [c] if c.kind == ChangeKind::Del && c.text.contains("Second")),
        "{}",
        r.body
    );
}

#[test]
fn changed_term_is_marked_and_its_description_is_not() {
    let r = run("term-marker");
    let marked = |kind, text: &str| r.changes.iter().any(|c| c.kind == kind && c.text == text);
    assert!(marked(ChangeKind::Ins, "Plum"), "{}", r.body);
    assert!(r.changes.iter().all(|c| !c.text.contains("description")), "{}", r.body);
}

#[test]
fn item_nested_under_its_neighbour_leaves_the_neighbour_unchanged() {
    let r = redline_of(
        "- Alpha item text\n- Beta item text\n- Gamma item text\n",
        "- Alpha item text\n  - Beta item text\n- Gamma item text\n",
    );
    assert!(r.changes.iter().all(|c| !c.text.contains("Alpha")), "{:?}", r.changes);
}

const MOVED_FROM: &str = "#redline-del(move: (";
const MOVED_TO: &str = "#redline-ins(move: (";

#[test]
fn moved_paragraph_is_marked_at_both_places_not_struck() {
    let r = run("moved-paragraph");
    assert_eq!(r.body.matches(MOVED_FROM).count(), 1, "{}", r.body);
    assert!(r.changes.iter().all(|c| c.kind == ChangeKind::Moved), "{:?}", r.changes);
}

#[test]
fn edits_inside_moved_text_are_marked() {
    let r = run("moved-edited");
    let changed = |kind, text: &str| r.changes.iter().any(|c| c.kind == kind && c.text == text);
    assert!(changed(ChangeKind::Moved, "0"), "{:?}", r.changes);
    assert!(
        changed(ChangeKind::Del, "dawn") && changed(ChangeKind::Ins, "dusk"),
        "{:?}",
        r.changes
    );
}

#[test]
fn moved_setup_code_is_listed_as_changed_code() {
    let r = run("moved-setup");
    assert!(
        r.changes.iter().any(|c| c.kind == ChangeKind::Unmarked),
        "{:?}",
        r.changes
    );
    assert!(r.changes.iter().all(|c| c.kind != ChangeKind::Moved), "{:?}", r.changes);
}

#[test]
fn text_moved_into_a_file_that_is_not_shown_is_deleted() {
    let r = run("moved-into-hidden");
    assert!(r.changes.iter().any(|c| c.kind == ChangeKind::Del), "{:?}", r.changes);
    assert!(r.changes.iter().all(|c| c.kind != ChangeKind::Moved), "{:?}", r.changes);
}

#[test]
fn deleted_block_after_a_list_is_not_part_of_its_last_item() {
    let out = body("deleted-after-list");
    assert!(out.contains("second item\n#redline-del("), "{out}");
}

/// Draws the markers of moves but nothing else.
const MOVE_MARKERS: &str = "
#let redline-del(..args) = none
#let redline-summary(info) = none
#let redline-bars(body) = body
";

#[test]
fn move_markers_do_not_change_the_layout() {
    let preamble = format!("{PREAMBLE}{MOVE_MARKERS}");
    for name in [
        "moved-paragraph",
        "moved-section",
        "moved-item",
        "moved-figure",
        "moved-heading-outline",
    ] {
        // Typst 0.11 antialiases the moved item's line one grey level
        // differently, with the text in the same place.
        if name == "moved-item" && typst_available() && typst_before_0_14() {
            continue;
        }
        let r = generate(name, &preamble, BY_NAME);
        assert!(r.body.contains(MOVED_TO), "{name}: {}", r.body);
        let Some(proof) = render(&format!("{name}-markers"), &r.files) else {
            return;
        };
        let new = render(&format!("{name}-markers-new"), &tree(name, "new")).unwrap();
        assert!(proof == new, "{name}:\n{}", r.body);
    }
}

#[test]
fn moved_heading_is_marked_once_with_an_outline_and_running_headers() {
    let Some(dir) = compile("moved-heading-outline", &run("moved-heading-outline").files) else {
        return;
    };
    let labels = query_field(&dir, r#"state("").update(none).func()"#, "label");
    assert_eq!(labels.matches("redline-move-to-").count(), 1, "{labels}");
}

#[test]
fn no_moves_shows_moved_text_deleted_and_inserted() {
    let dir = temp_dir("no-moves");
    for side in ["old", "new"] {
        let src = read_fixture("moved-paragraph", side, "main.typ").unwrap();
        std::fs::write(dir.join(format!("{side}.typ")), src).unwrap();
    }
    if !typst_available() {
        return;
    }
    let out = marked(&dir, &["old.typ", "new.typ", "--no-moves"], "new.typ");
    assert!(
        !out.contains("move:") && out.contains("#redline-del(") && out.contains("#redline-ins("),
        "{out}"
    );
}

#[test]
fn moved_section_is_one_move() {
    let out = body("moved-section");
    assert_eq!(out.matches(MOVED_FROM).count(), 1, "{out}");
    assert!(out.contains("moved: true"), "{out}");
}

#[test]
fn moved_section_moved_in_parts_counts_once() {
    let (forest, lake) = (
        "Old beech trees shade the forest floor.\n\n",
        "Herons wade along the shallow lake.\n\n",
    );
    let (bread, coast) = (
        "Fresh bread needs a long slow rise.\n\n",
        "The night train reaches the coast.\n\n",
    );
    let gulls = "Gulls circle above the harbour.\n\nSalt spray coats the windows.\n";
    let old = format!("= Forest\n\n{forest}{lake}{bread}= Coast\n\n{coast}{gulls}\n");
    let new = format!("= Coast\n\n{coast}{lake}{gulls}\n= Forest\n\n{forest}{bread}");
    let out = &redline_of(&old, &new).files[0].1;
    let forest = out.lines().find(|l| l.contains("title: [Forest]")).unwrap();
    assert!(forest.contains("moves: 0, moved: true"), "{out}");
}

#[test]
fn moved_list_item_is_marked_inside_its_item() {
    let out = body("moved-item");
    assert!(out.contains(&format!("- {MOVED_TO}")), "{out}");
    assert_eq!(out.matches(MOVED_FROM).count(), 1, "{out}");
}

#[test]
fn moved_code_is_named_by_its_first_words() {
    let out = body("move-note-code");
    let line = out.lines().find(|l| l.contains(MOVED_FROM)).unwrap();
    assert!(line.contains("A boxed remark"), "{out}");
}

#[test]
fn text_moved_into_an_included_file_is_marked_in_both_files() {
    let r = run("moved-include");
    assert!(r.body.contains(MOVED_FROM), "{}", r.body);
    assert!(r.file("part.typ").contains(MOVED_TO), "{}", r.file("part.typ"));
}

#[test]
fn moved_text_needs_the_preamble_support() {
    let dir = temp_dir("move-preamble");
    for side in ["old", "new"] {
        let src = read_fixture("moved-paragraph", side, "main.typ").unwrap();
        std::fs::write(dir.join(format!("{side}.typ")), src).unwrap();
    }
    std::fs::write(dir.join("p.typ"), PREAMBLE.replace("move:", "other:")).unwrap();
    let out = cli(&dir, &["old.typ", "new.typ", "--preamble", "p.typ"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.code() == Some(1) && stderr.contains("moved text"),
        "{stderr}"
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
    // Tests running in parallel may use the same name, e.g. for the same fixture.
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("typst-redline-test-{}-{n}-{name}", std::process::id()));
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
    query_field(dir, label, "value")
}

/// The `field` of the elements that `selector` finds in the compiled project
/// in `dir`.
fn query_field(dir: &Path, selector: &str, field: &str) -> String {
    let out = Command::new("typst")
        .args(["query", "--root"])
        .arg(dir)
        .arg(dir.join("main.typ"))
        .args([selector, "--field", field])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).split_whitespace().collect()
}

/// Reads its body as text, so that a mark in it is dropped.
const PIPES: &str = r#"#let pipes(body) = {
  let rows = body.children.map(c => if c.func() == text { c.text } else if c == [ ] { " " } else if c.func() == linebreak or repr(c.func()) == "space" { "\n" } else { "" }).join()
  let cells = rows.split("|").map(str.trim).filter(s => s != "")
  table(columns: 2, ..cells.map(x => [#x]))
}
"#;

#[test]
fn change_that_a_function_drops_is_listed() {
    let doc = |row: &str| format!("{PIPES}#pipes[\n| Alpha | 1 |\n| {row} |\n]\n");
    let r = redline_of(&doc("Gamma | 2"), &doc("Delta | 3"));
    let files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat();
    let Some(dir) = compile("text-parsed-body", &files) else {
        return;
    };
    let lost = query_field(&dir, "<redline-lost>", "key");
    let texts: Vec<&str> = lost
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|i| Some(r.changes.get(i.parse::<usize>().ok()?)?.text.as_str()))
        .collect();
    assert_eq!(texts, ["Gamma | 2", "Delta | 3"], "{lost}");
}

/// The `<probe>` values of the marked document and of the new version.
fn probes(name: &str, old: &str, new: &str) -> Option<(String, String)> {
    let r = redline_of(old, new);
    let files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat();
    let marked = compile(&format!("{name}-marked"), &files)?;
    let plain = compile(&format!("{name}-new"), &[("main.typ".into(), new.into())]).unwrap();
    Some((query(&marked, "<probe>"), query(&plain, "<probe>")))
}

#[test]
fn text_show_rule_matches_across_the_edge_of_a_change() {
    let doc = |w: &str| {
        format!("#show \"new version\": strong\n#show strong: it => [#metadata(0)<probe>#it]\nThe {w} version.\n")
    };
    if let Some((marked, new)) = probes("show-string", &doc("old"), &doc("new")) {
        assert_eq!(marked, new);
    }
}

#[test]
fn marked_word_is_laid_out_in_one_piece() {
    // Split, a word is shaped as two, which breaks the joining of letters.
    let doc = |w: &str| {
        format!(
            "#show text: it => if it.text.contains(\"{w}\") [#metadata(0)<probe>#it] else {{ it }}\nThe {w} text.\n"
        )
    };
    if let Some((marked, new)) = probes("one-piece", &doc("zebra"), &doc("quail")) {
        assert_eq!(marked, new);
    }
}

#[test]
fn deleted_drawing_in_the_text_is_crossed_out() {
    let doc = |c: &str| format!("#show line: it => [#metadata(it.end)<probe>#it]\nA shape{c} here.\n");
    if let Some((marked, _)) = probes("inline-drawing", &doc(" #box(circle(radius: 3mm))"), &doc("")) {
        // Lines of the cross have an end point, bars have none.
        assert_eq!(marked.matches("[\"").count(), 2, "{marked}");
    }
}

#[test]
fn bars_of_right_to_left_text_are_on_the_page() {
    // Typst 0.11 orders the tags of right-to-left text by position.
    let version = Command::new("typst").arg("--version").output().map(|o| o.stdout);
    if version.is_ok_and(|v| String::from_utf8_lossy(&v).contains(" 0.11.")) {
        return;
    }
    let doc = |w: &str| {
        format!(
            "#set page(width: 10cm, height: auto)\n#set text(lang: \"ar\")\n#show line: it => context [#metadata(here().position().x.pt())<probe>#it]\nمرحبا بالعالم {w}\n"
        )
    };
    if let Some((marked, _)) = probes("rtl-bars", &doc(""), &doc("جديد")) {
        let xs: Vec<f64> = marked
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|x| x.parse().unwrap())
            .collect();
        assert!(
            !xs.is_empty() && xs.iter().all(|x| (0.0..=283.5).contains(x)),
            "{marked}"
        );
    }
}

#[test]
fn change_in_hidden_content_is_listed() {
    let doc = |w: &str| format!("First line.\n\n#hide[Second line with {w}.]\n");
    let r = redline_of(&doc("gamma"), &doc("delta"));
    let files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat();
    let Some(dir) = compile("hidden-change", &files) else {
        return;
    };
    assert_eq!(query_field(&dir, "<redline-lost>", "key"), r#"["0,1"]"#);
}

#[test]
fn insertion_of_content_that_takes_no_room_adds_no_space() {
    let doc = |fill: &str| {
        format!(
            "First.\n\n#place(top + right, rect(fill: {fill}))\n\nSecond.#context [#metadata(here().position().y)<probe>]\n"
        )
    };
    let r = redline_of(&doc("red"), &doc("blue"));
    let without_deletions = format!("{PREAMBLE}\n#let redline-del(..args) = none\n#let redline-summary(info) = none\n");
    let files = [r.files, vec![(PREAMBLE_PATH.into(), without_deletions)]].concat();
    let Some(marked) = compile("roomless-mark", &files) else {
        return;
    };
    let new = compile("roomless-new", &[("main.typ".into(), doc("blue"))]).unwrap();
    assert_eq!(query(&marked, "<probe>"), query(&new, "<probe>"));
}

#[test]
fn changed_raw_block_is_marked_as_a_block() {
    let doc = |x: &str| format!("Text.\n\n```py\nx = {x}\n```\n");
    let r = redline_of(&doc("1"), &doc("2"));
    let src = without_ids(&r.files.iter().find(|(p, _)| p == "main.typ").unwrap().1);
    assert!(src.contains("redline-ins(block: true)[```py"), "{src}");
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

/// Every fixture: each change found is marked and on the summary's list of
/// marks, from which the summary lists those not laid out; the proof with
/// the deletions removed is the new document; and the binary makes a PDF in
/// both layouts.
#[test]
fn every_fixture_keeps_the_contract() {
    let names: Vec<String> = fixtures().collect();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let failed: Vec<&String> = std::thread::scope(|s| {
        let chunks: Vec<_> = names
            .chunks(names.len().div_ceil(threads))
            .map(|chunk| {
                s.spawn(move || {
                    let fails = |name: &&String| std::panic::catch_unwind(|| keeps_the_contract(name)).is_err();
                    chunk.iter().filter(fails).collect::<Vec<_>>()
                })
            })
            .collect();
        chunks.into_iter().flat_map(|c| c.join().unwrap()).collect()
    });
    assert!(failed.is_empty(), "{failed:?}");
}

fn keeps_the_contract(name: &str) {
    let r = generate(name, PLAIN, BY_NAME);
    assert!(name == "crlf" || !r.changes.is_empty(), "{name}: no change found");
    let marked = |i| r.files.iter().any(|(_, src)| src.contains(&format!("id: {i})[")));
    let entry = &r.files.iter().find(|(p, _)| p == "main.typ").unwrap().1;
    for (i, c) in r.changes.iter().enumerate() {
        if matches!(c.kind, ChangeKind::Ins | ChangeKind::Del) {
            assert!(
                marked(i) && entry.contains(&format!("(id: {i}, ")),
                "{name}: change {i} {c:?}"
            );
        }
    }
    let Some(proof) = render(&format!("{name}-proof"), &r.files) else {
        return;
    };
    let new = render(&format!("{name}-new"), &tree(name, "new")).unwrap();
    assert!(!new.is_empty() && proof == new, "{name}:\n{}", r.body);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    let side = |s: &str| match dir.join(s).is_dir() {
        true => dir.join(s).join("main.typ"),
        false => dir.join(format!("{s}.typ")),
    };
    let (old, new) = (side("old"), side("new"));
    let out = temp_dir(&format!("{name}-cli"));
    // The side-by-side layout shows the old document, which here does not
    // compile on its own: it reads files that are gone, or refers to an
    // equation without numbering.
    let old_fails = ["deleted-code", "include-deleted"].contains(&name);
    let layouts: &[&str] = if typst_before_0_14() || old_fails {
        &["inline"]
    } else {
        &["inline", "side-by-side"]
    };
    for layout in layouts {
        let pdf = out.join(format!("{layout}.pdf"));
        let args = [
            old.as_os_str(),
            new.as_os_str(),
            "-o".as_ref(),
            pdf.as_os_str(),
            "--layout".as_ref(),
            layout.as_ref(),
        ];
        let run = Command::new(env!("CARGO_BIN_EXE_typst-redline"))
            .current_dir(&*out)
            .args(args)
            .output()
            .unwrap();
        assert!(
            run.status.success() && pdf_pages(&pdf) > 0,
            "{name}, {layout}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
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
fn changed_argument_that_is_not_content_marks_the_whole_call() {
    let r = run("argument-not-content");
    let marks: Vec<_> = r
        .changes
        .iter()
        .map(|c| (c.kind, c.text.starts_with("#tablex(")))
        .collect();
    assert_eq!(marks, [(ChangeKind::Del, true), (ChangeKind::Ins, true)], "{}", r.body);
}

#[test]
fn changed_template_arguments_are_listed_with_old_and_new_or_marked() {
    let r = run("template-args");
    let listed = |old: &str, new: &str| {
        r.changes
            .iter()
            .any(|c| c.kind == ChangeKind::Unmarked && c.old.contains(old) && c.text.contains(new))
    };
    assert!(listed("ada@example.com", "ada@example.org"), "{:?}", r.changes);
    assert!(listed("\"Typesetting\"", "\"Diffing\""), "{:?}", r.changes);
    assert!(
        r.changes
            .iter()
            .any(|c| c.kind == ChangeKind::Ins && c.text == "several"),
        "{:?}",
        r.changes
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
fn frame_of_a_figure_holds_a_caption_wider_than_its_body() {
    let doc = |f: &str| format!("#show box: it => [#metadata(it.width)<probe>#it]\nText.\n{f}\n");
    let fig = "#figure(rect(width: 1cm), caption: [A caption much wider than the body])";
    if let Some((marked, _)) = probes("figure-caption-frame", &doc(fig), &doc("")) {
        let widest = marked
            .split('"')
            .filter_map(|v| v.rsplit('+').next()?.strip_suffix("pt")?.parse::<f64>().ok())
            .fold(0.0, f64::max);
        assert!(widest > 100.0, "{marked}");
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
fn summary_finds_sections_under_headings_inside_calls() {
    let mut r = run("headings-in-calls");
    let main = r.files.iter_mut().find(|(p, _)| p == "main.typ").unwrap();
    let marks = r.changes.len();
    main.1.push_str(&format!(
        "
#import \"/{PREAMBLE_PATH}\": redline-under-headings, redline-plain\n#context [#metadata(redline-under-headings(range({marks}).map(i => (id: i))).map(g => (redline-plain(g.heading.body), g.count)))<probe>]\n"
    ));
    let Some(dir) = compile("headings-in-calls", &r.files) else {
        return;
    };
    assert_eq!(query(&dir, "<probe>"), r#"[[["Goals",2],["Status",2]]]"#);
}

#[test]
fn summary_says_which_fallbacks_were_applied() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("fallback-note");
    std::fs::write(dir.join(PREAMBLE_PATH), PREAMBLE).unwrap();
    std::fs::write(
        dir.join("main.typ"),
        format!("#import \"{PREAMBLE_PATH}\": redline-fallback-note, redline-plain\n#metadata(redline-plain(redline-fallback-note))<probe>\n"),
    )
    .unwrap();
    let note = |fallbacks: &str| {
        let out = Command::new("typst")
            .args(["query", "--input", &format!("redline-fallback={fallbacks}")])
            .arg(dir.join("main.typ"))
            .args(["<probe>", "--field", "value", "--one"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    assert_eq!(note(""), "\"\"");
    assert!(note("frames,names").contains("by its name"));
}

#[test]
fn deleted_state_and_counter_updates_have_no_effect() {
    if let Some(v) = probe("deleted-update") {
        assert_eq!(v, "[[0,1]]");
    }
}

#[test]
fn two_sided_document_keeps_its_parity_and_page_numbers() {
    // Each header records its page's parity and the page number it reads.
    let Some(marked) = probe("page-parity") else {
        return;
    };
    let new = query(
        &compile("page-parity-new", &tree("page-parity", "new")).unwrap(),
        "<probe>",
    );
    assert_eq!(marked, new);
}

#[test]
fn document_reads_the_physical_page_numbers_of_the_new_version() {
    let Some(marked) = probe("physical-page") else {
        return;
    };
    let new = query(
        &compile("physical-page-new", &tree("physical-page", "new")).unwrap(),
        "<probe>",
    );
    assert_eq!(marked, new);
}

#[test]
fn document_queries_find_only_the_new_versions_elements() {
    let Some(marked) = probe("tool-metadata") else {
        return;
    };
    let new = query(
        &compile("tool-metadata-new", &tree("tool-metadata", "new")).unwrap(),
        "<probe>",
    );
    assert_eq!(marked, new);
}

#[test]
fn counters_stepped_by_deleted_content_keep_the_new_numbers() {
    if let Some(v) = probe("deleted-numbered") {
        assert_eq!(v, "[[2]]");
    }
}

#[test]
fn label_passed_to_deleted_code_is_not_defined_twice() {
    compile("label-argument", &run("label-argument").files);
}

#[test]
fn deleted_environment_that_checks_its_state_is_evaluated() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("state-check");
    let doc = |title: &str| {
        format!(
            "#let inside = state(\"inside\", false)
#let algo(title: \"\", body) = block[*#title* #inside.update(true)#body#inside.update(false)]
#let check = context assert(inside.get())
Text.

#algo(title: \"{title}\")[total #check]
"
        )
    };
    std::fs::write(dir.join("old.typ"), doc("Sum")).unwrap();
    std::fs::write(dir.join("new.typ"), doc("Total")).unwrap();
    let out = cli(&dir, &["old.typ", "new.typ"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && !stderr.contains("by name"), "{stderr}");
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
            .contains("bib: (added: (\"c\",), removed: (\"b\",), changed: (\"a\",)"),
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
    assert_eq!(query_field(&dir, "<redline-unbarred>", "key"), r#"[""]"#);
}

#[test]
fn computed_paths_are_reported_when_the_files_read_are_unknown() {
    let read = |_: &str| Some(b"#include \"a\" + \".typ\"\n".to_vec());
    let v = Version {
        label: "v".into(),
        entry: "main.typ".into(),
        read: &read,
    };
    let r = redline(&v, &v, FULL, &History::default(), Moves::Find, None);
    assert_eq!(r.warnings.len(), 1);
    assert!(r.bib.unchecked);
    let src = "#let d = json(\"d\" + \".json\")\n";
    assert!(redline_of(src, src).bib.unchecked);
}

#[test]
fn changed_files_read_by_unchanged_calls_are_listed() {
    let main = "#image(\"fig.svg\")\n#let d = json(\"/data/d.json\")\n#csv(\"same.csv\")\n";
    let read = |fig: &str, d: &str| {
        let (fig, d) = (fig.to_string(), d.to_string());
        move |p: &str| match p {
            "main.typ" => Some(main.into()),
            "fig.svg" => Some(fig.clone().into_bytes()),
            "data/d.json" => Some(d.clone().into_bytes()),
            "same.csv" => Some(b"a,b\n".to_vec()),
            _ => None,
        }
    };
    let (old, new) = (read("<svg/>", "1"), read("<svg></svg>", "2"));
    let v = |read| Version {
        label: "v".into(),
        entry: "main.typ".into(),
        read,
    };
    let r = redline(&v(&old), &v(&new), FULL, &History::default(), Moves::Find, None);
    assert_eq!(r.bib.files, ["data/d.json", "fig.svg"]);
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
    without_ids(&src)
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
fn files_read_through_a_computed_path_are_marked_or_listed() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("computed-path");
    let main = "#let chapter = \"intro\"\n#let data = json(chapter + \".json\")\n#include chapter + \".typ\"\n\nValue: #data.v\n";
    for (side, word) in [("o", "old"), ("n", "new")] {
        std::fs::create_dir(dir.join(side)).unwrap();
        std::fs::write(dir.join(side).join("main.typ"), main).unwrap();
        std::fs::write(dir.join(side).join("intro.typ"), format!("The {word} chapter.\n")).unwrap();
        std::fs::write(dir.join(side).join("intro.json"), format!("{{\"v\": \"{word}\"}}")).unwrap();
    }
    let out = cli(&dir, &["o/main.typ", "n/main.typ", "--keep-typ"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    if stderr.contains("the included path is computed") {
        eprintln!("typst before 0.14 does not tell which files it read, skipping");
        return;
    }
    let kept = Path::new(stderr.lines().find_map(|l| l.strip_prefix("kept ")).unwrap()).to_path_buf();
    let read = |p: &str| without_ids(&std::fs::read_to_string(kept.join(p)).unwrap());
    let (intro, entry) = (read("intro.typ"), read("main.typ"));
    std::fs::remove_dir_all(&kept).unwrap();
    assert!(intro.contains("#redline-ins()[new]"), "{intro}");
    assert!(entry.contains("files: (\"intro.json\",)"), "{entry}");
}

#[test]
fn working_copy_is_compared_with_head_by_default() {
    if !typst_available() {
        return;
    }
    let dir = repo("default-head", &["alpha", "beta"]);
    std::fs::write(dir.join("doc/main.typ"), doc("gamma")).unwrap();
    let src = marked(&dir.join("doc"), &["main.typ"], "main.typ");
    assert_compared(&src, "beta", "gamma");
    // No list of commits that holds only the uncommitted changes.
    assert!(!src.contains("uncommitted"), "{src}");
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
fn since_reviewed_compares_with_the_marked_commit() {
    if !typst_available() {
        return;
    }
    let dir = repo("reviewed", &["alpha", "beta"]);
    std::fs::write(dir.join("doc/main.typ"), doc("gamma")).unwrap();
    let out = cli(&dir.join("doc"), &["main.typ", "--mark-reviewed"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("uncommitted"));
    commit(&dir, "doc/main.typ", "gamma");
    std::fs::write(dir.join("doc/main.typ"), doc("delta")).unwrap();
    let src = marked(&dir, &["doc/main.typ", "--since", "reviewed"], "main.typ");
    assert_compared(&src, "beta", "delta");
}

#[test]
fn reviewed_mark_belongs_to_one_document() {
    let dir = repo("reviewed-per-doc", &["alpha"]);
    commit(&dir, "doc/other.typ", "beta");
    std::fs::write(dir.join("doc/other.typ"), doc("gamma")).unwrap();
    let out = cli(&dir, &["doc/main.typ", "--mark-reviewed"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && !stderr.contains("uncommitted"), "{stderr}");
    let out = cli(&dir, &["doc/other.typ", "--since", "reviewed"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--mark-reviewed"));
}

#[test]
fn reviewed_branch_takes_precedence_over_the_mark() {
    if !typst_available() {
        return;
    }
    let dir = repo("reviewed-branch", &["alpha", "beta"]);
    assert!(cli(&dir, &["doc/main.typ", "--mark-reviewed"]).status.success());
    git(&dir, &["branch", "reviewed", "c-alpha"]);
    std::fs::write(dir.join("doc/main.typ"), doc("gamma")).unwrap();
    let src = marked(&dir, &["doc/main.typ", "--since", "reviewed"], "main.typ");
    assert_compared(&src, "alpha", "gamma");
    let out = cli(&dir, &["doc/main.typ", "--since", "reviewed"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("refs/heads/reviewed"));
}

#[test]
fn mark_reviewed_needs_the_file_in_the_last_commit() {
    let dir = repo("reviewed-missing", &["alpha"]);
    std::fs::write(dir.join("doc/new.typ"), doc("beta")).unwrap();
    for file in ["doc/missing.typ", "doc/new.typ"] {
        let out = cli(&dir, &[file, "--mark-reviewed"]);
        assert_eq!(out.status.code(), Some(1), "{file}");
    }
}

#[test]
fn mark_reviewed_with_steps_is_a_usage_error() {
    let dir = temp_dir("mark-reviewed-steps");
    let out = cli(&dir, &["main.typ", "--mark-reviewed", "--each-commit"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.code() == Some(2) && stderr.contains("--each-commit"),
        "{stderr}"
    );
}

#[test]
fn reviewed_mark_ignores_the_case_of_the_name_given() {
    let dir = repo("reviewed-case", &["alpha"]);
    // Only a case-insensitive filesystem finds the file under another case.
    if !dir.join("doc/MAIN.typ").exists() {
        return;
    }
    assert!(cli(&dir, &["doc/MAIN.typ", "--mark-reviewed"]).status.success());
    git(&dir, &["pack-refs", "--all"]);
    let out = cli(&dir, &["doc/main.typ", "--since", "reviewed", "-o", "out.pdf"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("no commit is marked"), "{stderr}");
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
    let whole = stderr.contains("marked whole") && !stderr.contains("shown by name");
    assert!(out.status.success() && whole, "{stderr}");
}

#[test]
fn changed_drawing_that_needs_a_finite_width_compiles() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("finite-width");
    let doc = |w: &str| {
        format!(
            "#let chart(n) = layout(s => {{ assert(s.width < 1e4pt); rect(width: s.width)[#n] }})\n\nBefore.\n\n#chart(\"{w}\")\n\nAfter.\n"
        )
    };
    std::fs::write(dir.join("old.typ"), doc("alpha")).unwrap();
    std::fs::write(dir.join("new.typ"), doc("beta")).unwrap();
    let out = cli(&dir, &["old.typ", "new.typ", "-o", "out.pdf"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

fn pdf_pages(path: &Path) -> usize {
    let pdf = String::from_utf8_lossy(&std::fs::read(path).unwrap()).replace("/Type /Page", "/Type/Page");
    pdf.matches("/Type/Page").count() - pdf.matches("/Type/Pages").count()
}

#[test]
fn inserted_content_that_sets_the_page_adds_only_its_own_pages() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("sets-page");
    let slides = "#let plain(t) = { set page(fill: luma(240)); heading(t) }\n#let computed(t) = context { set page(fill: luma(240)); heading(t) }\n";
    std::fs::write(dir.join("old.typ"), format!("{slides}#plain[One]\n")).unwrap();
    let new = format!("{slides}#plain[One]\n\n#plain[Two]\n\n#computed[Three]\n");
    std::fs::write(dir.join("new.typ"), &new).unwrap();
    let out = cli(&dir, &["old.typ", "new.typ", "-o", "out.pdf"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // The summary and the three pages of the new document.
    assert_eq!(pdf_pages(&dir.join("out.pdf")), 4);
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
    assert!(composite(&side, &side, &[], &bib, &History::default()).contains("\"c\""));
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
    // The summary and the one changed pair.
    assert_eq!(pdf_pages(&dir.join("out.pdf")), 2);
}

#[test]
fn side_by_side_shows_a_pure_move() {
    if !typst_available() || typst_before_0_14() {
        return;
    }
    let dir = temp_dir("side-by-side-move");
    for side in ["old", "new"] {
        let src = read_fixture("moved-paragraph", side, "main.typ").unwrap();
        std::fs::write(dir.join(format!("{side}.typ")), src).unwrap();
    }
    let out = cli(
        &dir,
        &["old.typ", "new.typ", "-o", "out.pdf", "--layout", "side-by-side"],
    );
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let pdf =
        String::from_utf8_lossy(&std::fs::read(dir.join("out.pdf")).unwrap()).replace("/Type /Page", "/Type/Page");
    let pages = pdf.matches("/Type/Page").count() - pdf.matches("/Type/Pages").count();
    // The summary and the pages that show the move.
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

#[test]
fn each_change_is_marked_with_the_commit_that_made_it() {
    let out = &redline_steps(
        "The lazy fox jumps.\n",
        "The quick brown fox jumps.\n",
        "The quick red fox jumps.\n",
    )
    .files[0]
        .1;
    let marks = steps_of_marks(out);
    for (step, body) in [(1, "lazy"), (1, "quick"), (2, "red")] {
        assert!(marks.contains(&(step, body.into())), "{out}");
    }
}

/// The redline of `new` against `old` with each change marked with its
/// step, `mid` being the version after the first.
fn redline_steps(old: &str, mid: &str, new: &str) -> Redline {
    let read = |s: &str| {
        let s = s.as_bytes().to_vec();
        move |_: &str| Some(s.clone())
    };
    let (old, mid, new) = (read(old), read(mid), read(new));
    let v = |read| Version {
        label: "v".into(),
        entry: "main.typ".into(),
        read,
    };
    let history = History {
        steps: Some(vec![v(&mid)]),
        ..History::default()
    };
    redline(&v(&old), &v(&new), FULL, &history, Moves::Find, None)
}

/// The step and the body of each mark in marked source.
fn steps_of_marks(src: &str) -> Vec<(u8, String)> {
    src.split("#redline-")
        .skip(1)
        .filter_map(|m| {
            let (args, rest) = m.split_once(")[")?;
            let step = args.split("step: ").nth(1)?.split([',', ' ']).next()?;
            Some((step.parse().ok()?, rest.split("];").next()?.to_string()))
        })
        .collect()
}

#[test]
fn paragraph_inserted_in_two_steps_keeps_its_lines() {
    let para = |inline: &str| format!("#set page(width: 10cm, height: auto)\nA new paragraph{inline} with words.\n");
    let new = para(" #context [here]");
    let r = redline_steps("", &para(""), &new);
    let marked = [r.files[0].clone(), (PREAMBLE_PATH.into(), PREAMBLE.into())];
    let Some(marked) = render("two-steps", &marked) else {
        return;
    };
    let new = render("two-steps-new", &[("main.typ".into(), new)]).unwrap();
    let height = |png: &[u8]| u32::from_be_bytes(png[20..24].try_into().unwrap());
    assert_eq!(height(&marked[0]), height(&new[0]), "{}", r.files[0].1);
}

#[test]
fn move_is_marked_with_the_commit_that_made_it() {
    let old = read_fixture("moved-paragraph", "old", "main.typ").unwrap();
    let mid = read_fixture("moved-paragraph", "new", "main.typ").unwrap();
    let new = mid.replace("dawn", "dusk");
    let (old, mid, new) = (
        |_: &str| Some(old.clone().into_bytes()),
        |_: &str| Some(mid.clone().into_bytes()),
        |_: &str| Some(new.clone().into_bytes()),
    );
    let v = |read| Version {
        label: "v".into(),
        entry: "main.typ".into(),
        read,
    };
    let history = History {
        steps: Some(vec![v(&mid)]),
        ..History::default()
    };
    let out = &without_ids(&redline(&v(&old), &v(&new), FULL, &history, Moves::Find, None).files[0].1);
    let moves: Vec<&str> = out.lines().filter(|l| l.contains("move: (")).collect();
    assert!(
        !moves.is_empty() && moves.iter().all(|l| l.contains("step: 1)")),
        "{out}"
    );
    assert!(out.contains("step: 2)[dusk]"), "{out}");
}

#[test]
fn summary_lists_the_commits_since_the_old_revision() {
    if !typst_available() {
        return;
    }
    let dir = repo("commit-list", &["alpha", "beta", "gamma"]);
    std::fs::write(dir.join("doc/main.typ"), doc("delta")).unwrap();
    let src = marked(&dir, &["doc/main.typ", "--since", "c-alpha"], "main.typ");
    let subjects: Vec<&str> = src
        .split("subject: ")
        .skip(1)
        .map(|s| s.split(')').next().unwrap())
        .collect();
    assert_eq!(subjects, ["\"beta\"", "\"gamma\""], "{src}");
    assert!(src.contains("uncommitted: true"), "{src}");
}

#[test]
fn each_commit_compiles_with_the_commits_coloured() {
    if !typst_available() {
        return;
    }
    let dir = repo("each-commit", &["alpha", "beta", "gamma"]);
    let src = marked(
        &dir,
        &["doc/main.typ", "--since", "c-alpha", "--each-commit"],
        "main.typ",
    );
    let gamma = steps_of_marks(&src)
        .into_iter()
        .any(|(s, b)| s == 2 && b.starts_with("gamma"));
    assert!(gamma, "{src}");
}

#[test]
fn each_commit_follows_a_renamed_file() {
    if !typst_available() {
        return;
    }
    let dir = repo("each-commit-rename", &["alpha", "beta"]);
    git(&dir, &["mv", "doc/main.typ", "doc/paper.typ"]);
    git(&dir, &["commit", "-q", "-m", "rename"]);
    let args = ["doc/paper.typ", "--since", "c-alpha", "--each-commit"];
    let src = marked(&dir, &args, "paper.typ");
    assert!(steps_of_marks(&src).contains(&(1, "beta".into())), "{src}");
}

#[test]
fn each_commit_refuses_more_commits_than_colours() {
    let dir = repo("many-commits", &["a", "b", "c", "d", "e", "f", "g", "h"]);
    let out = cli(&dir, &["doc/main.typ", "--since", "c-a", "--each-commit"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.code() == Some(1) && stderr.contains("7 commits"), "{stderr}");
}

#[test]
fn commits_that_only_touch_other_files_are_not_listed() {
    if !typst_available() {
        return;
    }
    let dir = repo("other-files", &["alpha"]);
    std::fs::write(dir.join("doc/notes.typ"), "Notes.").unwrap();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "notes"]);
    commit(&dir, "doc/main.typ", "beta");
    let src = marked(&dir, &["doc/main.typ", "--since", "c-alpha"], "main.typ");
    assert!(src.contains("\"beta\"") && !src.contains("\"notes\""), "{src}");
}

#[test]
fn each_commit_needs_the_old_revision_to_be_an_ancestor() {
    let dir = repo("not-ancestor", &["alpha", "beta"]);
    git(&dir, &["checkout", "-q", "-b", "side", "c-alpha"]);
    commit(&dir, "doc/main.typ", "gamma");
    let out = cli(&dir, &["doc/main.typ", "--since", "main", "--each-commit"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.code() == Some(1) && stderr.contains("--since main..."),
        "{stderr}"
    );
}

#[test]
fn each_commit_needs_one_file() {
    let dir = temp_dir("each-commit-usage");
    let out = cli(&dir, &["old.typ", "new.typ", "--each-commit"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn each_commit_side_by_side_colours_both_sides_by_commit() {
    if !typst_available() || typst_before_0_14() {
        return;
    }
    let dir = repo("each-commit-side", &["alpha", "beta", "gamma"]);
    let args = [
        "doc/main.typ",
        "--since",
        "c-alpha",
        "--each-commit",
        "--layout",
        "side-by-side",
        "--keep-typ",
    ];
    let out = cli(&dir, &args);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    let kept = Path::new(stderr.lines().find_map(|l| l.strip_prefix("kept ")).unwrap());
    let read = |p: &str| without_ids(&std::fs::read_to_string(kept.join(p)).unwrap());
    let (old, new) = (read(".redline-old-side/main.typ"), read("main.typ"));
    std::fs::remove_dir_all(kept).unwrap();
    // beta replaced alpha, gamma replaced beta.
    assert!(old.contains("step: 1)[alpha"), "{old}");
    assert!(new.contains("step: 2)[gamma"), "{new}");
}

#[test]
fn each_commit_needs_the_preamble_function() {
    let dir = repo("each-commit-preamble", &["alpha", "beta"]);
    std::fs::write(dir.join("p.typ"), PREAMBLE.replace("redline-step-style", "other")).unwrap();
    let out = cli(
        &dir,
        &[
            "doc/main.typ",
            "--since",
            "c-alpha",
            "--each-commit",
            "--preamble",
            "p.typ",
        ],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("does not define redline-step-style"), "{stderr}");
}

/// A repository whose `doc/main.typ` gains a paragraph per commit: `lines`
/// of `Commit <i>.` each in commit `i`, tagged `s<i>`.
fn growing_repo(name: &str, lines: &[usize]) -> TempDir {
    let dir = repo(name, &["base"]);
    let mut text = doc("base");
    for (i, n) in lines.iter().enumerate() {
        for _ in 0..*n {
            text.push_str(&format!("\nCommit {}.\n", i + 1));
        }
        std::fs::write(dir.join("doc/main.typ"), &text).unwrap();
        git(&dir, &["commit", "-q", "-am", &format!("commit {}", i + 1)]);
        git(&dir, &["tag", &format!("s{}", i + 1)]);
    }
    dir
}

/// The step of the first mark that contains `text`.
fn step_of(src: &str, text: &str) -> Option<u8> {
    steps_of_marks(src)
        .into_iter()
        .find(|(_, b)| b.contains(text))
        .map(|(s, _)| s)
}

#[test]
fn steps_end_at_the_given_revisions() {
    if !typst_available() {
        return;
    }
    let dir = growing_repo("steps", &[1; 6]);
    let args = ["doc/main.typ", "--since", "c-base", "--steps", "s2,s4"];
    let src = marked(&dir, &args, "main.typ");
    let steps: Vec<_> = (1..=6).map(|i| step_of(&src, &format!("Commit {i}."))).collect();
    let expected = [1, 1, 2, 2, 3, 3].map(Some);
    assert_eq!(steps, expected, "{src}");
}

#[test]
fn steps_with_a_number_balance_the_changes() {
    if !typst_available() {
        return;
    }
    // An even split by number of commits would cut after the second.
    let dir = growing_repo("steps-count", &[12, 1, 1, 1, 1]);
    let args = ["doc/main.typ", "--since", "c-base", "--steps", "2"];
    let src = marked(&dir, &args, "main.typ");
    assert_eq!(step_of(&src, "Commit 1."), Some(1), "{src}");
    assert_eq!(step_of(&src, "Commit 2."), Some(2), "{src}");
}

#[test]
fn steps_with_more_steps_than_changes_make_each_a_step() {
    if !typst_available() {
        return;
    }
    let dir = growing_repo("steps-few", &[1, 1]);
    let path = dir.join("doc/main.typ");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        text + "
Uncommitted.
",
    )
    .unwrap();
    let args = ["doc/main.typ", "--since", "c-base", "--steps", "4"];
    let src = marked(&dir, &args, "main.typ");
    let steps: Vec<_> = ["Commit 1.", "Commit 2.", "Uncommitted."]
        .iter()
        .map(|t| step_of(&src, t))
        .collect();
    assert_eq!(steps, [1, 2, 3].map(Some), "{src}");
}

#[test]
fn step_off_the_first_parent_path_is_an_error() {
    let dir = repo("steps-off-path", &["alpha", "beta"]);
    git(&dir, &["checkout", "-q", "-b", "side", "c-alpha"]);
    commit(&dir, "doc/side.typ", "gamma");
    git(&dir, &["checkout", "-q", "main"]);
    git(&dir, &["merge", "-q", "--no-ff", "-m", "merge", "side"]);
    commit(&dir, "doc/main.typ", "delta");
    let out = cli(&dir, &["doc/main.typ", "--since", "c-alpha", "--steps", "c-gamma"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.code() == Some(1) && stderr.contains("c-gamma"), "{stderr}");
}

#[test]
fn step_ending_at_the_new_version_is_an_error() {
    let dir = repo("steps-at-new", &["alpha", "beta"]);
    let out = cli(
        &dir,
        &["doc/main.typ", "--since", "c-alpha", "--to", "HEAD", "--steps", "HEAD"],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.code() == Some(1) && stderr.contains("last step already ends"),
        "{stderr}"
    );
}

#[test]
fn steps_and_each_commit_together_are_a_usage_error() {
    let dir = temp_dir("steps-usage");
    let out = cli(&dir, &["main.typ", "--steps", "3", "--each-commit"]);
    assert_eq!(out.status.code(), Some(2));
}
