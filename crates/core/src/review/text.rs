//! Small text helpers of the review: git's blob hash, UTF-16 positions, names to file slugs, RFC 3339 times.

use std::time::{SystemTime, UNIX_EPOCH};

/// The hash git gives `text` as a blob (`git hash-object`): SHA-1 of `blob <len>\0<bytes>`, in hex. A committed
/// version of a document is the same object for git.
pub fn blob_hash(text: &str) -> String {
    let mut h = sha1_smol::Sha1::new();
    h.update(format!("blob {}\0", text.len()).as_bytes());
    h.update(text.as_bytes());
    h.digest().to_string()
}

/// The position in UTF-16 code units (what CodeMirror counts) of the byte offset `byte` of `text`. An offset inside
/// a character counts as the start of that character; one past the end as the end.
pub fn utf16_of(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    text[..byte].encode_utf16().count()
}

/// The byte offset in `text` of the position `utf16` in UTF-16 code units. A position between the two halves of a
/// surrogate pair gives the end of that character; one past the end gives `text.len()`.
pub fn byte_of(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}

/// The name as a file name part: lowercase ASCII letters and digits, anything else (a run of it) a single `-`;
/// common Latin letters lose their accent (`José` is `jose`); `anon` when nothing is left.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = match c {
            'à'..='å' | 'À'..='Å' => 'a',
            'ç' | 'Ç' => 'c',
            'è'..='ë' | 'È'..='Ë' => 'e',
            'ì'..='ï' | 'Ì'..='Ï' => 'i',
            'ñ' | 'Ñ' => 'n',
            'ò'..='ö' | 'Ò'..='Ö' | 'ø' | 'Ø' => 'o',
            'ù'..='ü' | 'Ù'..='Ü' => 'u',
            'ý' | 'ÿ' | 'Ý' => 'y',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "anon".into() } else { out }
}

/// `YYYY-MM-DDTHH:MM:SSZ` for seconds since the Unix epoch (UTC).
pub fn rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // civil from days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Now, as RFC 3339 in UTC.
pub fn now() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    rfc3339(secs)
}

/// The at most `n` characters of `text` before byte `at`.
pub(crate) fn before(text: &str, at: usize, n: usize) -> &str {
    let start = text[..at].char_indices().rev().nth(n.saturating_sub(1)).map_or(0, |(i, _)| i);
    if n == 0 { &text[at..at] } else { &text[start..at] }
}

/// The at most `n` characters of `text` from byte `at`.
pub(crate) fn after(text: &str, at: usize, n: usize) -> &str {
    let end = text[at..].char_indices().nth(n).map_or(text.len(), |(i, _)| at + i);
    &text[at..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_known_vector() {
        assert_eq!(blob_hash("hello\n"), "ce013625030ba8dba906f756967f9e9ca394464a");
        assert_eq!(blob_hash(""), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
    }

    #[test]
    fn hash_equals_git_hash_object() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let text = "Grounding line é 😀\r\nretreats.\n";
        let Ok(mut child) = Command::new("git")
            .args(["hash-object", "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
        else {
            return; // no git here
        };
        child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), blob_hash(text));
    }

    #[test]
    fn utf16_ascii_accent_emoji_crlf() {
        let t = "aé😀b\r\nc";
        // a=1, é=1 (2 bytes), 😀=2 units (4 bytes), b, \r, \n, c
        assert_eq!(utf16_of(t, 0), 0);
        assert_eq!(utf16_of(t, 1), 1);
        assert_eq!(utf16_of(t, 3), 2);
        assert_eq!(utf16_of(t, 7), 4);
        assert_eq!(utf16_of(t, t.len()), 8);
        assert_eq!(utf16_of(t, 2), 1, "inside é counts as its start");
        assert_eq!(byte_of(t, 2), 3);
        assert_eq!(byte_of(t, 4), 7);
        assert_eq!(byte_of(t, 3), 7, "between the halves of a surrogate pair: the end of the character");
        assert_eq!(byte_of(t, 8), t.len());
        assert_eq!(byte_of(t, 99), t.len());
        for (b, _) in t.char_indices() {
            assert_eq!(byte_of(t, utf16_of(t, b)), b);
        }
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Jane Doe"), "jane-doe");
        assert_eq!(slug("  José  Núñez-Ruiz "), "jose-nunez-ruiz");
        assert_eq!(slug("李"), "anon");
        assert_eq!(slug(""), "anon");
    }

    #[test]
    fn times() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_780_000_000), "2026-05-28T20:26:40Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(now().len(), 20);
    }

    #[test]
    fn context() {
        let t = "héllo wörld";
        assert_eq!(before(t, 7, 3), "lo ");
        assert_eq!(after(t, 7, 3), "wör");
        assert_eq!(before(t, 1, 32), "h");
        assert_eq!(after(t, 7, 0), "");
    }
}
