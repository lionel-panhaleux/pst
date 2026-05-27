use std::io;

use memchr::{memchr, memchr3, memchr_iter};

pub const RS: u8 = 0x1E;
pub const US: u8 = 0x1F;
const GS: u8 = 0x1D;
pub const NL: u8 = 0x0A;
pub const STATUS_WIDTH: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Open,
    Wip,
    Closed,
}

impl Status {
    pub fn padded(self) -> &'static [u8] {
        match self {
            Status::Open => b"open  ",
            Status::Wip => b"wip   ",
            Status::Closed => b"closed",
        }
    }

    /// Status fields are fixed 6-byte padded literals, so match them whole.
    pub fn from_field(field: &[u8]) -> Option<Status> {
        match field {
            b"open  " => Some(Status::Open),
            b"wip   " => Some(Status::Wip),
            b"closed" => Some(Status::Closed),
            _ => None,
        }
    }

    pub fn from_word(word: &str) -> Option<Status> {
        match word {
            "open" => Some(Status::Open),
            "wip" => Some(Status::Wip),
            "closed" => Some(Status::Closed),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Fields<'a> {
    pub status: &'a [u8],
    pub tags: &'a [u8],
    pub body: &'a [u8],
}

/// Split a line (no trailing `\n`) into three fields; `None` unless exactly two RS.
pub fn parse_line(line: &[u8]) -> Option<Fields<'_>> {
    let a = memchr(RS, line)?;
    let b = a + 1 + memchr(RS, &line[a + 1..])?;
    if memchr(RS, &line[b + 1..]).is_some() {
        return None;
    }
    Some(Fields {
        status: &line[..a],
        tags: &line[a + 1..b],
        body: &line[b + 1..],
    })
}

/// Build a line `status␞tags␞body` (no trailing `\n`) into one buffer. Tags are
/// borrowed, not owned, so nothing is copied until it lands in `out`. UTF-8 is
/// not checked here — callers pass `&str`-derived bytes and `lint` re-checks.
pub fn encode_fields(status: Status, tags: &[&[u8]], body: &[u8]) -> io::Result<Vec<u8>> {
    let bad = |m| io::Error::new(io::ErrorKind::InvalidInput, m);
    if body.is_empty() {
        return Err(bad("empty body"));
    }
    if has_control(body) {
        return Err(bad("control byte in body"));
    }
    if tags.iter().any(|t| t.is_empty() || has_control(t)) {
        return Err(bad("invalid tag"));
    }
    let cap = STATUS_WIDTH + 2 + body.len() + tags.iter().map(|t| t.len() + 1).sum::<usize>();
    let mut out = Vec::with_capacity(cap);
    out.extend_from_slice(status.padded());
    out.push(RS);
    for (i, t) in tags.iter().enumerate() {
        if i > 0 {
            out.push(US);
        }
        out.extend_from_slice(t);
    }
    out.push(RS);
    out.extend_from_slice(body);
    Ok(out)
}

