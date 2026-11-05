//! `pw` run as a program, on documents in temporary folders.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use write_core::review::{NewRecord, Person, Review};

const TEXT: &str = "The grounding line retreats rapidly when the bed slopes inland.\nWe show this in Fig. 3.\n";

/// A fresh git folder (the user is "Alex Tester" there) with `paper.md` in it.
fn folder() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("pw-cli-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for args in [&["init", "-q"][..], &["config", "user.name", "Alex Tester"], &["config", "user.email", "alex@example.org"]] {
        assert!(Command::new("git").arg("-C").arg(&dir).args(args).status().unwrap().success());
    }
    std::fs::write(dir.join("paper.md"), TEXT).unwrap();
    dir
}

fn pw(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pw")).args(args).output().unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn range(needle: &str) -> (usize, usize) {
    let from = TEXT.find(needle).unwrap();
    (from, from + needle.len())
}

/// Jane's suggestion (`old` replaced by `new`) on the document; its id.
fn suggest(doc: &Path, who: &str, old: &str, new: &str) -> String {
    let (from, to) = range(old);
    Review::add(doc, &Person::new(who, ""), TEXT, NewRecord::suggestion(from, to, new)).unwrap().id
}

fn comment(doc: &Path, who: &str, on: &str, body: &str) -> String {
    let (from, to) = range(on);
    Review::add(doc, &Person::new(who, ""), TEXT, NewRecord::comment(from, to, body)).unwrap().id
}

fn path(doc: &Path) -> &str {
    doc.to_str().unwrap()
}

#[test]
fn list_says_who_where_and_what() {
    let dir = folder();
    let doc = dir.join("paper.md");
    suggest(&doc, "Jane Doe", "rapidly", "slowly");
    comment(&doc, "Bob Roe", "Fig. 3", "Which one?");
    let o = pw(&["review", "list", path(&doc)]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(text.contains("jane-doe-1  1:29  Jane Doe  \"rapidly\" → \"slowly\""), "{text}");
    assert!(text.contains("bob-roe-1  2:17  Bob Roe  comment: Which one?"), "{text}");

    let o = pw(&["review", "list", path(&doc), "--json"]);
    let rows: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["id"], "jane-doe-1");
    assert_eq!(rows[0]["old"], "rapidly");
    assert_eq!(rows[0]["new"], "slowly");
    assert_eq!(rows[0]["line"], 1);
    assert_eq!(rows[1]["kind"], "comment");
    assert_eq!(rows[1]["text"], "Which one?");
}

#[test]
fn an_empty_review_lists_nothing() {
    let dir = folder();
    let doc = dir.join("paper.md");
    assert!(out(&pw(&["review", "list", path(&doc)])).contains("no open"));
    assert_eq!(out(&pw(&["review", "list", path(&doc), "--json"])).trim(), "[]");
}

#[test]
fn accept_by_id_applies_the_file_and_records_the_decision_under_the_user() {
    let dir = folder();
    let doc = dir.join("paper.md");
    let a = suggest(&doc, "Jane Doe", "rapidly", "slowly");
    let b = suggest(&doc, "Jane Doe", "inland", "seaward");
    let o = pw(&["review", "accept", path(&doc), "--id", &a]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("accepted 1 as Alex Tester"), "{}", out(&o));
    let now = std::fs::read_to_string(&doc).unwrap();
    assert_eq!(now, TEXT.replace("rapidly", "slowly"));
    // the decision is in the user's own file; the other suggestion is still open, carried onto the new text
    assert!(dir.join(".papers-write/review/paper.md/alex-tester.toml").is_file());
    let list = out(&pw(&["review", "list", path(&doc)]));
    assert!(!list.contains(&a), "{list}");
    assert!(list.contains(&b), "{list}");
}

#[test]
fn accept_all_goes_from_the_end_and_skips_what_overlaps_and_what_is_outdated() {
    let dir = folder();
    let doc = dir.join("paper.md");
    let a = suggest(&doc, "Jane Doe", "rapidly", "slowly");
    let b = suggest(&doc, "Bob Roe", "rapidly when", "quickly if");
    let c = suggest(&doc, "Jane Doe", "inland", "seaward");
    let o = pw(&["review", "accept", path(&doc), "--all"]);
    assert!(o.status.success(), "{}", err(&o));
    let said = out(&o);
    assert!(said.contains(&format!("skipped {b}: overlaps {a}")), "{said}");
    assert_eq!(std::fs::read_to_string(&doc).unwrap(), TEXT.replace("rapidly", "slowly").replace("inland", "seaward"));
    assert!(said.contains(&a) && said.contains(&c), "{said}");
    // what was skipped stays open
    assert!(out(&pw(&["review", "list", path(&doc)])).contains(&b));

    // an edit under a suggestion makes it outdated: not applied
    let dir = folder();
    let doc = dir.join("paper.md");
    let id = suggest(&doc, "Jane Doe", "rapidly", "slowly");
    std::fs::write(&doc, TEXT.replace("rapidly", "very fast")).unwrap();
    let o = pw(&["review", "accept", path(&doc), "--all"]);
    assert!(out(&o).contains(&format!("skipped {id}: outdated")), "{}", out(&o));
    assert_eq!(std::fs::read_to_string(&doc).unwrap(), TEXT.replace("rapidly", "very fast"));
}

#[test]
fn accept_by_author_takes_theirs_only() {
    let dir = folder();
    let doc = dir.join("paper.md");
    suggest(&doc, "Jane Doe", "rapidly", "slowly");
    let bob = suggest(&doc, "Bob Roe", "inland", "seaward");
    let o = pw(&["review", "accept", path(&doc), "--author", "bob roe"]);
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(std::fs::read_to_string(&doc).unwrap(), TEXT.replace("inland", "seaward"));
    assert!(out(&o).contains(&bob));
}

