//! Drives the shared wire vectors through the Rust kanonak-wire port.

use kanonak_wire::*;
use serde_json::Value as J;
use std::fs;
use std::path::PathBuf;

/// Rust writer params are exact-width types and `&str` is always well-formed
/// UTF-8: none of the representability capabilities apply.
const CAPABILITIES: &[&str] = &[];

fn vectors_file() -> PathBuf {
    // tests run from the crate root (rust/); vectors are at ../vectors.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("..");
    p.push("vectors");
    p.push("wire-vectors.json");
    p
}

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

fn bytes_to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn check_error(id: &str, op: &str, e: &WireError, want: &J, fails: &mut usize) {
    let want_kind = want.get("kind").and_then(|k| k.as_str()).unwrap_or("");
    if e.kind.as_str() != want_kind {
        eprintln!("{id}: {op}: expected error kind {want_kind}, got {} ({e})", e.kind.as_str());
        *fails += 1;
        return;
    }
    if let Some(want_offset) = want.get("offset").and_then(|o| o.as_u64()) {
        if e.offset != Some(want_offset as usize) {
            eprintln!(
                "{id}: {op}: expected error offset {want_offset}, got {:?}",
                e.offset
            );
            *fails += 1;
        }
    }
}

enum ReadResult {
    Num(u64),
    Str(String),
    Hex(String),
    Unit,
}

fn run_read_op(r: &mut WireReader, op: &J) -> Result<ReadResult, WireError> {
    let name = op.get("op").and_then(|o| o.as_str()).unwrap_or("");
    let n = op.get("n").and_then(|n| n.as_u64()).unwrap_or(0) as usize;
    match name {
        "u8" => Ok(ReadResult::Num(r.u8()? as u64)),
        "u16be" => Ok(ReadResult::Num(r.u16_be()? as u64)),
        "u32be" => Ok(ReadResult::Num(r.u32_be()? as u64)),
        "bytes" => Ok(ReadResult::Hex(bytes_to_hex(r.bytes(n)?))),
        "uuid" => Ok(ReadResult::Str(r.uuid()?)),
        "utf8" => Ok(ReadResult::Str(r.utf8(n)?.to_string())),
        "lenPrefixedBytes16" => Ok(ReadResult::Hex(bytes_to_hex(r.len_prefixed_bytes16()?))),
        "rest" => Ok(ReadResult::Hex(bytes_to_hex(r.rest()))),
        "remaining" => Ok(ReadResult::Num(r.remaining() as u64)),
        "expectEnd" => {
            r.expect_end()?;
            Ok(ReadResult::Unit)
        }
        other => panic!("conformance: unknown read op '{other}'"),
    }
}

fn expected_matches(id: &str, opname: &str, got: &ReadResult, expected: &J, fails: &mut usize) {
    let ok = match got {
        ReadResult::Num(v) => expected.as_u64() == Some(*v),
        ReadResult::Str(s) | ReadResult::Hex(s) => expected.as_str() == Some(s.as_str()),
        ReadResult::Unit => true,
    };
    if !ok {
        let got_str = match got {
            ReadResult::Num(v) => v.to_string(),
            ReadResult::Str(s) | ReadResult::Hex(s) => s.clone(),
            ReadResult::Unit => "()".into(),
        };
        eprintln!("{id}: {opname}: expected {expected}, got {got_str}");
        *fails += 1;
    }
}

fn run_write_op(w: &mut WireWriter, op: &J) -> Result<(), WireError> {
    let name = op.get("op").and_then(|o| o.as_str()).unwrap_or("");
    match name {
        "u8" => {
            w.u8(op["value"].as_u64().unwrap() as u8);
            Ok(())
        }
        "u16be" => {
            w.u16_be(op["value"].as_u64().unwrap() as u16);
            Ok(())
        }
        "u32be" => {
            w.u32_be(op["value"].as_u64().unwrap() as u32);
            Ok(())
        }
        "bytes" => {
            w.bytes(&hex_to_bytes(op["hex"].as_str().unwrap()));
            Ok(())
        }
        "uuid" => w.uuid(op["value"].as_str().unwrap()).map(|_| ()),
        "utf8" => {
            w.utf8(op["value"].as_str().unwrap());
            Ok(())
        }
        "lenPrefixedBytes16" => w
            .len_prefixed_bytes16(&hex_to_bytes(op["hex"].as_str().unwrap()))
            .map(|_| ()),
        other => panic!("conformance: unknown write op '{other}'"),
    }
}

#[test]
fn wire_vectors() {
    let data: J = serde_json::from_str(&fs::read_to_string(vectors_file()).unwrap()).unwrap();
    let read_vectors = data["readVectors"].as_array().unwrap();
    let write_vectors = data["writeVectors"].as_array().unwrap();

    let mut pass = 0usize;
    let mut fails = 0usize;
    let mut skipped = 0usize;

    for v in read_vectors {
        let id = v["id"].as_str().unwrap();
        if let Some(req) = v.get("requires").and_then(|r| r.as_str()) {
            if !CAPABILITIES.contains(&req) {
                skipped += 1;
                continue;
            }
        }
        let bytes = hex_to_bytes(v["bytes"].as_str().unwrap());
        let mut r = WireReader::new(&bytes);
        let before = fails;
        for op in v["ops"].as_array().unwrap() {
            let opname = op["op"].as_str().unwrap();
            if let Some(want) = op.get("expectError") {
                match run_read_op(&mut r, op) {
                    Ok(_) => {
                        eprintln!("{id}: {opname}: expected {}, got a value", want["kind"]);
                        fails += 1;
                    }
                    Err(e) => check_error(id, opname, &e, want, &mut fails),
                }
                break;
            }
            match run_read_op(&mut r, op) {
                Ok(got) => {
                    if let Some(expected) = op.get("expected") {
                        expected_matches(id, opname, &got, expected, &mut fails);
                        if fails > before {
                            break;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{id}: {opname}: threw {e}");
                    fails += 1;
                    break;
                }
            }
        }
        if fails == before {
            pass += 1;
        }
    }

    for v in write_vectors {
        let id = v["id"].as_str().unwrap();
        if let Some(req) = v.get("requires").and_then(|r| r.as_str()) {
            if !CAPABILITIES.contains(&req) {
                skipped += 1;
                continue;
            }
        }
        let mut w = WireWriter::new();
        let before = fails;
        for op in v["ops"].as_array().unwrap() {
            let opname = op["op"].as_str().unwrap();
            if let Some(want) = op.get("expectError") {
                match run_write_op(&mut w, op) {
                    Ok(()) => {
                        eprintln!("{id}: {opname}: expected {}, got success", want["kind"]);
                        fails += 1;
                    }
                    Err(e) => check_error(id, opname, &e, want, &mut fails),
                }
                break;
            }
            if let Err(e) = run_write_op(&mut w, op) {
                eprintln!("{id}: {opname}: threw {e}");
                fails += 1;
                break;
            }
        }
        if fails == before {
            if let Some(expected) = v.get("expectedBytes").and_then(|e| e.as_str()) {
                let got = bytes_to_hex(w.as_bytes());
                if got != expected {
                    eprintln!("{id}: expected bytes {expected}, got {got}");
                    fails += 1;
                }
            }
        }
        if fails == before {
            pass += 1;
        }
    }

    let total = read_vectors.len() + write_vectors.len();
    println!("wire-vectors: {pass}/{total} pass ({skipped} skipped)");
    assert_eq!(fails, 0, "{fails} VECTOR(S) FAILED");
}
