use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use typst_redline::{
    Change, ChangeKind, History, Moves, OLD_DIR, Options, PREAMBLE, PREAMBLE_PATH, Pair, Placed, PlacedKind, Redline,
    Version, What, pair_pages, redline,
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
        out.contains("\n  - Nested two\n    #redline-del(item: (depth: 2, number: none, inside: true, tight: true))[Nested gone];\n- Third item"),
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
    assert!(out.contains("日本語#redline-ins(bare: \"both\")[の];テキスト"), "{out}");
}

#[test]
fn marks_hold_whole_characters() {
    let out = body("whole-characters");
    for (old, new) in [
        ("👍🏽", "👍🏻"),
        ("👨\u{200d}👩\u{200d}👧", "👨\u{200d}👩\u{200d}👦"),
        ("Cafe\u{301}", "Cafe\u{300}"),
        ("পড়ে", "পড়ো"),
        ("தமிழில்", "தமிழின்"),
    ] {
        assert!(
            out.contains(&format!("#redline-del(gap: true)[{old}];#redline-ins()[{new}];")),
            "{out}"
        );
    }
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
        out.contains("#redline-ins(block: true)[A whole new paragraph.]")
            && out.contains("#redline-ins(block: true)[And more here.]"),
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
fn deleted_code_refers_to_the_labels_it_passes() {
    compile("deleted-label-reference", &run("deleted-label-reference").files);
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
fn renamed_heading_label_is_listed_not_marked() {
    let r = run("heading-label");
    let changes: Vec<_> = r
        .changes
        .iter()
        .map(|c| (c.kind, c.old.as_str(), c.text.as_str()))
        .collect();
    assert_eq!(changes, [(ChangeKind::Unmarked, "<m>", "<methods>")], "{}", r.body);
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
fn setup_code_moved_unchanged_is_listed_once_and_marks_nothing() {
    let r = run("moved-setup");
    assert!(
        matches!(&r.changes[..], [c] if c.kind == ChangeKind::Unmarked && c.old == c.text
            && matches!(&c.what[..], [What::Note(n)] if n.contains("between"))),
        "{:?}",
        r.changes
    );
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
    let labels = query_field(&dir, "<redline-move-to-0>", "label");
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

/// Slides as headings that each start a page, the "Cost" slide moved two
/// slides later.
fn reordered_slides() -> (String, String) {
    let slide = |h: &str, body: &str| format!("= {h}\n{body}\n\n");
    let [outline, cost, pivot, cond] = [
        slide("Outline", "Linear systems and how to solve them."),
        slide("Cost", "#table(columns: 2, [Gauss], [two thirds n cubed flops])"),
        slide("Pivoting", "Without pivoting small pivots amplify rounding errors."),
        slide("Condition", "A large condition number means the solution is sensitive."),
    ];
    let rule = "#set page(width: 16cm, height: 9cm)\n#show heading: it => { pagebreak(weak: true); it }\n";
    (
        format!("{rule}{outline}{cost}{pivot}{cond}"),
        format!("{rule}{outline}{pivot}{cond}{cost}"),
    )
}

#[test]
fn moved_slide_starts_on_its_own_page() {
    let (old, new) = reordered_slides();
    let r = redline_of(&old, &new);
    let mut files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat();
    files[0].1 += "\n#import \"/.redline-preamble.typ\": redline-move-start\n#context [#metadata(redline-move-start(0).page())<probe>]\n";
    let Some(dir) = compile("moved-slide", &files) else {
        return;
    };
    // The page of the "Cost" heading, not the end of the page before it.
    assert_eq!(query_field(&dir, "<probe>", "value"), "[4]");
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

/// A glyph or image on a page: what it shows, where, in points, and in
/// which colour.
#[derive(Debug)]
struct Glyph {
    id: String,
    x: f64,
    y: f64,
    fill: Option<String>,
}

/// The glyphs and images on each page of a project, from its SVG rendering,
/// whatever their colour.
fn glyphs(name: &str, files: &[(String, String)]) -> Option<Vec<Vec<Glyph>>> {
    let dir = compile_to(name, files, "page-{n}.svg")?;
    let mut pages: Vec<PathBuf> = std::fs::read_dir(&*dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "svg"))
        .collect();
    pages.sort_by_key(|p| p.file_stem().unwrap().to_str().unwrap()[5..].parse::<usize>().unwrap());
    Some(
        pages
            .iter()
            .map(|p| svg_glyphs(&std::fs::read_to_string(p).unwrap()))
            .collect(),
    )
}

fn svg_glyphs(svg: &str) -> Vec<Glyph> {
    type Affine = [f64; 6];
    let then = |m: Affine, n: Affine| -> Affine {
        [
            m[0] * n[0] + m[2] * n[1],
            m[1] * n[0] + m[3] * n[1],
            m[0] * n[2] + m[2] * n[3],
            m[1] * n[2] + m[3] * n[3],
            m[0] * n[4] + m[2] * n[5] + m[4],
            m[1] * n[4] + m[3] * n[5] + m[5],
        ]
    };
    let attr = |tag: &str, key: &str| {
        let i = tag.find(&format!(" {key}=\""))? + key.len() + 3;
        Some(tag[i..i + tag[i..].find('"')?].to_string())
    };
    let transform = |tag: &str| -> Affine {
        let Some(t) = attr(tag, "transform") else {
            return [1., 0., 0., 1., 0., 0.];
        };
        let (kind, args) = t.trim_end_matches(')').split_once('(').unwrap();
        let v: Vec<f64> = args
            .split([' ', ','])
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().unwrap())
            .collect();
        match (kind, v.as_slice()) {
            ("matrix", &[a, b, c, d, e, f]) => [a, b, c, d, e, f],
            ("translate", &[x]) => [1., 0., 0., 1., x, 0.],
            ("translate", &[x, y]) => [1., 0., 0., 1., x, y],
            ("scale", &[s]) => [s, 0., 0., s, 0., 0.],
            ("scale", &[sx, sy]) => [sx, 0., 0., sy, 0., 0.],
            _ => panic!("transform {t}"),
        }
    };
    let mut out = Vec::new();
    let mut stack = vec![[1., 0., 0., 1., 0., 0.]];
    let mut defs = 0;
    for tag in svg.split('<').skip(1).map(|t| &t[..t.find('>').unwrap()]) {
        let closed = tag.ends_with('/');
        let name = tag.split(' ').next().unwrap().trim_end_matches('/');
        match name {
            "defs" if !closed => defs += 1,
            "/defs" => defs -= 1,
            _ if defs > 0 => {}
            "g" if !closed => stack.push(then(*stack.last().unwrap(), transform(tag))),
            "/g" => {
                stack.pop();
            }
            "use" | "image" => {
                let m = then(*stack.last().unwrap(), transform(tag));
                let num = |k| attr(tag, k).map_or(0., |v| v.parse().unwrap());
                let (x, y) = (num("x"), num("y"));
                let id = attr(tag, "xlink:href").or_else(|| attr(tag, "href")).unwrap();
                // An image by its size and the start of its data.
                let id = if name == "image" {
                    format!("{}x{}:{}", num("width"), num("height"), &id[..id.len().min(200)])
                } else {
                    id
                };
                out.push(Glyph {
                    id,
                    x: m[0] * x + m[2] * y + m[4],
                    y: m[1] * x + m[3] * y + m[5],
                    fill: attr(tag, "fill"),
                });
            }
            _ => {}
        }
    }
    out
}

/// Where the glyphs and images of `new` are not where `proof` has them, as
/// lines naming page and place; `proof` may show more only outside the area
/// that `new` fills on the page, in the margins. A mark's colour makes its
/// text a run of its own, shaped without kerning against the text next to
/// it, which moves a line's glyphs by less than a point sideways.
fn moved_glyphs(mut proof: &[Vec<Glyph>], new: &[Vec<Glyph>]) -> Vec<String> {
    // Typst 0.11 ends the proof with the page the summary starts.
    if proof.len() == new.len() + 1 && typst_version().starts_with("0.11.") {
        proof = &proof[..new.len()];
    }
    if proof.len() != new.len() {
        return vec![format!("{} pages instead of {}", proof.len(), new.len())];
    }
    let mut out = Vec::new();
    for (p, (proof, new)) in proof.iter().zip(new).enumerate() {
        let near = |a: &Glyph, b: &Glyph| a.id == b.id && (a.x - b.x).abs() < 1.0 && (a.y - b.y).abs() < 0.01;
        let mut used = vec![false; proof.len()];
        for g in new {
            match (0..proof.len()).find(|&i| !used[i] && near(&proof[i], g)) {
                Some(i) => used[i] = true,
                None => out.push(format!("page {}: {g:?} missing", p + 1)),
            }
        }
        let fold = |f: fn(f64, f64) -> f64, init, v: fn(&Glyph) -> f64| new.iter().map(v).fold(init, f);
        let (x0, x1) = (fold(f64::min, f64::MAX, |g| g.x), fold(f64::max, f64::MIN, |g| g.x));
        let (y0, y1) = (fold(f64::min, f64::MAX, |g| g.y), fold(f64::max, f64::MIN, |g| g.y));
        for (g, _) in proof.iter().zip(&used).filter(|(_, u)| !**u) {
            if (x0..=x1).contains(&g.x) && (y0..=y1).contains(&g.y) {
                out.push(format!("page {}: {g:?} extra", p + 1));
            }
        }
    }
    out
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

/// The ids of the marks that the compiled one-file redline `r` does not lay
/// out; `None` without typst.
fn lost_marks(name: &str, r: Redline) -> Option<String> {
    let files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat();
    let dir = compile(name, &files)?;
    Some(query_field(&dir, "<redline-lost>", "key"))
}

#[test]
fn code_that_shows_the_same_after_a_change_is_listed_not_marked() {
    let Some(dir) = compile("same-output", &run("inspected-arguments").files) else {
        return;
    };
    assert_eq!(query_field(&dir, "<redline-lost>", "key"), r#"["0,1"]"#);
}

#[test]
fn change_under_a_heading_that_a_template_drops_is_listed() {
    let doc = |w: &str| format!("#show: doc => [Title.]\n= Draft\nThe {w} text.\n");
    if let Some(lost) = lost_marks("dropped-heading", redline_of(&doc("old"), &doc("new"))) {
        assert_eq!(lost, r#"["0,1"]"#);
    }
}

#[test]
fn code_whose_styles_changed_is_marked() {
    let doc = |c: &str| format!("#let f(c) = {{ set text(fill: c); [Word] }}\n#f({c})\n");
    if let Some(lost) = lost_marks("styled-output", redline_of(&doc("red"), &doc("blue"))) {
        assert_eq!(lost, "[]");
    }
}

#[test]
fn figure_whose_body_changed_shows_its_caption_once() {
    let Some(dir) = compile("figure-body", &run("changed-shape").files) else {
        return;
    };
    let captions = query_field(&dir, "figure", "caption");
    assert_eq!(captions.matches("shape").count(), 1, "{captions}");
}

#[test]
fn figure_whose_body_changed_is_the_new_figure() {
    let doc = |body: &str| {
        format!(
            "#show figure: it => [#metadata(repr(it.kind) + repr(it.body.func()))<probe>#it]
See @f.

#figure({body}, caption: [Same.]) <f>
"
        )
    };
    let (old, new) = (doc("raw(\"x = 1\", block: true)"), doc("table(columns: 2)[a][b]"));
    if let Some((marked, plain)) = probes("figure-kind", &old, &new) {
        assert_eq!(marked, plain);
    }
}

#[test]
fn code_whose_label_changed_is_marked() {
    let doc = |l: &str| {
        format!(
            "#show <hot>: set text(red)
#let tag(l) = [#strong[Note] #label(l)]
#tag(\"{l}\")
"
        )
    };
    if let Some(lost) = lost_marks("label-output", redline_of(&doc("hot"), &doc("cold"))) {
        assert_eq!(lost, "[]");
    }
}

#[test]
fn deletions_not_laid_out_are_each_listed() {
    let marks = r#"((id: 0, old: "a", new: ""), (id: 1, joined: true, old: "b", new: ""), (id: 2, joined: true, old: "", new: "c"))"#;
    let main = format!(
        "#import \"/{PREAMBLE_PATH}\": redline-joined
#metadata(redline-joined({marks}).map(m => m.old + \"/\" + m.new))<probe>
"
    );
    let files = [("main.typ".to_string(), main), (PREAMBLE_PATH.into(), PREAMBLE.into())];
    if let Some(dir) = compile("joined-deletions", &files) {
        assert_eq!(query(&dir, "<probe>"), r#"[["a/","b/c"]]"#);
    }
}

#[test]
fn call_whose_changed_argument_is_not_content_is_replaced_whole() {
    let doc = |c: &str| format!("Text.\n\n#rect(fill: rgb(\"{c}\"))[Box]\n");
    if let Some(lost) = lost_marks("value-argument", redline_of(&doc("#ff0000"), &doc("#0000ff"))) {
        assert_eq!(lost, "[]");
    }
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
        assert!(marked.matches("[\"").count() >= 2, "{marked}");
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
fn bar_of_chinese_text_starts_on_the_line_its_change_starts() {
    let doc = |w: &str| {
        format!(
            "#set text(size: 10pt)\n#show line: it => context [#metadata(here().position().y.pt())<probe>#it]\n#show \"{w}\": it => context [#metadata(-here().position().y.pt())<probe>#it]\n第一行 \\\n{w}的功能。\n"
        )
    };
    if let Some((marked, _)) = probes("cjk-bar-line", &doc("它"), &doc("这")) {
        // The tops of the bars, and the baselines of the changed text as
        // negative numbers.
        let ys: Vec<f64> = marked
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|y| y.parse().unwrap())
            .collect();
        let text = ys.iter().filter(|y| **y < 0.0).map(|y| -y).fold(f64::MAX, f64::min);
        let bars: Vec<&f64> = ys.iter().filter(|y| **y > 0.0).collect();
        assert!(!bars.is_empty() && bars.iter().all(|&&y| y > text - 12.0), "{marked}");
    }
}

#[test]
fn deletion_in_right_to_left_text_compiles() {
    let doc = |w: &str| format!("#set text(dir: rtl, lang: \"ar\")\nمرحبا {w}بالعالم\n");
    let r = redline_of(&doc("جديد "), &doc(""));
    compile(
        "rtl-deletion",
        &[r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat(),
    );
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
    let files = [r.files, vec![(PREAMBLE_PATH.into(), without_deletions())]].concat();
    let Some(marked) = compile("roomless-mark", &files) else {
        return;
    };
    let new = compile("roomless-new", &[("main.typ".into(), doc("blue"))]).unwrap();
    assert_eq!(query(&marked, "<probe>"), query(&new, "<probe>"));
}

#[test]
fn tool_text_keeps_its_look_under_the_documents_text_rules() {
    // The glyphs of the pointer line above the text area, and of the change
    // number in the left margin by what they show and their colour.
    let tool_text = |name: &str, rules: &str| {
        let r = redline_of(&format!("{rules}Alpha beta.\n"), &format!("{rules}Alpha gamma.\n"));
        let files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat();
        let page = glyphs(name, &files)?.remove(0);
        let shown = |g: &Glyph| match g.y < 60. {
            true => format!("{} {:?} {:.1} {:.1}", g.id, g.fill, g.x, g.y),
            false => format!("{} {:?}", g.id, g.fill),
        };
        Some(
            page.iter()
                .filter(|g| g.x < 60. || g.y < 60.)
                .map(shown)
                .collect::<Vec<_>>(),
        )
    };
    let Some(plain) = tool_text("tool-text-plain", "") else {
        return;
    };
    let styled = tool_text(
        "tool-text-styled",
        "#show text: set text(fill: red)\n#set text(size: 20pt, weight: \"bold\", style: \"italic\", tracking: 1pt)\n",
    )
    .unwrap();
    assert!(plain.len() > 1);
    assert_eq!(plain, styled);
}

/// The `<probe>` values of the marked document and of the old version.
fn old_probes(name: &str, old: &str, new: &str) -> Option<(String, String)> {
    let r = redline_of(old, new);
    let files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat();
    let marked = compile(&format!("{name}-marked"), &files)?;
    let plain = compile(&format!("{name}-old"), &[("main.typ".into(), old.into())]).unwrap();
    Some((query(&marked, "<probe>"), query(&plain, "<probe>")))
}

const PROBE_Y: &str = "#context [#metadata(here().position().y)<probe>]";

#[test]
fn deleted_line_break_does_not_break_the_line() {
    if let Some((marked, new)) = probes(
        "line-break",
        &format!("One \\ two.{PROBE_Y}\n"),
        &format!("One two.{PROBE_Y}\n"),
    ) {
        assert_eq!(marked, new);
    }
}

#[test]
fn deleted_update_takes_no_room() {
    let doc = |u: &str| format!("#let st = state(\"s\", 0)\n{u}\n= Head\nNow.{PROBE_Y}\n");
    if let Some((marked, new)) = probes("deleted-update", &doc("\n#st.update(5)\n"), &doc("")) {
        assert_eq!(marked, new);
    }
}

#[test]
fn deleted_code_that_draws_nothing_is_listed_with_its_code() {
    let r = run("deleted-code-in-text");
    let Some(dir) = compile("unseen-deletions", &r.files) else {
        return;
    };
    let ids = query_field(&dir, "<redline-unseen>", "key");
    let texts: Vec<&str> = ids
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|i| Some(r.changes.get(i.parse::<usize>().ok()?)?.text.trim()))
        .collect();
    assert_eq!(
        texts,
        [
            "#total.update(x => x + 5)",
            "#counter(\"c\").step()",
            "#metadata(\"x\")"
        ],
        "{ids}"
    );
}

#[test]
fn old_code_whose_names_meant_something_else_there_is_marked() {
    let cases = [
        (
            "#let unit = [cm]\n#{ let unit = [in]; [Width: 3 #unit] }\n",
            "#unit",
            "#[cm]",
        ),
        (
            "#let f = [Apple]\nFirst #f.\n\n#let f = [Pear]\nLast #f.\n",
            "First #f",
            "First #[Pear]",
        ),
    ];
    for (i, (old, from, to)) in cases.into_iter().enumerate() {
        let r = redline_of(old, &old.replacen(from, to, 1));
        if let Some(lost) = lost_marks(&format!("old-scope-{i}"), r) {
            assert_eq!(lost, "[]", "{old}");
        }
    }
}

#[test]
fn deleted_item_is_numbered_from_the_enumerations_start() {
    let doc = |items: &str| {
        format!(
            "#set enum(numbering: \"(a)\", start: 3)\n#show regex(\"\\(\\w\\)\"): it => [#metadata(it.text)<probe>#it]\n{items}"
        )
    };
    if let Some((marked, _)) = probes("enum-start", &doc("+ One\n+ Two\n+ Three\n"), &doc("+ One\n+ Three\n")) {
        assert_eq!(marked, r#"["(c)","(d)","(d)"]"#);
    }
}

#[test]
fn deleted_items_take_the_room_of_items() {
    let (old, new) = (
        format!("- one\n- two\n- three{PROBE_Y}\n\n+ one\n+ two\n+ three{PROBE_Y}\n"),
        format!("- one\n- three{PROBE_Y}\n\n+ one\n+ three{PROBE_Y}\n"),
    );
    if let Some((marked, old)) = old_probes("deleted-items", &old, &new) {
        assert_eq!(marked, old);
    }
}

#[test]
fn deleted_text_after_setup_code_stays_in_its_paragraph() {
    let doc = |w: &str| format!("Keep #let x = 1\n{w}\n\nNext.{PROBE_Y}\n");
    if let Some((marked, old)) = old_probes("after-setup", &doc("old words."), &doc("")) {
        assert_eq!(marked, old);
    }
}

#[test]
fn deleted_include_of_a_computed_path_is_listed() {
    let r = redline_of("A.\n\n#include \"p\" + \".typ\"\n", "A.\n");
    assert!(
        r.changes
            .iter()
            .any(|c| c.kind == ChangeKind::Unmarked && c.old.contains("#include")),
        "{:?}",
        r.changes
    );
}

#[test]
fn deleted_drawing_that_fits_on_a_page_is_not_split() {
    let doc = |chart: &str| {
        format!(
            "#set page(height: 6cm, margin: 1cm)\n#show rect.where(fill: red): it => context [#metadata(here().page())<probe>#it]\n#lorem(25)\n\n{chart}\n"
        )
    };
    let chart = "#grid(rect(height: 1.8cm, fill: red), rect(height: 1.8cm, fill: red))";
    if let Some((marked, _)) = probes("unsplit-drawing", &doc(chart), &doc("")) {
        assert_eq!(marked, "[2,2]");
    }
}

#[test]
fn blank_line_that_spaces_items_apart_is_marked_and_listed() {
    let probe = "#context [#metadata(here().position().y)<probe>]";
    let (old, new) = (
        format!(
            "- one
- two

{probe}
"
        ),
        format!(
            "- one

- two

{probe}
"
        ),
    );
    let r = redline_of(&old, &new);
    let kinds: Vec<_> = r
        .changes
        .iter()
        .map(|c| (c.kind, matches!(c.what.as_slice(), [What::Note(_)])))
        .collect();
    assert_eq!(kinds, [(ChangeKind::Ins, true)]);
    assert!(
        without_ids(&r.files[0].1).contains("- #redline-ins()[];two"),
        "{}",
        r.files[0].1
    );
    let unchanged = redline_of(
        "One.

Two.
",
        "One.



Two.
",
    );
    assert!(unchanged.changes.is_empty(), "{:?}", unchanged.changes);
    let without_deletions = format!(
        "{PREAMBLE}
#let redline-summary(info) = none
"
    );
    let files = [r.files, vec![(PREAMBLE_PATH.into(), without_deletions)]].concat();
    let Some(marked) = compile("loose-list", &files) else {
        return;
    };
    let new = compile("loose-list-new", &[("main.typ".into(), new)]).unwrap();
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

/// The preamble without the summary and with what was deleted left out:
/// insertions, moves and bars as the proof shows them, and each deletion
/// with what the tool puts around it (tags, paragraph breaks, the restoring
/// of counters) but none of the deleted content, nor the space after it or
/// the marker of a deleted item, which are there for that content. Content
/// that draws nothing is kept: it decides whether the deletion is a block.
fn without_deletions() -> String {
    format!(
        "{PREAMBLE}
#let redline-del-real = redline-del
#let redline-del(body, move: none, item: none, gap: false, block: false, ..args) = if move == none {{
  redline-del-real(if redline-unseen(body) {{ body }} else {{ [] }}, block: block or item != none, ..args)
}}
#let redline-summary(info) = none
"
    )
}

/// Every fixture: each change found is marked and on the summary's list of
/// marks, from which the summary lists those not laid out; the proof with
/// the deletions removed shows the new document's text and images where the
/// new document has them; and the binary makes a PDF in both layouts, whose
/// layout converges.
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
    // Its deleted code reads files that are gone.
    let opts = if name == "deleted-code" { BY_NAME } else { FULL };
    let r = generate(name, &without_deletions(), opts);
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
    let Some(proof) = glyphs(&format!("{name}-proof"), &r.files) else {
        return;
    };
    let new = glyphs(&format!("{name}-new"), &tree(name, "new")).unwrap();
    let moved = moved_glyphs(&proof, &new);
    assert!(
        new.iter().any(|p| !p.is_empty()) && moved.is_empty(),
        "{name}: {}\n{}",
        moved[..moved.len().min(5)].join("\n"),
        r.body
    );
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
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(
            run.status.success() && pdf_pages(&pdf) > 0 && !stderr.contains("did not converge"),
            "{name}, {layout}: {stderr}"
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
fn changed_include_path_is_listed() {
    let r = run("include-renamed");
    assert!(
        r.changes
            .iter()
            .any(|c| c.kind == ChangeKind::Unmarked && c.old.contains("a.typ") && c.text.contains("b.typ")),
        "{:?}",
        r.changes
    );
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
fn listed_template_argument_is_described_by_name_and_values() {
    let src = |title: &str| {
        format!(
            "#let t(title: \"\", body) = {{ align(center, text(20pt, title)); body }}
#show: t.with(title: \"{title}\")
Text.
"
        )
    };
    let r = redline_of(&src("Fast Transforms"), &src("Fast Transforms on Small Devices"));
    let what: Vec<&What> = r.changes.iter().flat_map(|c| &c.what).collect();
    let title = What::Arg {
        name: "title".into(),
        old: Some("\"Fast Transforms\"".into()),
        new: Some("\"Fast Transforms on Small Devices\"".into()),
    };
    assert_eq!(what, [&title], "{:?}", r.changes);
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
    let long = "A caption that wraps: ".repeat(8);
    let figs = [
        (
            "#figure(rect(width: 1cm), caption: [A caption much wider than the body])".to_string(),
            100.0,
        ),
        (format!("#figure(rect(width: 20%), caption: [{long}])"), 400.0),
    ];
    for (fig, least) in figs {
        let Some((marked, _)) = probes("figure-caption-frame", &doc(&fig), &doc("")) else {
            return;
        };
        let widest = marked
            .split('"')
            .filter_map(|v| v.rsplit('+').next()?.strip_suffix("pt")?.parse::<f64>().ok())
            .fold(0.0, f64::max);
        assert!(widest > least, "{fig}: {marked}");
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
fn inserted_slide_starts_on_its_own_page() {
    let slides = "#set page(width: 12cm, height: 7cm)\n#let n = counter(\"slides\")\n#let slide(body) = { context if n.get().first() > 0 { pagebreak(weak: true) }; n.step(); body }\n#slide[= One\nFirst.]\n";
    let r = redline_of(slides, &format!("{slides}#slide[= Two\nSecond.]\n"));
    let mut files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat();
    files[0].1 += "\n#import \"/.redline-preamble.typ\": redline-start-at\n#context [#metadata(redline-start-at(query(<redline-tag>).first().location(), 0).page())<probe>]\n";
    let Some(dir) = compile("inserted-slide-page", &files) else {
        return;
    };
    // On the slide's page, not at the end of the slide before.
    assert_eq!(query(&dir, "<probe>"), "[2]");
}

#[test]
fn inserted_item_of_a_tight_list_is_barred_below_the_item_before() {
    let alpha = "- Alpha #metadata(none)<before>\n";
    let r = redline_of(&format!("{alpha}- Gamma\n"), &format!("{alpha}- Beta\n- Gamma\n"));
    let mut files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat();
    files[0].1 += "\n#import \"/.redline-preamble.typ\": redline-value\n#context [#metadata({ let t = query(<redline-tag>).first(); t.location().position().y + redline-value(t).at(1).to-absolute() > query(<before>).first().location().position().y })<probe>]\n";
    let Some(dir) = compile("tight-list-bar", &files) else {
        return;
    };
    // The bar starts below the baseline of "Alpha".
    assert_eq!(query(&dir, "<probe>"), "[true]");
}

#[test]
fn inserted_footnote_is_marked_at_the_page_foot_once() {
    let r = redline_of("Text.\n", "Text#footnote[A note.].\n");
    let mut files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat();
    files[0].1 += "\n#context [#metadata(query(footnote).map(f => f.body != [A note.]))<probe>]\n";
    let Some(dir) = compile("inserted-footnote", &files) else {
        return;
    };
    assert_eq!(query(&dir, "<probe>"), "[[true]]");
}

#[test]
fn changes_are_numbered_once_each_in_reading_order() {
    let moved = "The moved paragraph has quite a few words in it so that it is found to move.";
    let old = format!("#set text(size: 11pt)\n= A\nAlpha beta gamma.\n\n{moved}\n\nDelta epsilon.\n= B\nZeta eta.\n");
    let new =
        format!("#set text(size: 10pt)\n= A\nAlpha BETA gamma.\n\nDelta epsilon new.\n= B\nZeta eta.\n\n{moved}\n");
    let r = redline_of(&old, &new);
    let changes = r
        .changes
        .iter()
        .filter(|c| c.kind != ChangeKind::Moved && !c.joined)
        .count()
        + r.moves.len();
    let mut files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat();
    files[0].1 += "\n#import \"/.redline-preamble.typ\": redline-value\n#context [#metadata(redline-value(query(<redline-numbers>).first()))<probe>]\n";
    let Some(dir) = compile("numbered", &files) else {
        return;
    };
    let nums = query(&dir, "<probe>");
    let mut values: Vec<u32> = nums
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect::<Vec<_>>()
        .chunks(2)
        .map(|kv| kv[1])
        .collect();
    values.sort();
    values.dedup();
    assert_eq!(values, (1..=changes as u32).collect::<Vec<_>>(), "{nums}");
    // The move comes last in the new document.
    assert!(nums.contains(&format!("\"m0\":{changes}")), "{nums}");
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
    // Typst 0.11 keeps the document's header on the summary pages: the page
    // set rule after the context that draws the bars does not take effect.
    let version = Command::new("typst").arg("--version").output().map(|o| o.stdout);
    if version.is_ok_and(|v| String::from_utf8_lossy(&v).contains(" 0.11.")) {
        return;
    }
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
fn summary_takes_one_page_after_small_pages_and_more_when_long() {
    // Typst 0.11 lays the summary out on the document's pages.
    let version = Command::new("typst").arg("--version").output().map(|o| o.stdout);
    if version.is_ok_and(|v| String::from_utf8_lossy(&v).contains(" 0.11.")) {
        return;
    }
    let Some(dir) = compile("page-parity", &run("page-parity").files) else {
        return;
    };
    assert_eq!(pdf_pages(&dir.join("main.pdf")), 3);
    let doc = |w: &str| {
        let body: String = (1..=80).map(|i| format!("= Part {i}\nThe {w} text.\n")).collect();
        format!("#set page(width: 8cm, height: 6cm)\n{body}")
    };
    let r = redline_of(&doc("old"), &doc("new"));
    let files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat();
    let marked = pdf_pages(&compile("long-summary", &files).unwrap().join("main.pdf"));
    let new = pdf_pages(
        &compile_to("long-summary-new", &[("main.typ".into(), doc("new"))], "main.pdf")
            .unwrap()
            .join("main.pdf"),
    );
    assert!(marked > new + 1, "{marked} pages, {new} without the summary");
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
fn deleted_citation_group_shows_each_citation() {
    // Citations cannot be queried before Typst 0.14, so deleted ones show
    // their keys there.
    if !typst_available() || typst_before_0_14() {
        return;
    }
    let dir = compile("deleted-citation-group", &run("deleted-citation-group").files).unwrap();
    let keys = query_field(&dir, "<redline-deleted-cite>", "key");
    assert!(keys.contains("<a>") && keys.contains("<b>"), "{keys}");
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
fn bibliography_entries_are_named_by_authors_and_year() {
    let r = run("bib-entries");
    assert!(
        r.body
            .contains(r#"names: ("c": "Cc 2003","a": "Aa 2011","b": "Bb 2002",)"#),
        "{}",
        r.body
    );
}

#[test]
fn bibliography_file_is_not_listed_when_its_changed_entries_are() {
    let read = |side: &'static str| move |p: &str| read_fixture("bib-entries", side, p).map(String::into_bytes);
    let (read_old, read_new) = (read("old"), read("new"));
    let v = |read| Version {
        label: "v".into(),
        entry: "main.typ".into(),
        read,
    };
    let reads = ["refs.bib".to_string()];
    let r = redline(
        &v(&read_old),
        &v(&read_new),
        FULL,
        &History::default(),
        Moves::Find,
        Some(&reads),
    );
    assert_eq!(r.bib.changed, ["a"]);
    assert!(r.bib.files.is_empty(), "{:?}", r.bib.files);
}

#[test]
fn replaced_word_counts_as_one_change() {
    let r = redline_of("Some old text.\n", "Some new text.\n");
    let main = &r.files.iter().find(|(p, _)| p == "main.typ").unwrap().1;
    assert!(main.contains("count: 1,"), "{main}");
}

#[test]
fn paragraphs_replaced_at_one_place_count_as_one_change() {
    let out = body("interleaved");
    assert!(out.contains("count: 1,"), "{out}");
}

#[test]
fn move_counts_once_where_it_arrives() {
    let out = body("moved-section");
    let left = out.lines().find(|l| l.contains("title: [Forest]")).unwrap();
    assert!(left.contains("count: 0,") && !left.contains("moves:"), "{out}");
}

#[test]
fn page_names_give_the_printed_number_and_the_pdf_page_where_they_differ() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("page-names");
    std::fs::write(dir.join(PREAMBLE_PATH), PREAMBLE).unwrap();
    std::fs::write(
        dir.join("main.typ"),
        format!(
            "#import \"{PREAMBLE_PATH}\": redline-bars, redline-page-names, redline-plain
#show: redline-bars
Cover <a>
#set page(numbering: \"i\")
Front <b>
#set page(numbering: \"1\")
#counter(page).update(1)
Body <c>
#pagebreak()
More <d>
#context [#metadata((<a>, <b>, <c>).map(l => redline-plain(redline-page-names((locate(l),)))) + (redline-plain(redline-page-names((locate(<c>), locate(<d>)))),))<probe>]
"
        ),
    )
    .unwrap();
    assert_eq!(
        query(&dir, "<probe>"),
        r#"[["PDFpage1","p.ii(PDFpage2)","p.1(PDFpage3)","pp.1–2(PDFpages3–4)"]]"#
    );
}

#[test]
fn comparison_without_changes_says_so() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("no-changes");
    std::fs::write(dir.join("a.typ"), doc("alpha")).unwrap();
    let out = cli(&dir, &["a.typ", "a.typ"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(stderr.contains("no changes found"), "{stderr}");
    assert!(dir.join("a.redline.pdf").exists());
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
fn first_page_with_its_own_foreground_and_background_is_listed_without_bars() {
    let doc = |w: &str| format!("#page(foreground: [F], background: [B])[The {w} title.]\nThe body.\n");
    let r = redline_of(&doc("old"), &doc("new"));
    let files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat();
    let Some(dir) = compile("unbarred-first", &files) else {
        return;
    };
    assert_eq!(query_field(&dir, "<redline-unbarred>", "key"), r#"["1"]"#);
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
fn a_call_that_reads_a_changed_file_is_a_change_where_it_stands() {
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
    assert!(r.bib.files.is_empty());
    let kinds: Vec<ChangeKind> = r.changes.iter().map(|c| c.kind).collect();
    assert_eq!(kinds, [ChangeKind::Del, ChangeKind::Ins, ChangeKind::Unmarked]);
    // The old image is read from the old version.
    assert!(r.changes[0].text.contains(&format!("\"/{OLD_DIR}/fig.svg\"")));
    let data = &r.changes[2];
    assert_eq!(data.what, [What::File("data/d.json".into())]);
    assert_eq!(
        (data.old.as_str(), data.text.as_str()),
        ("#let d = json(\"/data/d.json\")", "#let d = json(\"/data/d.json\")")
    );
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
fn show_reviewed_prints_the_marked_commit_or_nothing() {
    let dir = repo("show-reviewed", &["alpha"]);
    std::fs::create_dir(dir.join("my döc")).unwrap();
    commit(&dir, "my döc/nöte.typ", "beta");
    let show = |dir: &Path| {
        let out = cli(dir, &["my döc/nöte.typ", "--show-reviewed"]);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(show(&dir), "");
    assert!(cli(&dir, &["my döc/nöte.typ", "--mark-reviewed"]).status.success());
    commit(&dir, "my döc/nöte.typ", "gamma");
    assert_eq!(
        show(&dir),
        format!(
            "{}
",
            git(&dir, &["rev-parse", "c-beta"])
        )
    );
    git(&dir, &["branch", "reviewed", "c-alpha"]);
    assert_eq!(
        show(&dir),
        format!(
            "{}
",
            git(&dir, &["rev-parse", "c-alpha"])
        )
    );
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
fn replaced_image_shows_old_and_new_at_its_figure() {
    if !typst_available() {
        return;
    }
    let dir = repo("replaced-image", &["alpha"]);
    let svg = |w: u32| format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="10"/>"#);
    let main = "#show image: it => context [#metadata(measure(it).width)<probe>#it]\n#figure(image(\"fig.svg\"))\n";
    std::fs::write(dir.join("doc/fig.svg"), svg(10)).unwrap();
    std::fs::write(dir.join("doc/main.typ"), main).unwrap();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "figure"]);
    std::fs::write(dir.join("doc/fig.svg"), svg(20)).unwrap();
    let out = cli(&dir.join("doc"), &["main.typ", "--keep-typ"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    let kept = Path::new(stderr.lines().find_map(|l| l.strip_prefix("kept ")).unwrap());
    let widths = query(kept, "<probe>");
    std::fs::remove_dir_all(kept).unwrap();
    // The old image, then the new one, twice as wide.
    let pts: Vec<f64> = widths
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .filter_map(|w| w.parse().ok())
        .collect();
    assert!(pts.len() == 2 && pts[1] == 2.0 * pts[0], "{widths}");
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
        (&["old.typ", "new.typ", "-o", "new.typ"][..], "not a .pdf"),
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
fn output_other_than_a_pdf_is_a_usage_error() {
    let dir = temp_dir("output-png");
    let out = cli(&dir, &["old.typ", "new.typ", "-o", "out.png"]);
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
    let c = |ins, id, page| Placed {
        kind: if ins { PlacedKind::Ins } else { PlacedKind::Del },
        id,
        mark: id as usize,
        page,
        last: page,
    };
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
    let (pairs, unpaired) = pair_pages(&old, &new, (&[], &[]), (&[], &[]));
    let pair = |old: &[usize], new: &[usize], steps_old: &[usize], steps_new: &[usize]| Pair {
        old: old.to_vec(),
        new: new.to_vec(),
        steps_old: steps_old.to_vec(),
        steps_new: steps_new.to_vec(),
        moved: false,
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
    let c = |ins, id, page| Placed {
        kind: if ins { PlacedKind::Ins } else { PlacedKind::Del },
        id,
        mark: id as usize,
        page,
        last: page,
    };
    let old = [c(true, 5, 1), c(true, 6, 2), c(true, 7, 3), c(false, 1, 4)];
    let new = [
        c(true, 8, 1),
        c(true, 9, 2),
        c(true, 7, 3),
        c(false, 2, 4),
        c(false, 3, 5),
    ];
    let (pairs, unpaired) = pair_pages(&old, &new, (&[], &[]), (&[], &[]));
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
    let c = |ins, id, page| Placed {
        kind: if ins { PlacedKind::Ins } else { PlacedKind::Del },
        id,
        mark: id as usize,
        page,
        last: page,
    };
    let pages = |ins, anchors| {
        let (pairs, _) = pair_pages(&[c(ins, 7, 3)], &[c(ins, 7, 3)], anchors, (&[], &[]));
        pairs.iter().map(|p| (p.old.clone(), p.new.clone())).collect::<Vec<_>>()
    };
    // Page 3 of the new version is inserted; the old version marks its
    // place at the top of page 3, whose heading is on page 4 of the new.
    assert_eq!(pages(true, (&[(1, 3)], &[(1, 4)])), [(vec![], vec![3])]);
    // And the other way round for a deleted page.
    assert_eq!(pages(false, (&[(1, 4)], &[(1, 3)])), [(vec![3], vec![])]);
}

#[test]
fn side_by_side_does_not_take_slides_with_the_same_changed_header_for_steps() {
    let c = |ins, page| Placed {
        kind: if ins { PlacedKind::Ins } else { PlacedKind::Del },
        id: 1,
        mark: 1,
        page,
        last: page,
    };
    // Each of three slides shows the same changed header.
    let old: Vec<_> = (1..4).map(|p| c(false, p)).collect();
    let new: Vec<_> = (1..4).map(|p| c(true, p)).collect();
    let anchors = [(1, 1), (2, 2), (3, 3)];
    let (pairs, _) = pair_pages(&old, &new, (&anchors, &anchors), (&[], &[]));
    assert_eq!(pairs.len(), 3);
    assert!(pairs.iter().all(|p| p.steps_old.is_empty() && p.steps_new.is_empty()));
}

#[test]
fn side_by_side_pairs_pages_drawn_differently_by_their_headings() {
    // A listed change adds a step to a slide built in a loop: old pages
    // 4 and 5 (the last step and the end) and new pages 4 to 6 look different.
    let (build, end) = (1, 2);
    let anchors_old = [(build, 2), (build, 3), (build, 4), (end, 5)];
    let anchors_new = [(build, 2), (build, 3), (build, 4), (build, 5), (end, 6)];
    let looks_old = [1, 2, 3, 4, 5];
    let looks_new = [1, 2, 3, 14, 15, 16];
    let (pairs, _) = pair_pages(&[], &[], (&anchors_old, &anchors_new), (&looks_old, &looks_new));
    let pages: Vec<_> = pairs.iter().map(|p| (p.old.clone(), p.new.clone())).collect();
    assert_eq!(pages, [(vec![4], vec![4, 5]), (vec![5], vec![6])]);
}

#[test]
fn side_by_side_pairs_slides_by_their_headings_where_the_versions_mark_them_differently() {
    let c = |ins, id, page| Placed {
        kind: if ins { PlacedKind::Ins } else { PlacedKind::Del },
        id,
        mark: id as usize,
        page,
        last: page,
    };
    // Old slides: title, "First", "Changes" in two steps, "Box". New slides:
    // title, "Changes" in two steps, "Box", "New". Each version's own
    // comparison takes the slides in between as replaced by each other.
    let old = [
        c(true, 1, 1),
        c(false, 2, 1),
        c(true, 10, 1),
        c(true, 11, 1),
        c(true, 12, 2),
        c(false, 13, 2),
        c(true, 14, 2),
        c(false, 15, 2),
        c(false, 16, 3),
        c(false, 17, 5),
    ];
    let new = [
        c(false, 2, 1),
        c(true, 1, 1),
        c(false, 20, 1),
        c(false, 21, 2),
        c(true, 22, 2),
        c(false, 21, 3),
        c(true, 22, 3),
        c(false, 23, 4),
        c(true, 24, 4),
        c(true, 25, 5),
    ];
    let (title, first, changes, boxed, added) = (1, 2, 3, 4, 5);
    let anchors_old = [(title, 1), (first, 2), (changes, 3), (changes, 4), (boxed, 5)];
    let anchors_new = [(title, 1), (changes, 2), (changes, 3), (boxed, 4), (added, 5)];
    let (pairs, _) = pair_pages(&old, &new, (&anchors_old, &anchors_new), (&[], &[]));
    let pages: Vec<_> = pairs.iter().map(|p| (p.old.clone(), p.new.clone())).collect();
    assert_eq!(
        pages,
        [
            (vec![1], vec![1]),
            (vec![3], vec![2]),
            (vec![4], vec![3]),
            (vec![5], vec![4]),
            (vec![2], vec![5])
        ]
    );
}

#[test]
fn side_by_side_shows_a_page_inserted_after_a_moved_one_without_a_counterpart() {
    let c = |kind, id, page| Placed {
        kind,
        id,
        mark: id as usize,
        page,
        last: page,
    };
    let (mv, ins) = (PlacedKind::Move, PlacedKind::Ins);
    // The old version marks the place of new page 4 on page 3, whose moved
    // passage is on page 2 of the new version.
    let old = [c(mv, 5, 3), c(mv, 5, 3), c(ins, 7, 3)];
    let new = [c(mv, 5, 2), c(mv, 5, 2), c(ins, 7, 4)];
    let (pairs, _) = pair_pages(&old, &new, (&[], &[]), (&[], &[]));
    let pages: Vec<_> = pairs.iter().map(|p| (p.old.clone(), p.new.clone(), p.moved)).collect();
    assert_eq!(pages, [(vec![3], vec![2], true), (vec![], vec![4], false)]);
}

/// Whether the typst on PATH is older than the side-by-side layout needs.
fn typst_before_0_14() -> bool {
    ["0.11.", "0.12.", "0.13."]
        .iter()
        .any(|v| typst_version().starts_with(v))
}

/// The version of the typst on PATH, such as "0.14.2".
fn typst_version() -> String {
    let out = Command::new("typst").arg("--version").output().unwrap().stdout;
    let out = String::from_utf8_lossy(&out);
    out.split_whitespace().nth(1).unwrap_or_default().to_string()
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
    let doc = |word: &str| format!("One.\n#pagebreak()\nThe {word} two.\n#pagebreak()\nThree.\n");
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
    // The overview, the one changed pair and the summary.
    assert_eq!(pdf_pages(&dir.join("out.pdf")), 3);
}

/// The pairs of pages that a side-by-side run in `dir` shows, as the
/// composite source lists them.
fn shown_pairs(dir: &Path) -> Vec<String> {
    let src = marked(
        dir,
        &["old.typ", "new.typ", "-o", "out.pdf", "--layout", "side-by-side"],
        ".redline-side-by-side.typ",
    );
    src.lines()
        .filter_map(|l| Some(l.trim().strip_prefix("(old: ")?.split(", steps").next()?.to_string()))
        .collect()
}

#[test]
fn side_by_side_shows_the_pages_of_a_changed_page_call() {
    if !typst_available() || typst_before_0_14() {
        return;
    }
    let dir = temp_dir("side-by-side-page-call");
    for side in ["old", "new"] {
        let src = read_fixture("changed-page-call", side, "main.typ").unwrap();
        std::fs::write(dir.join(format!("{side}.typ")), src).unwrap();
    }
    assert_eq!(shown_pairs(&dir), ["(2,), new: (2,)"]);
}

#[test]
fn side_by_side_shows_every_page_of_a_long_insertion() {
    if !typst_available() || typst_before_0_14() {
        return;
    }
    let dir = temp_dir("side-by-side-long");
    let long = (1..25)
        .map(|i| format!("Inserted sentence number {i} keeps running. "))
        .collect::<String>();
    let doc = |middle: &str| format!("#set page(width: 10cm, height: 6cm)\nIntro.\n\n{middle}\n\nEnd.\n");
    std::fs::write(dir.join("old.typ"), doc("")).unwrap();
    std::fs::write(dir.join("new.typ"), doc(&long)).unwrap();
    assert_eq!(shown_pairs(&dir), ["(1,), new: (1,2,3,)"]);
}

#[test]
fn summary_pages_do_not_count_towards_the_number_of_pages() {
    let doc = |w: &str| {
        format!("The {w} word.\n#pagebreak()\nEnd.\n#context [#metadata(counter(page).final().first())<probe>]\n")
    };
    let r = redline_of(&doc("old"), &doc("new"));
    let files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat();
    let Some(dir) = compile("summary-page-count", &files) else {
        return;
    };
    assert_eq!(query(&dir, "<probe>"), "[2]");
}

#[test]
fn side_by_side_shows_no_page_for_an_empty_file() {
    if !typst_available() || typst_before_0_14() {
        return;
    }
    let dir = temp_dir("side-by-side-empty-new");
    std::fs::write(dir.join("old.typ"), "= Doc\nLine one.\n").unwrap();
    std::fs::write(dir.join("new.typ"), "").unwrap();
    assert_eq!(shown_pairs(&dir), ["(1,), new: ()"]);
}

#[test]
fn side_by_side_shows_a_document_whose_layout_does_not_settle() {
    if !typst_available() || typst_before_0_14() {
        return;
    }
    let dir = temp_dir("side-by-side-unsettled");
    let doc = |word: &str| {
        format!(
            "#context for i in range(calc.rem(counter(page).final().first(), 3) + 1) {{ pagebreak() }}\nThe {word} word.\n"
        )
    };
    std::fs::write(dir.join("old.typ"), doc("old")).unwrap();
    std::fs::write(dir.join("new.typ"), doc("new")).unwrap();
    let out = cli(
        &dir,
        &["old.typ", "new.typ", "-o", "out.pdf", "--layout", "side-by-side"],
    );
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn side_by_side_takes_a_heading_without_text() {
    if !typst_available() || typst_before_0_14() {
        return;
    }
    let dir = temp_dir("side-by-side-empty-heading");
    std::fs::write(dir.join("old.typ"), "#heading[#[]]\nThe old word.\n").unwrap();
    std::fs::write(dir.join("new.typ"), "#heading[#[]]\nThe new word.\n").unwrap();
    let out = cli(
        &dir,
        &["old.typ", "new.typ", "-o", "out.pdf", "--layout", "side-by-side"],
    );
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn side_by_side_shows_two_pages_with_the_same_edit() {
    if !typst_available() || typst_before_0_14() {
        return;
    }
    let dir = temp_dir("side-by-side-same-edit");
    let doc = |word: &str| format!("The {word} one.\n#pagebreak()\nThe {word} two.\n");
    std::fs::write(dir.join("old.typ"), doc("old")).unwrap();
    std::fs::write(dir.join("new.typ"), doc("new")).unwrap();
    let out = cli(
        &dir,
        &["old.typ", "new.typ", "-o", "out.pdf", "--layout", "side-by-side"],
    );
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // The overview, both changed pairs and the summary.
    assert_eq!(pdf_pages(&dir.join("out.pdf")), 4);
}

#[test]
fn side_by_side_shows_pages_changed_by_a_listed_change() {
    if !typst_available() || typst_before_0_14() {
        return;
    }
    let dir = temp_dir("side-by-side-listed");
    let doc = |n: &str| format!("#set heading(numbering: \"{n}\")\n= One\n#pagebreak()\nTwo.\n");
    std::fs::write(dir.join("old.typ"), doc("I.")).unwrap();
    std::fs::write(dir.join("new.typ"), doc("1.")).unwrap();
    let out = cli(
        &dir,
        &["old.typ", "new.typ", "-o", "out.pdf", "--layout", "side-by-side"],
    );
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // The overview, the page with the renumbered heading and the summary.
    assert_eq!(pdf_pages(&dir.join("out.pdf")), 3);
}

#[test]
fn side_by_side_shows_pages_changed_by_a_listed_change_next_to_a_marked_edit() {
    if !typst_available() || typst_before_0_14() {
        return;
    }
    let dir = temp_dir("side-by-side-listed-and-marked");
    let doc = |to: &str, word: &str| {
        format!(
            "#show \"colour\": \"{to}\"
The {word} word.
#pagebreak()
A colour.
#pagebreak()
The end.
"
        )
    };
    std::fs::write(dir.join("old.typ"), doc("color", "old")).unwrap();
    std::fs::write(dir.join("new.typ"), doc("hue", "new")).unwrap();
    assert_eq!(shown_pairs(&dir), ["(1,), new: (1,)", "(2,), new: (2,)"]);
}

#[test]
fn side_by_side_summary_finds_the_section_of_a_deletion() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("side-deletion-section");
    std::fs::write(dir.join(PREAMBLE_PATH), PREAMBLE).unwrap();
    let main = format!(
        "#import \"/{PREAMBLE_PATH}\": redline-del, redline-under-headings
= Intro
A.
= Build
Text #redline-del(id: 0)[gone] more.
#context [#metadata(redline-under-headings(((id: 0),)).map(g => g.heading.body.text))<probe>]
"
    );
    std::fs::write(dir.join("main.typ"), main).unwrap();
    let out = Command::new("typst")
        .args(["query", "--input", "redline-side=new", "--root"])
        .arg(&*dir)
        .arg(dir.join("main.typ"))
        .args(["<probe>", "--field", "value"])
        .output()
        .unwrap();
    let found: String = String::from_utf8_lossy(&out.stdout).split_whitespace().collect();
    assert_eq!(found, "[[\"Build\"]]", "{}", String::from_utf8_lossy(&out.stderr));
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
    // The overview, the pages that show the move and the summary.
    assert_eq!(pages, 3);
}

#[test]
fn side_by_side_shows_a_moved_slide_next_to_its_old_page() {
    if !typst_available() || typst_before_0_14() {
        return;
    }
    let dir = temp_dir("side-by-side-moved-slide");
    let (old, new) = reordered_slides();
    std::fs::write(dir.join("old.typ"), old).unwrap();
    std::fs::write(dir.join("new.typ"), new).unwrap();
    let out = cli(
        &dir,
        &["old.typ", "new.typ", "-o", "out.pdf", "--layout", "side-by-side"],
    );
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // The overview, old page 2 next to new page 4, and the summary.
    assert_eq!(pdf_pages(&dir.join("out.pdf")), 3);
}

#[test]
fn side_by_side_pairs_changes_around_a_move_in_either_order() {
    let c = |kind, id, page| Placed {
        kind,
        id,
        mark: id as usize,
        page,
        last: page,
    };
    use PlacedKind::{Ins, Move};
    // Text moved from page 2 to page 4 with an insertion inside it, and an
    // insertion on page 3 of the new version that the old version has after
    // the move.
    let old = [c(Move, 9, 2), c(Ins, 5, 2), c(Move, 9, 2), c(Ins, 1, 3)];
    let new = [c(Ins, 2, 3), c(Move, 9, 4), c(Ins, 5, 4), c(Move, 9, 4)];
    let (pairs, unpaired) = pair_pages(&old, &new, (&[], &[]), (&[], &[]));
    let pages: Vec<_> = pairs.iter().map(|p| (p.old.clone(), p.new.clone(), p.moved)).collect();
    assert_eq!(pages, [(vec![3], vec![3], false), (vec![2], vec![4], true)]);
    assert!(unpaired.is_empty());
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
fn a_missing_old_file_is_an_error() {
    let dir = temp_dir("missing-old");
    std::fs::write(dir.join("new.typ"), "New.\n").unwrap();
    let out = cli(&dir, &["old.typ", "new.typ"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.code() == Some(1) && stderr.contains("does not exist"),
        "{stderr}"
    );
    assert!(!dir.join("new.redline.pdf").exists());
}

#[test]
fn errors_of_the_document_itself_name_its_own_lines() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("broken-new");
    std::fs::write(dir.join("old.typ"), "Text.\n").unwrap();
    std::fs::write(dir.join("new.typ"), "Text.\n\nMore #undefined() here.\n").unwrap();
    let out = cli(&dir, &["old.typ", "new.typ"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.code() == Some(1) && stderr.contains("new.typ:3:") && stderr.contains("does not compile on its own"),
        "{stderr}"
    );
}

#[test]
fn deleted_content_of_a_file_in_a_moved_folder_finds_its_files() {
    if !typst_available() {
        return;
    }
    let dir = temp_dir("moved-folder");
    let svg =
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"10\"><rect width=\"20\" height=\"10\"/></svg>";
    for (path, text) in [
        ("v1/m.typ", "#include \"ch/c.typ\"\n"),
        ("v1/ch/c.typ", "= C\nSome text.\n#image(\"../img/a.svg\", width: 1cm)\n"),
        ("v1/img/a.svg", svg),
        ("v2/m.typ", "#include \"../shared/ch/c.typ\"\n"),
        (
            "shared/ch/c.typ",
            "= C\nSome new text.\n#image(\"../img/b.svg\", width: 1cm)\n",
        ),
        ("shared/img/b.svg", svg),
    ] {
        std::fs::create_dir_all(dir.join(path).parent().unwrap()).unwrap();
        std::fs::write(dir.join(path), text).unwrap();
    }
    let out = cli(&dir, &["v1/m.typ", "v2/m.typ", "--root", "."]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && !stderr.contains("note:"), "{stderr}");
}

#[test]
fn a_path_split_at_a_space_gets_a_short_usage_error() {
    let dir = temp_dir("split-path");
    let out = cli(&dir, &["My", "Thesis/old.typ", "My", "Thesis/new.typ"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.code() == Some(2) && stderr.contains("quote") && stderr.lines().count() <= 3,
        "{stderr}"
    );
}

#[test]
fn a_root_above_the_repository_is_named_with_it() {
    let dir = temp_dir("root-above-repo");
    let proj = dir.join("proj");
    std::fs::create_dir(&proj).unwrap();
    git(&proj, &["init", "-q"]);
    std::fs::write(proj.join("main.typ"), "Text.\n").unwrap();
    let out = cli(&dir, &["proj/main.typ", "--root", "."]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.code() == Some(1) && stderr.contains("outside the git repository"),
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
fn commits_that_only_change_data_the_document_reads_are_listed() {
    if !typst_available() {
        return;
    }
    let dir = repo("data-commit", &["alpha"]);
    let main = format!("#let d = json(\"d.json\")\n{}Value: #d.v\n", doc("alpha"));
    std::fs::write(dir.join("doc/main.typ"), main).unwrap();
    std::fs::write(dir.join("doc/d.json"), "{\"v\": 1}").unwrap();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "data"]);
    git(&dir, &["tag", "c-data"]);
    std::fs::write(dir.join("doc/d.json"), "{\"v\": 2}").unwrap();
    git(&dir, &["commit", "-q", "-am", "value"]);
    let src = marked(&dir, &["doc/main.typ", "--since", "c-data"], "main.typ");
    assert!(src.contains("subject: \"value\""), "{src}");
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

#[test]
fn deletion_in_a_document_with_numbered_lines_is_marked_in_full() {
    if !typst_available() || typst_before_0_14() {
        return;
    }
    let dir = temp_dir("numbered-lines");
    std::fs::write(dir.join("old.typ"), "#set par.line(numbering: \"1\")\nSome old text.\n").unwrap();
    std::fs::write(dir.join("new.typ"), "#set par.line(numbering: \"1\")\nSome new text.\n").unwrap();
    let out = cli(&dir, &["old.typ", "new.typ", "-o", "out.pdf"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && !stderr.contains("note:"), "{stderr}");
}

/// The marked pages of `old` against `new` as glyphs, with `preamble`.
fn proof_glyphs(name: &str, old: &str, new: &str) -> Option<Vec<Vec<Glyph>>> {
    let r = redline_of(old, new);
    glyphs(name, &[r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.into())]].concat())
}

#[test]
fn change_numbers_go_to_the_margin_that_has_room() {
    // A header, a left column next to a margin too narrow for a number and
    // a middle column: each change's number stands in the right margin.
    let page =
        "#set page(width: 14cm, height: 8cm, margin: (left: 0.3cm, rest: 1.5cm), columns: 3, header: [Head WORD])\n";
    let text = |w: &str| {
        format!(
            "{page}Left {w} column.\n#colbreak()\nMiddle column.\n\nIts {w} second paragraph.\n#colbreak()\nRight column.\n"
        )
    };
    let Some(pages) = proof_glyphs(
        "margin-numbers",
        &text("old").replace("WORD", "old"),
        &text("new").replace("WORD", "new"),
    ) else {
        return;
    };
    let cm = 72.0 / 2.54;
    let mut rows: Vec<i64> = pages[0]
        .iter()
        .filter(|g| g.x > 12.5 * cm)
        .map(|g| g.y.round() as i64)
        .collect();
    rows.sort();
    rows.dedup();
    assert_eq!(rows.len(), 3, "{rows:?}");
}

/// The summary's rows of changes by section, as (name, changes, pages).
fn section_rows(name: &str, old: &str, new: &str) -> Option<String> {
    let r = redline_of(old, new);
    let probe = "\n#let redline-summary(info) = context [#metadata(redline-section-rows(info, info.entries.map(e => ()), redline-numbering(info.entries)).map(r => (redline-plain(r.name).trim(), redline-plain(r.what), r.pages)))<probe>]\n";
    let files = [r.files, vec![(PREAMBLE_PATH.into(), PREAMBLE.to_string() + probe)]].concat();
    Some(query(&compile(name, &files)?, "<probe>"))
}

#[test]
fn summary_rows_are_the_headings_laid_out_with_the_pages_of_their_changes() {
    // The header and footer stay with the code that sets them, a heading
    // written as a call is a section, and a section's pages are those of
    // its changes.
    let doc = |w: &str| {
        format!(
            "#set page(height: 8cm, header: [Head {w}], footer: [Foot {w}])\n= Alpha\nOne {w}.\n#pagebreak()\nTwo {w}.\n#heading[Beta]\nThree {w}.\n"
        )
    };
    if let Some(rows) = section_rows("section-rows", &doc("old"), &doc("new")) {
        assert_eq!(
            rows,
            r#"[[["Startofthedocument","2changes",[1]],["Alpha","2changes",[1,2]],["Beta","1change",[2]]]]"#
        );
    }
}

#[test]
fn picture_inside_a_mark_of_text_is_framed() {
    let files = [
        (
            "a.svg".into(),
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"10\"/>".into(),
        ),
        (PREAMBLE_PATH.into(), PREAMBLE.into()),
        (
            "main.typ".into(),
            format!(
                "#import \"/{PREAMBLE_PATH}\": redline-framed
#metadata(redline-framed(image(\"a.svg\"), false))<probe>
"
            ),
        ),
    ];
    if let Some(dir) = compile("picture-in-text", &files) {
        assert_eq!(query(&dir, "<probe>"), "[true]");
    }
}

#[test]
fn set_rule_of_a_show_rule_is_listed_with_it() {
    let r = redline_of(
        "#show emph: set text(fill: red)\n_a_\n",
        "#show emph: set text(fill: blue)\n_a_\n",
    );
    let texts: Vec<&str> = r.changes.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, ["show emph: set text(fill: blue)"]);
}