#[test]
fn reject_records_decisions_and_leaves_the_file() {
    let dir = folder();
    let doc = dir.join("paper.md");
    let a = suggest(&doc, "Jane Doe", "rapidly", "slowly");
    let note = comment(&doc, "Jane Doe", "Fig. 3", "Which one?");
    let o = pw(&["review", "reject", path(&doc), "--all"]);
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(std::fs::read_to_string(&doc).unwrap(), TEXT);
    let list = out(&pw(&["review", "list", path(&doc)]));
    assert!(!list.contains(&a) && list.contains(&note), "{list}");
    // an id that is a comment, or not there, is said
    let o = pw(&["review", "reject", path(&doc), "--id", &note, "nobody-9"]);
    assert!(err(&o).contains("is a comment") && err(&o).contains("nobody-9 is not in the review"), "{}", err(&o));
}

#[test]
fn a_command_line_that_asks_nothing_clear_is_refused() {
    let dir = folder();
    let doc = dir.join("paper.md");
    for args in [
        vec!["review", "accept", path(&doc)],
        vec!["review", "accept", path(&doc), "--all", "--author", "x"],
        vec!["review", "export", path(&doc)],
        vec!["review", "nothing", path(&doc)],
        vec!["--nothing"],
    ] {
        let o = pw(&args);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {}", err(&o));
    }
    assert_eq!(pw(&["review", "list", "/no/such/paper.md"]).status.code(), Some(1));
}

#[test]
fn export_to_markdown_is_critic_markup() {
    let dir = folder();
    let doc = dir.join("paper.md");
    suggest(&doc, "Jane Doe", "rapidly", "slowly");
    suggest(&doc, "Jane Doe", "inland", "");
    comment(&doc, "Bob Roe", "Fig. 3", "Which one?");
    let to = dir.join("out.md");
    let o = pw(&["review", "export", path(&doc), "--markdown", path(&to)]);
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(
        std::fs::read_to_string(&to).unwrap(),
        "The grounding line retreats {~~rapidly~>slowly~~} when the bed slopes {--inland--}.\nWe show this in Fig. 3{>>Bob Roe: Which one?<<}.\n"
    );
}

fn pandoc_there() -> bool {
    Command::new("pandoc").arg("--version").output().is_ok_and(|o| o.status.success())
        || ["/opt/homebrew/bin/pandoc", "/usr/local/bin/pandoc"].iter().any(|p| Path::new(p).is_file())
}

#[test]
fn word_goes_out_and_comes_back() {
    if !pandoc_there() {
        return;
    }
    let dir = folder();
    let doc = dir.join("paper.md");
    suggest(&doc, "Jane Doe", "rapidly", "slowly");
    comment(&doc, "Bob Roe", "Fig. 3", "Which one?");
    let docx = dir.join("out.docx");
    let o = pw(&["review", "export", path(&doc), "--word", path(&docx)]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("1 suggestions, 1 comments"), "{}", out(&o));

    // the same document elsewhere, without the review
    let other = folder();
    let doc2 = other.join("paper.md");
    let o = pw(&["review", "import", path(&doc2), path(&docx)]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("1 suggestions and 1 comments from"), "{}", out(&o));
    let list = out(&pw(&["review", "list", path(&doc2)]));
    assert!(list.contains("Jane Doe") && list.contains("\"rapidly\" → \"slowly\"") && list.contains("Which one?"), "{list}");
}

#[test]
fn a_review_sent_back_is_merged() {
    let dir = folder();
    let doc = dir.join("paper.md");
    suggest(&doc, "Jane Doe", "rapidly", "slowly");
    // sent for review, then back: the package of the sender's side
    let me = Person::new("Alex Tester", "");
    let sent = dir.join("sent.review");
    write_core::review::package::send(&doc, TEXT, &me, "Round 1", &sent).unwrap();
    // Jane's copy: unpack, suggest, send back
    let root = dir.join("unpacked");
    let opened = write_core::review::package::open(&sent).unwrap();
    let jane_doc = write_core::review::package::unpack(&opened, &root).unwrap();
    let (from, to) = range("inland");
    Review::add(&jane_doc, &Person::new("Carol Poe", ""), TEXT, NewRecord::suggestion(from, to, "seaward")).unwrap();
    let back = dir.join("back.review");
    write_core::review::package::send_back(&jane_doc, &Person::new("Carol Poe", ""), &back).unwrap();

    let other = folder();
    let doc2 = other.join("paper.md");
    let o = pw(&["review", "import", path(&doc2), path(&back)]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("Carol Poe"), "{}", out(&o));
    assert!(out(&pw(&["review", "list", path(&doc2)])).contains("Carol Poe"));
}

#[cfg(target_os = "macos")]
#[test]
fn files_are_opened_by_the_app_found_through_spotlight() {
    use std::os::unix::fs::PermissionsExt;
    let dir = folder();
    let bin = dir.join("bin");
    let app = dir.join("Papers Write.app");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&app).unwrap();
    let log = dir.join("opened.txt");
    let script = |name: &str, body: String| {
        let p = bin.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    script("mdfind", format!("echo '{}'", app.display()));
    script("open", format!("echo \"$@\" > '{}'", log.display()));
    let o = Command::new(env!("CARGO_BIN_EXE_pw")).env("PATH", &bin).args([path(&dir.join("paper.md"))]).output().unwrap();
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(std::fs::read_to_string(&log).unwrap().trim(), format!("-a {} {}", app.display(), dir.join("paper.md").display()));
    // a file that is not there
    let o = Command::new(env!("CARGO_BIN_EXE_pw")).env("PATH", &bin).arg(path(&dir.join("none.md"))).output().unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("no such file"));
}
