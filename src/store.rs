use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

use memchr::memchr_iter;

use crate::format::{encode_fields, parse_line, Status, NL, RS, STATUS_WIDTH, US};

const CHUNK: usize = 64 * 1024;

pub enum TagOp {
    Add(String),
    Remove(String),
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Store { path: path.into() }
    }

    pub fn add(&self, status: Status, tags: &[String], body: &str) -> io::Result<usize> {
        let tags: Vec<&[u8]> = tags.iter().map(|t| t.as_bytes()).collect();
        let mut line = encode_fields(status, &tags, body.as_bytes())?;
        line.push(NL);

        let mut f = OpenOptions::new().create(true).read(true).append(true).open(&self.path)?;
        f.lock()?;
        let (lines, unterminated) = count_lines(&mut f)?;
        if unterminated {
            f.write_all(&[NL])?; // don't let the new ticket fuse onto an unterminated last line
        }
        f.write_all(&line)?;
        f.sync_all()?;
        Ok(lines + usize::from(unterminated) + 1)
    }

    pub fn set_status(&self, n: usize, status: Status) -> io::Result<()> {
        let mut f = self.open_locked()?;
        let (start, end) = line_span(&mut f, n)?.ok_or_else(|| not_found(n))?;
        // The status field is the 6 bytes at `start`; the next byte must be its
        // closing RS. Refuse to pwrite into a line not shaped that way.
        if end - start < STATUS_WIDTH as u64 + 1 {
            return Err(malformed());
        }
        let mut sep = [0u8];
        f.read_exact_at(&mut sep, start + STATUS_WIDTH as u64)?;
        if sep[0] != RS {
            return Err(malformed());
        }
        f.write_all_at(status.padded(), start)?;
        f.sync_all()
    }

    pub fn rewrite_line(
        &self,
        n: usize,
        new_status: Option<Status>,
        ops: &[TagOp],
        new_body: Option<&str>,
    ) -> io::Result<()> {
        let mut f = self.open_locked()?;
        let (start, end) = line_span(&mut f, n)?.ok_or_else(|| not_found(n))?;

        // Read only from the edited line to EOF; the prefix stays put on disk.
        f.seek(SeekFrom::Start(start))?;
        let mut rest = Vec::new();
        f.read_to_end(&mut rest)?;
        let len = (end - start) as usize;

        let cur = parse_line(&rest[..len]).ok_or_else(malformed)?;
        let status = new_status.or_else(|| Status::from_field(cur.status)).ok_or_else(malformed)?;
        let tags = apply_tag_ops(cur.tags, ops);
        let body = new_body.map_or(cur.body, str::as_bytes);
        let line = encode_fields(status, &tags, body)?;

        f.seek(SeekFrom::Start(start))?;
        f.write_all(&line)?;
        f.write_all(&rest[len..])?; // the '\n' and every later line, byte-for-byte
        f.set_len(start + line.len() as u64 + (rest.len() - len) as u64)?;
        f.sync_all()
    }

    pub fn show(&self, n: usize) -> io::Result<String> {
        let mut f = File::open(&self.path)?;
        let (start, end) = line_span(&mut f, n)?.ok_or_else(|| not_found(n))?;
        let mut line = vec![0u8; (end - start) as usize];
        f.read_exact_at(&mut line, start)?;
        let cur = parse_line(&line).ok_or_else(malformed)?;

        let status = String::from_utf8_lossy(cur.status);
        let tags: Vec<_> = if cur.tags.is_empty() {
            Vec::new()
        } else {
            cur.tags.split(|&b| b == US).map(String::from_utf8_lossy).collect()
        };
        let mut out = format!(
            "#{n}  [{}]  {}\n{}\n",
            status.trim_end(),
            tags.join(", "),
            String::from_utf8_lossy(cur.body)
        );
        if let Some(detail) = self.find_detail(n) {
            out.push_str(&format!("\n--- {} ---\n", detail.file_name().unwrap().to_string_lossy()));
            out.push_str(&std::fs::read_to_string(&detail)?);
        }
        Ok(out)
    }

    fn find_detail(&self, n: usize) -> Option<PathBuf> {
        let dir = self.path.parent().unwrap_or(Path::new(".")).join("details");
        let prefix = format!("{n}-");
        std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with(&prefix) && s.ends_with(".md"))
        })
    }

    fn open_locked(&self) -> io::Result<File> {
        let f = OpenOptions::new().read(true).write(true).open(&self.path)?;
        f.lock()?;
        Ok(f)
    }
}

/// Stream the file counting newlines; also reports whether it ends without one.
fn count_lines(f: &mut File) -> io::Result<(usize, bool)> {
    let mut buf = [0u8; CHUNK];
    let mut lines = 0;
    let mut last = NL;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        lines += memchr_iter(NL, &buf[..n]).count();
        last = buf[n - 1];
    }
    Ok((lines, last != NL))
}

/// Byte range `[start, end)` of 1-based line `n` (`end` indexes its `\n`), found
/// by streaming so we never hold the whole file. `None` if line `n` is absent or
/// unterminated.
fn line_span(f: &mut File, n: usize) -> io::Result<Option<(u64, u64)>> {
    if n == 0 {
        return Ok(None);
    }
    let mut buf = [0u8; CHUNK];
    let mut base = 0u64;
    let mut count = 0;
    let mut start = 0u64;
    loop {
        let read = f.read(&mut buf)?;
        if read == 0 {
            return Ok(None);
        }
        for i in memchr_iter(NL, &buf[..read]) {
            count += 1;
            let nl = base + i as u64;
            if count == n {
                return Ok(Some((start, nl)));
            }
            start = nl + 1;
        }
        base += read as u64;
    }
}

/// Apply +/- tag ops to the existing tags field, returning borrowed tokens
/// (into `field` and `ops`) so no tag bytes are copied.
fn apply_tag_ops<'a>(field: &'a [u8], ops: &'a [TagOp]) -> Vec<&'a [u8]> {
    let mut tags: Vec<&[u8]> =
        if field.is_empty() { Vec::new() } else { field.split(|&b| b == US).collect() };
    for op in ops {
        match op {
            TagOp::Add(t) => {
                let t = t.as_bytes();
                if !tags.contains(&t) {
                    tags.push(t);
                }
            }
            TagOp::Remove(t) => tags.retain(|&x| x != t.as_bytes()),
        }
    }
    tags
}

fn not_found(n: usize) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("no ticket #{n}"))
}

fn malformed() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "malformed line")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(content: &[u8]) -> PathBuf {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let p = std::env::temp_dir().join(format!("pst-unit-{n}"));
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn line_span_finds_complete_lines_only() {
        let mut f = File::open(tmp(b"open  \x1e\x1eone\nwip   \x1e\x1etwo\n")).unwrap();
        assert_eq!(line_span(&mut f, 1).unwrap(), Some((0, 11)));
        f.rewind().unwrap();
        assert_eq!(line_span(&mut f, 2).unwrap(), Some((12, 23)));
        f.rewind().unwrap();
        assert_eq!(line_span(&mut f, 3).unwrap(), None); // unterminated/absent
        f.rewind().unwrap();
        assert_eq!(line_span(&mut f, 0).unwrap(), None);
    }

    #[test]
    fn tag_ops_borrow_and_dedup() {
        let ops = [TagOp::Add("login".into()), TagOp::Add("bug".into())];
        assert_eq!(apply_tag_ops(b"bug", &ops), vec![&b"bug"[..], b"login"]);
        let ops = [TagOp::Remove("bug".into())];
        assert_eq!(apply_tag_ops(b"bug\x1flogin", &ops), vec![&b"login"[..]]);
    }
}
