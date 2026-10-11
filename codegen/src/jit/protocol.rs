//! The byte forms between a dev-tier parent and the process that runs
//! the program: the request a runner reads (compiler.md §190.1 rule 2)
//! and the outcome a forked child or a runner writes.
//!
//! Every integer is little-endian. A byte string is its `u64` length
//! and then its bytes.

#[cfg(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu")))]
use std::borrow::Cow;
use std::fs::File;
use std::io::Write;

use subscript_compiler::Pos;
#[cfg(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu")))]
use subscript_compiler::{CheckOptions, SourceFile};
use subscript_runtime::TrapKind;

use super::{RunError, TrapReport};
use crate::lower::internal;

/// The first bytes of a runner request.
#[cfg(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu")))]
const REQUEST_MAGIC: &[u8; 4] = b"SJR1";

/// Outcome tags.
const OUTCOME_COMPLETED: u8 = 0;
const OUTCOME_TRAP: u8 = 1;
const OUTCOME_INTERNAL: u8 = 2;
const OUTCOME_UNRESOLVED_FOREIGN_SYMBOL: u8 = 3;

/// The program and the options that a runner needs to compile and run
/// it. A runner run has no native library and no file provider
/// (compiler.md §190.1 rules 2 and 3), so neither is a field.
/// The parent borrows the caller's files; the runner owns the files it
/// reads.
#[cfg(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu")))]
#[derive(Debug, Clone)]
pub(super) struct RunnerRequest<'a> {
    pub(super) files: Cow<'a, [SourceFile]>,
    pub(super) check: CheckOptions,
    pub(super) fail_alloc_after: Option<u64>,
    pub(super) freed_handle_diagnostics: bool,
}

#[cfg(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu")))]
fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(bytes);
}

#[cfg(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu")))]
fn put_strings(out: &mut Vec<u8>, strings: &[String]) {
    out.extend_from_slice(&(strings.len() as u64).to_le_bytes());
    for string in strings {
        put_bytes(out, string.as_bytes());
    }
}

#[cfg(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu")))]
impl RunnerRequest<'_> {
    pub(super) fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(REQUEST_MAGIC);
        out.extend_from_slice(&(self.files.len() as u64).to_le_bytes());
        for file in self.files.iter() {
            put_bytes(&mut out, file.name.as_bytes());
            put_bytes(&mut out, file.source.as_bytes());
            out.push(u8::from(file.dts));
            out.push(u8::from(file.entry));
        }
        put_strings(&mut out, &self.check.enabled_modules);
        put_strings(&mut out, &self.check.poison_missing_modules);
        match self.fail_alloc_after {
            Some(n) => {
                out.push(1);
                out.extend_from_slice(&n.to_le_bytes());
            }
            None => out.push(0),
        }
        out.push(u8::from(self.freed_handle_diagnostics));
        out
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<RunnerRequest<'static>, RunError> {
        let mut reader = ProtocolReader::new(bytes, "JIT runner request");
        if reader.take(REQUEST_MAGIC.len())? != REQUEST_MAGIC {
            return Err(reader.error("unknown request magic"));
        }
        let file_count = reader.u64()?;
        let mut files = Vec::new();
        for _ in 0..file_count {
            let name = reader.string()?;
            let source = reader.string()?;
            let mut file = SourceFile::new(name, source);
            file.dts = reader.flag()?;
            file.entry = reader.flag()?;
            files.push(file);
        }
        let mut check = CheckOptions::default();
        check.enabled_modules = reader.strings()?;
        check.poison_missing_modules = reader.strings()?;
        let fail_alloc_after = if reader.flag()? {
            Some(reader.u64()?)
        } else {
            None
        };
        let freed_handle_diagnostics = reader.flag()?;
        if reader.offset != bytes.len() {
            return Err(reader.error("trailing bytes"));
        }
        Ok(RunnerRequest {
            files: Cow::Owned(files),
            check,
            fail_alloc_after,
            freed_handle_diagnostics,
        })
    }
}

fn write_protocol_bytes(file: &mut File, bytes: &[u8]) -> std::io::Result<()> {
    file.write_all(&(bytes.len() as u64).to_le_bytes())?;
    file.write_all(bytes)
}

/// Writes the outcome of one run. Stdout is not part of the outcome: it
/// is in the retained output file.
pub(super) fn write_outcome<T>(
    protocol: &mut File,
    outcome: &Result<T, RunError>,
) -> std::io::Result<()> {
    match outcome {
        Ok(_) => protocol.write_all(&[OUTCOME_COMPLETED])?,
        Err(RunError::Trap(report)) => {
            protocol.write_all(&[OUTCOME_TRAP])?;
            protocol.write_all(&(report.rule as u32).to_le_bytes())?;
            protocol.write_all(&report.pos.line.to_le_bytes())?;
            protocol.write_all(&report.pos.col.to_le_bytes())?;
            write_protocol_bytes(protocol, report.pos.file.as_bytes())?;
            write_protocol_bytes(protocol, report.message.as_bytes())?;
        }
        Err(RunError::UnresolvedForeignSymbol(name)) => {
            protocol.write_all(&[OUTCOME_UNRESOLVED_FOREIGN_SYMBOL])?;
            write_protocol_bytes(protocol, name.as_bytes())?;
        }
        Err(error) => {
            protocol.write_all(&[OUTCOME_INTERNAL])?;
            write_protocol_bytes(protocol, error.to_string().as_bytes())?;
        }
    }
    protocol.flush()
}

struct ProtocolReader<'a> {
    bytes: &'a [u8],
    offset: usize,
    form: &'static str,
}

