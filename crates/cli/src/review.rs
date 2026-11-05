//! `pw review …`: the review of a document, read and written as the app does (appending to the user's own review
//! file; the document itself written only by accept).

use std::path::{Path, PathBuf};

use serde_json::json;
use write_core::exchange;
use write_core::review::critic;
use write_core::review::package::{self, PackageKind};
use write_core::review::{Decision, Item, Kind, Person, Review, State, Status};

use crate::{Result, fail, misuse};

pub fn run(args: &[String]) -> Result {
    let Some((sub, rest)) = args.split_first() else {
        return misuse("review: list, accept, reject, export or import?");
    };
    let Some((doc, flags)) = rest.split_first().filter(|(d, _)| !d.starts_with('-')) else {
        return misuse(format!("review {sub}: which document?"));
    };
    let doc = PathBuf::from(doc);
    match sub.as_str() {
        "list" => list(&doc, flags),
        "accept" => decide(&doc, flags, Decision::Accepted),
        "reject" => decide(&doc, flags, Decision::Rejected),
        "export" => export(&doc, flags),
        "import" => import(&doc, flags),
        other => misuse(format!("review {other}: not a command")),
    }
}

fn read(doc: &Path) -> Result<String> {
    let bytes = std::fs::read(doc).map_err(|e| crate::Failure { message: format!("{}: {e}", doc.display()), usage: false })?;
    String::from_utf8(bytes).or_else(|_| fail(format!("{}: not a UTF-8 text file", doc.display())))
}

/// The items of the review of `doc` on its text, and what could not be read (said on stderr).
fn items_of(doc: &Path, text: &str) -> Result<Vec<Item>> {
    let review = Review::load(doc).or_else(|e| fail(e.to_string()))?;
    for e in &review.errors {
        eprintln!("pw: {}: {}", e.file.display(), e.message);
    }
    Ok(review.items(text))
}

/// `line:col` (1-based, columns in characters) of the byte offset `at` of `text`.
fn line_col(text: &str, at: usize) -> (usize, usize) {
    let before = &text[..at.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, col)
}

fn status_word(s: &Status) -> &'static str {
    match s {
        Status::Exact => "exact",
        Status::Moved => "moved",
        Status::Outdated { .. } => "outdated",
        Status::Orphaned => "orphaned",
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\n', "\\n"))
}

// ---- list