#[derive(Debug, PartialEq, Eq)]
pub enum LineError {
    FieldCount,
    BadStatus,
    EmptyBody,
    ForbiddenByte,
    NotUtf8,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LintError {
    MissingTrailingNewline,
    Line { number: usize, error: LineError },
}

/// Validate the whole DB buffer; empty `Vec` means valid.
pub fn lint(bytes: &[u8]) -> Vec<LintError> {
    let mut errors = Vec::new();
    if bytes.is_empty() {
        return errors;
    }
    if *bytes.last().unwrap() != NL {
        errors.push(LintError::MissingTrailingNewline);
    }
    let mut start = 0;
    let mut number = 0;
    for nl in memchr_iter(NL, bytes) {
        number += 1;
        if let Err(error) = validate_line(&bytes[start..nl]) {
            errors.push(LintError::Line { number, error });
        }
        start = nl + 1;
    }
    if start < bytes.len() {
        if let Err(error) = validate_line(&bytes[start..]) {
            errors.push(LintError::Line { number: number + 1, error });
        }
    }
    errors
}

fn validate_line(line: &[u8]) -> Result<(), LineError> {
    let f = parse_line(line).ok_or(LineError::FieldCount)?;
    if Status::from_field(f.status).is_none() {
        return Err(LineError::BadStatus);
    }
    if !tags_ok(f.tags) {
        return Err(LineError::ForbiddenByte);
    }
    if f.body.is_empty() {
        return Err(LineError::EmptyBody);
    }
    if has_control(f.body) {
        return Err(LineError::ForbiddenByte);
    }
    if std::str::from_utf8(f.body).is_err() {
        return Err(LineError::NotUtf8);
    }
    Ok(())
}

fn tags_ok(field: &[u8]) -> bool {
    field.is_empty()
        || field
            .split(|&b| b == US)
            .all(|t| !t.is_empty() && memchr3(RS, GS, NL, t).is_none())
}

fn has_control(b: &[u8]) -> bool {
    memchr3(RS, US, GS, b).is_some() || memchr(NL, b).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_codec() {
        for s in [Status::Open, Status::Wip, Status::Closed] {
            assert_eq!(s.padded().len(), STATUS_WIDTH);
            assert_eq!(Status::from_field(s.padded()), Some(s));
        }
        assert_eq!(Status::from_field(b"open"), None);
        assert_eq!(Status::from_field(b"OPEN  "), None);
        assert_eq!(Status::from_word("wip"), Some(Status::Wip));
        assert_eq!(Status::from_word("done"), None);
    }

    #[test]
    fn parse_line_fields() {
        let f = parse_line(b"open  \x1ebug\x1flogin\x1ehello").unwrap();
        assert_eq!((f.status, f.tags, f.body), (&b"open  "[..], &b"bug\x1flogin"[..], &b"hello"[..]));
        assert_eq!(parse_line(b"wip   \x1e\x1ex").unwrap().tags, b"");
        assert!(parse_line(b"open  \x1ebody").is_none());
        assert!(parse_line(b"a\x1eb\x1ec\x1ed").is_none());
    }

    #[test]
    fn encode_roundtrips_and_rejects() {
        let line = encode_fields(Status::Open, &[b"bug", b"login"], b"hi").unwrap();
        assert_eq!(line, b"open  \x1ebug\x1flogin\x1ehi");
        assert_eq!(encode_fields(Status::Wip, &[], b"x").unwrap(), b"wip   \x1e\x1ex");
        assert!(encode_fields(Status::Open, &[], b"").is_err());
        assert!(encode_fields(Status::Open, &[], b"a\nb").is_err());
        assert!(encode_fields(Status::Open, &[b"a\x1fb"], b"x").is_err());
        assert!(encode_fields(Status::Open, &[b""], b"x").is_err());
    }

    #[test]
    fn lint_accepts_and_pinpoints() {
        assert_eq!(lint(b""), vec![]);
        assert_eq!(lint(b"open  \x1e\x1efirst\nclosed\x1ebug\x1esecond\n"), vec![]);
        assert_eq!(lint(b"open  \x1e\x1efirst"), vec![LintError::MissingTrailingNewline]);
        assert_eq!(
            lint(b"open  \x1e\x1eok\nbadline\n"),
            vec![LintError::Line { number: 2, error: LineError::FieldCount }]
        );
    }

    #[test]
    fn validate_rejects_each_shape() {
        let cases: &[(&[u8], LineError)] = &[
            (b"open  \x1ebody", LineError::FieldCount),
            (b"openx \x1e\x1ebody", LineError::BadStatus),
            (b"open\x1e\x1ebody", LineError::BadStatus),
            (b"open  \x1e\x1e", LineError::EmptyBody),
            (b"open  \x1e\x1ebo\x1ddy", LineError::ForbiddenByte),
            (b"open  \x1ea\x1f\x1fb\x1ebody", LineError::ForbiddenByte),
            (b"open  \x1e\x1e\xff\xfe", LineError::NotUtf8),
        ];
        for (line, want) in cases {
            assert_eq!(validate_line(line).unwrap_err(), *want, "{line:?}");
        }
    }
}
