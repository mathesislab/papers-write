//! The records of a review file and the people who write them.

use serde::{Deserialize, Serialize};

/// Who writes a review file: the header table `[person]` of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub name: String,
    #[serde(default)]
    pub email: String,
    /// the highest number of the ids of this person's records removed by Clean Up Review (0: none): new ids go
    /// above it, so that an id removed is never given again (a decision of someone else may still name it)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub through: u64,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

impl Person {
    pub fn new(name: impl Into<String>, email: impl Into<String>) -> Self {
        Self { name: name.into(), email: email.into(), through: 0 }
    }

    /// `user.name` and `user.email` of the git configuration in `folder` (the `git` command); `None` without a name
    /// or without git.
    pub fn from_git(folder: &std::path::Path) -> Option<Person> {
        let get = |key: &str| {
            let out = std::process::Command::new("git").arg("-C").arg(folder).args(["config", "--get", key]).output().ok()?;
            let v = String::from_utf8(out.stdout).ok()?.trim().to_string();
            (out.status.success() && !v.is_empty()).then_some(v)
        };
        Some(Person { name: get("user.name")?, email: get("user.email").unwrap_or_default(), through: 0 })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Suggestion,
    Comment,
    Reply,
    Decision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Accepted,
    Rejected,
    Resolved,
    Reopened,
    Withdrawn,
}

/// One `[[record]]` table. Only the fields of its kind are set:
///
/// - suggestion: `base`, `start`, `end`, `prefix`, `exact`, `suffix`, `insert` (empty: a deletion; `start == end`: an
///   insertion), `text` (a note), `on` (a counter-suggestion: the suggestion it answers), `supersedes`;
/// - comment: `base`, `start`, `end`, quote, `text`, `supersedes`;
/// - reply: `on`, `text`;
/// - decision: `on`, `decision`.
///
/// `start` and `end` are UTF-8 byte offsets into the text `base` names (the hash of it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub kind: Kind,
    /// RFC 3339, UTC
    pub time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<Decision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
}

impl Record {
    pub(crate) fn blank(id: String, kind: Kind) -> Record {
        Record {
            id,
            kind,
            time: super::text::now(),
            round: None,
            base: None,
            start: None,
            end: None,
            prefix: None,
            exact: None,
            suffix: None,
            insert: None,
            text: None,
            on: None,
            decision: None,
            supersedes: None,
        }
    }

    /// The `n` of `<slug>-<n>`.
    pub(crate) fn number(id: &str) -> Option<u64> {
        id.rsplit_once('-')?.1.parse().ok()
    }

    /// A sort key: the time, then the person's slug and number.
    pub(crate) fn order_key(&self) -> (String, String, u64) {
        let slug = self.id.rsplit_once('-').map_or(self.id.as_str(), |(s, _)| s);
        (self.time.clone(), slug.to_string(), Self::number(&self.id).unwrap_or(0))
    }
}

/// The file as TOML: the header and the records.
#[derive(Default, Deserialize)]
pub(crate) struct FileContent {
    pub person: Option<Person>,
    #[serde(default)]
    pub record: Vec<Record>,
}