impl<'a> ProtocolReader<'a> {
    fn new(bytes: &'a [u8], form: &'static str) -> Self {
        Self {
            bytes,
            offset: 0,
            form,
        }
    }

    fn error(&self, what: &str) -> RunError {
        RunError::Internal(internal(format!("{what} in {}", self.form)))
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], RunError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| self.error("overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| self.error("truncated field"))?;
        self.offset = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8, RunError> {
        Ok(self.take(1)?[0])
    }

    #[cfg(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu")))]
    fn flag(&mut self) -> Result<bool, RunError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(self.error("invalid flag")),
        }
    }

    fn u32(&mut self) -> Result<u32, RunError> {
        let mut word = [0; 4];
        word.copy_from_slice(self.take(4)?);
        Ok(u32::from_le_bytes(word))
    }

    fn u64(&mut self) -> Result<u64, RunError> {
        let mut word = [0; 8];
        word.copy_from_slice(self.take(8)?);
        Ok(u64::from_le_bytes(word))
    }

    fn bytes(&mut self) -> Result<&'a [u8], RunError> {
        let len = self.u64()?;
        let len = usize::try_from(len).map_err(|_| self.error("oversized field"))?;
        self.take(len)
    }

    fn string(&mut self) -> Result<String, RunError> {
        let bytes = self.bytes()?;
        String::from_utf8(bytes.to_vec()).map_err(|error| self.error(&format!("{error}")))
    }

    #[cfg(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu")))]
    fn strings(&mut self) -> Result<Vec<String>, RunError> {
        let count = self.u64()?;
        (0..count).map(|_| self.string()).collect()
    }
}

/// Reads an outcome that [`write_outcome`] wrote, with the retained
/// stdout of the run.
pub(super) fn parse_outcome(bytes: &[u8], stdout: Vec<u8>) -> Result<Vec<u8>, RunError> {
    let mut protocol = ProtocolReader::new(bytes, "JIT child protocol");
    match protocol.u8()? {
        OUTCOME_COMPLETED => Ok(stdout),
        OUTCOME_TRAP => {
            let rule_number = protocol.u32()?;
            let rule = TrapKind::from_u32(rule_number).ok_or_else(|| {
                RunError::Internal(internal(format!(
                    "unknown trap kind {rule_number} in JIT child protocol"
                )))
            })?;
            let line = protocol.u32()?;
            let col = protocol.u32()?;
            let file = protocol.string()?;
            let message = protocol.string()?;
            Err(RunError::Trap(TrapReport {
                rule,
                message,
                pos: Pos::new(file, line, col),
                stdout,
            }))
        }
        OUTCOME_INTERNAL => Err(RunError::Internal(protocol.string()?)),
        OUTCOME_UNRESOLVED_FOREIGN_SYMBOL => {
            Err(RunError::UnresolvedForeignSymbol(protocol.string()?))
        }
        tag => Err(RunError::Internal(internal(format!(
            "unknown JIT child protocol tag {tag}"
        )))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every field of `SourceFile` and `CheckOptions` is set to a value
    /// other than its default, and the `Debug` forms compare every field.
    #[cfg(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu")))]
    #[test]
    fn a_request_reads_back_every_field() {
        let mut ambient = SourceFile::ambient("lib.d.ts", "declare function f(): void;\n");
        ambient.entry = true;
        let mut plain = SourceFile::new("util.ts", "export const x: i32 = 1;\n");
        plain.dts = false;
        plain.entry = false;
        let mut check = CheckOptions::default();
        check.enabled_modules = vec!["node:fs/promises".to_string()];
        check.poison_missing_modules = vec!["missing".to_string()];
        let request = RunnerRequest {
            files: Cow::Owned(vec![
                SourceFile::entry("main.ts", "export function main(): void {}\n"),
                ambient,
                plain,
            ]),
            check,
            fail_alloc_after: Some(7),
            freed_handle_diagnostics: true,
        };
        let read = RunnerRequest::decode(&request.encode()).expect("decode the request");
        assert_eq!(format!("{:?}", read.files), format!("{:?}", request.files));
        assert_eq!(format!("{:?}", read.check), format!("{:?}", request.check));
        assert_eq!(read.fail_alloc_after, Some(7));
        assert!(read.freed_handle_diagnostics);
    }

    #[cfg(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu")))]
    #[test]
    fn a_truncated_request_is_an_internal_error() {
        let request = RunnerRequest {
            files: Cow::Owned(vec![SourceFile::entry("main.ts", "")]),
            check: CheckOptions::default(),
            fail_alloc_after: None,
            freed_handle_diagnostics: false,
        };
        let bytes = request.encode();
        match RunnerRequest::decode(&bytes[..bytes.len() - 1]) {
            Err(RunError::Internal(message)) => assert!(message.contains("truncated"), "{message}"),
            other => panic!("expected an internal error, got {other:?}"),
        }
    }

    #[test]
    fn an_unresolved_symbol_outcome_keeps_its_kind() {
        let path = std::env::temp_dir().join(format!(
            "subscript-jit-protocol-test-{}",
            std::process::id()
        ));
        let mut file = File::create(&path).expect("create the protocol file");
        let outcome: Result<(), RunError> = Err(RunError::UnresolvedForeignSymbol("f".into()));
        write_outcome(&mut file, &outcome).expect("write the outcome");
        drop(file);
        let bytes = std::fs::read(&path).expect("read the protocol file");
        std::fs::remove_file(&path).expect("remove the protocol file");
        match parse_outcome(&bytes, Vec::new()) {
            Err(RunError::UnresolvedForeignSymbol(name)) => assert_eq!(name, "f"),
            other => panic!("expected the unresolved symbol, got {other:?}"),
        }
    }
}