fn list(doc: &Path, flags: &[String]) -> Result {
    let mut json_out = false;
    for f in flags {
        match f.as_str() {
            "--json" => json_out = true,
            other => return misuse(format!("review list: unknown option {other}")),
        }
    }
    let text = read(doc)?;
    let items = items_of(doc, &text)?;
    let open: Vec<&Item> = items.iter().filter(|i| i.state == State::Open).collect();
    if json_out {
        let rows: Vec<_> = open
            .iter()
            .map(|i| {
                let (line, col) = line_col(&text, i.range.start);
                let old = &text[i.range.clone()];
                json!({
                    "id": i.id,
                    "kind": i.kind,
                    "author": i.author,
                    "state": i.state,
                    "status": status_word(&i.status),
                    "line": line,
                    "column": col,
                    "old": (i.kind == Kind::Suggestion).then_some(old),
                    "new": i.replacement,
                    "text": i.record.text,
                    "replies": i.thread.len(),
                    "on": i.on,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows).unwrap_or_default());
        return Ok(());
    }
    if open.is_empty() {
        println!("no open suggestions or comments");
    }
    for i in open {
        let (line, col) = line_col(&text, i.range.start);
        let what = match i.kind {
            Kind::Suggestion => {
                let note = i.record.text.as_deref().map(str::trim).filter(|t| !t.is_empty());
                format!("{} → {}{}", quote(&text[i.range.clone()]), quote(i.replacement.as_deref().unwrap_or("")), note.map_or(String::new(), |n| format!("  ({n})")))
            }
            _ => format!("comment: {}", i.record.text.as_deref().unwrap_or("").trim().replace('\n', " ")),
        };
        let status = match i.status {
            Status::Exact | Status::Moved => String::new(),
            ref s => format!("  [{}]", status_word(s)),
        };
        println!("{}  {line}:{col}  {}  {what}{status}", i.id, i.author);
    }
    Ok(())
}

// ---- accept, reject

/// Which open suggestions: all, an author's, or those named.
enum Select {
    All,
    Author(String),
    Ids(Vec<String>),
}

fn parse_select(command: &str, flags: &[String]) -> Result<Select> {
    let mut all = false;
    let mut author = None;
    let mut ids: Vec<String> = Vec::new();
    let mut it = flags.iter().peekable();
    while let Some(f) = it.next() {
        match f.as_str() {
            "--all" => all = true,
            "--author" => match it.next() {
                Some(name) => author = Some(name.clone()),
                None => return misuse(format!("review {command}: --author needs a name")),
            },
            "--id" => {
                while let Some(id) = it.next_if(|n| !n.starts_with("--")) {
                    ids.push(id.clone());
                }
                if ids.is_empty() {
                    return misuse(format!("review {command}: --id needs an id"));
                }
            }
            other => return misuse(format!("review {command}: unknown option {other}")),
        }
    }
    match (all, author, ids.is_empty()) {
        (true, None, true) => Ok(Select::All),
        (false, Some(a), true) => Ok(Select::Author(a)),
        (false, None, false) => Ok(Select::Ids(ids)),
        (false, None, true) => misuse(format!("review {command}: which suggestions? --all, --author NAME or --id ID...")),
        _ => misuse(format!("review {command}: use one of --all, --author and --id")),
    }
}

/// The open suggestions chosen, and the ids asked for that are not (each with why).
fn chosen<'a>(items: &'a [Item], select: &Select) -> (Vec<&'a Item>, Vec<(String, String)>) {
    let suggestions = items.iter().filter(|i| i.kind == Kind::Suggestion && i.state == State::Open);
    match select {
        Select::All => (suggestions.collect(), Vec::new()),
        Select::Author(name) => (suggestions.filter(|i| i.author.eq_ignore_ascii_case(name)).collect(), Vec::new()),
        Select::Ids(ids) => {
            let mut found = Vec::new();
            let mut missing = Vec::new();
            for id in ids {
                match items.iter().find(|i| i.id == *id) {
                    Some(i) if i.kind != Kind::Suggestion => missing.push((id.clone(), "is a comment, not a suggestion".into())),
                    Some(i) if i.state != State::Open => missing.push((id.clone(), format!("is already {}", format!("{:?}", i.state).to_lowercase()))),
                    Some(i) => found.push(i),
                    None => missing.push((id.clone(), "is not in the review".into())),
                }
            }
            (found, missing)
        }
    }
}

/// Of the suggestions to apply, those that can be: not outdated or orphaned, and not overlapping another (the one
/// earlier in the text stays). With the reasons of the others.
fn applicable(chosen: Vec<&Item>) -> (Vec<&Item>, Vec<(String, String)>) {
    let mut sorted = chosen;
    sorted.sort_by_key(|i| (i.range.start, i.range.end));
    let mut kept: Vec<&Item> = Vec::new();
    let mut skipped = Vec::new();
    for i in sorted {
        if !matches!(i.status, Status::Exact | Status::Moved) {
            skipped.push((i.id.clone(), format!("{}: the text it was made on has changed", status_word(&i.status))));
        } else if let Some(other) = kept.iter().find(|k| i.range.start < k.range.end && k.range.start < i.range.end) {
            skipped.push((i.id.clone(), format!("overlaps {}", other.id)));
        } else {
            kept.push(i);
        }
    }
    (kept, skipped)
}

fn decide(doc: &Path, flags: &[String], decision: Decision) -> Result {
    let accept = decision == Decision::Accepted;
    let command = if accept { "accept" } else { "reject" };
    let select = parse_select(command, flags)?;
    let text = read(doc)?;
    let items = items_of(doc, &text)?;
    let (picked, missing) = chosen(&items, &select);
    for (id, why) in &missing {
        eprintln!("pw: {id} {why}");
    }
    let (done, skipped) = if accept { applicable(picked) } else { (picked, Vec::new()) };
    for (id, why) in &skipped {
        println!("skipped {id}: {why}");
    }
    if done.is_empty() {
        println!("nothing {}", if accept { "accepted" } else { "rejected" });
        return Ok(());
    }
    if accept {
        let applied = write_core::review::apply_all(&text, &done.iter().map(|i| (*i).clone()).collect::<Vec<_>>());
        std::fs::write(doc, applied).or_else(|e| fail(format!("{}: {e}", doc.display())))?;
    }
    let me = Person::in_folder(doc.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")));
    for i in &done {
        Review::decide(doc, &me, &i.id, decision).or_else(|e| fail(format!("{}: {e}", i.id)))?;
    }
    let ids: Vec<&str> = done.iter().map(|i| i.id.as_str()).collect();
    println!("{} {} as {}: {}", if accept { "accepted" } else { "rejected" }, done.len(), me.name, ids.join(", "));
    Ok(())
}

// ---- export

fn export(doc: &Path, flags: &[String]) -> Result {
    let (mut word, mut markdown) = (None, None);
    let mut it = flags.iter();
    while let Some(f) = it.next() {
        let slot = match f.as_str() {
            "--word" => &mut word,
            "--markdown" => &mut markdown,
            other => return misuse(format!("review export: unknown option {other}")),
        };
        match it.next() {
            Some(path) => *slot = Some(PathBuf::from(path)),
            None => return misuse(format!("review export: {f} needs a file")),
        }
    }
    let text = read(doc)?;
    match (word, markdown) {
        (Some(to), None) => {
            let to = if to.extension().is_some_and(|e| e.eq_ignore_ascii_case("docx")) { to } else { to.with_extension("docx") };
            let made = exchange::export(doc, &text, &to).or_else(fail)?;
            println!("{}: {} suggestions, {} comments", to.display(), made.suggestions, made.comments);
            for s in made.skipped.iter().chain(&made.notes) {
                println!("note: {s}");
            }
            Ok(())
        }
        (None, Some(to)) => {
            let items = items_of(doc, &text)?;
            let (out, skipped) = critic::markdown_with_changes(&text, &items);
            std::fs::write(&to, out).or_else(|e| fail(format!("{}: {e}", to.display())))?;
            let shown = items.iter().filter(|i| i.state == State::Open).count() - skipped.len();
            println!("{}: {shown} suggestions and comments", to.display());
            for id in skipped {
                println!("note: {id} left out (outdated or overlapping)");
            }
            Ok(())
        }
        (None, None) => misuse("review export: --word out.docx or --markdown out.md"),
        (Some(_), Some(_)) => misuse("review export: one of --word and --markdown"),
    }
}

// ---- import

fn import(doc: &Path, flags: &[String]) -> Result {
    let mut file = None;
    let mut untracked = false;
    for f in flags {
        match f.as_str() {
            "--untracked" => untracked = true,
            other if other.starts_with('-') => return misuse(format!("review import: unknown option {other}")),
            other if file.is_none() => file = Some(PathBuf::from(other)),
            _ => return misuse("review import: one file"),
        }
    }
    let Some(file) = file else {
        return misuse("review import: a .docx or a .review file");
    };
    let text = read(doc)?;
    let ext = file.extension().map(|e| e.to_string_lossy().to_lowercase());
    match ext.as_deref() {
        Some("docx") => {
            let made = exchange::import(doc, &text, &file, untracked, "Word").or_else(fail)?;
            println!("{} suggestions and {} comments from {}", made.suggestions, made.comments, if made.authors.is_empty() { "nobody".into() } else { made.authors.join(", ") });
            for u in made.unplaced {
                println!("not placed: {u}");
            }
            Ok(())
        }
        Some(e) if e == package::EXTENSION => {
            let opened = package::open(&file).or_else(|e| fail(e.to_string()))?;
            if opened.manifest.kind != PackageKind::Back {
                return fail(format!("{}: a document to review, not a review sent back (open it in the app)", file.display()));
            }
            let m = package::merge(doc, &opened).or_else(|e| fail(e.to_string()))?;
            println!("{} suggestions, {} comments and {} other records from {}", m.suggestions, m.comments, m.others, if m.people.is_empty() { "nobody new".into() } else { m.people.join(", ") });
            Ok(())
        }
        _ => misuse(format!("{}: a .docx or a .review file", file.display())),
    }
}
