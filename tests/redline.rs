use std::path::Path;
use std::process::Command;

use typst_redline::{PREAMBLE, redline};

const FIXTURES: [&str; 5] = ["inline-math", "display-label", "deleted-label", "crlf", "sections"];

fn fixture(name: &str) -> (String, String) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap();
    (read("old.typ"), read("new.typ"))
}

fn body(name: &str) -> String {
    let (old, new) = fixture(name);
    redline(&old, &new, "")
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
    let out = body("crlf");
    assert!(!out.contains("#redline-ins") && !out.contains("#redline-del"), "{out}");
}

#[test]
fn output_compiles() {
    if Command::new("typst").arg("--version").output().is_err() {
        eprintln!("typst not found, skipping");
        return;
    }
    let dir = std::env::temp_dir().join(format!("typst-redline-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in FIXTURES {
        let (old, new) = fixture(name);
        let src = dir.join(format!("{name}.typ"));
        std::fs::write(&src, redline(&old, &new, PREAMBLE)).unwrap();
        let out = Command::new("typst")
            .arg("compile")
            .arg(&src)
            .arg(dir.join(format!("{name}.pdf")))
            .output()
            .unwrap();
        assert!(out.status.success(), "{name}: {}", String::from_utf8_lossy(&out.stderr));
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
