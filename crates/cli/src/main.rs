//! `pw`: Papers Write from the command line.
//!
//! - `pw <file>…` opens the files in the app;
//! - `pw review list|accept|reject|export|import <doc> …` works on the review of a document (the files beside it,
//!   `write_core::review`) as the app does, without the app.

mod open;
mod review;

use std::process::ExitCode;

const USAGE: &str = "\
pw: Papers Write from the command line

usage:
  pw <file>...                          open the files in Papers Write
  pw review list <doc> [--json]         the open suggestions and comments
  pw review accept <doc> (--all | --author NAME | --id ID...)
                                        apply the suggestions to the file, and record the decisions
  pw review reject <doc> (--all | --author NAME | --id ID...)
                                        record the decisions (the file is not changed)
  pw review export <doc> --word out.docx | --markdown out.md
                                        tracked changes and comments in Word; CriticMarkup in Markdown
  pw review import <doc> <file.docx|file.review> [--untracked]
                                        a Word review (--untracked: also the changes made with tracking off),
                                        or a review sent back
";

/// What went wrong, and whether the command line was at fault (exit 2: usage; else 1).
pub struct Failure {
    pub message: String,
    pub usage: bool,
}

pub type Result<T = ()> = std::result::Result<T, Failure>;

pub fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(Failure { message: message.into(), usage: false })
}

pub fn misuse<T>(message: impl Into<String>) -> Result<T> {
    Err(Failure { message: message.into(), usage: true })
}

fn run(args: &[String]) -> Result {
    match args.first().map(String::as_str) {
        None | Some("-h" | "--help" | "help") => {
            print!("{USAGE}");
            Ok(())
        }
        Some("-V" | "--version") => {
            println!("pw {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("review") => review::run(&args[1..]),
        Some(flag) if flag.starts_with('-') => misuse(format!("unknown option {flag}")),
        Some(_) => open::open(args),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(f) => {
            eprintln!("pw: {}", f.message);
            if f.usage {
                eprintln!("try `pw --help`");
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}
