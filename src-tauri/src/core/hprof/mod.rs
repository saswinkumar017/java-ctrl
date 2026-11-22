pub mod analysis;
pub mod model;
pub mod parse;
pub mod reader;

use crate::core::error::Result;
use crate::core::models::HeapAnalysis;
use std::path::Path;
use std::time::Instant;

/// Analyse a binary HPROF heap dump. The file is streamed twice: pass one
/// indexes strings, classes, object headers and GC roots, pass two resolves
/// instance and array references once class layouts are known. This keeps
/// memory proportional to the object graph rather than the dump file.
pub fn analyze_heap_dump(path: &Path) -> Result<HeapAnalysis> {
    let started = Instant::now();
    let (mut state, id_size, timestamp, file_size) = parse::read_pass1(path)?;
    let mismatch = parse::read_pass2(path, &mut state)?;
    Ok(analysis::analyze(
        state,
        id_size,
        timestamp,
        file_size,
        path.display().to_string(),
        mismatch,
        started,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    const S_OBJECT: u64 = 1;
    const S_REFERENCE: u64 = 2;
    const S_WEAK_REFERENCE: u64 = 3;
    const S_DEMO: u64 = 4;
    const S_REFERENT: u64 = 5;
    const S_NEXT: u64 = 6;

    const C_OBJECT: u64 = 0x100;
    const C_REFERENCE: u64 = 0x101;
    const C_WEAK_REFERENCE: u64 = 0x102;
    const C_DEMO: u64 = 0x103;

    const O_A: u64 = 0x200;
    const O_B: u64 = 0x201;
    const O_C: u64 = 0x202;
    const O_BYTES: u64 = 0x203;
    const O_WEAK_REF: u64 = 0x204;
    const O_WEAK_TARGET: u64 = 0x205;

    fn put_u16(out: &mut Vec<u8>, value: u16) {
        out.extend_from_slice(&value.to_be_bytes());
    }

    fn put_u32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_be_bytes());
    }

    fn put_u64(out: &mut Vec<u8>, value: u64) {
        out.extend_from_slice(&value.to_be_bytes());
    }

    fn put_id(out: &mut Vec<u8>, value: u64) {
        put_u64(out, value);
    }

    fn record(tag: u8, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(9 + body.len());
        out.push(tag);
        put_u32(&mut out, 0);
        put_u32(&mut out, body.len() as u32);
        out.extend_from_slice(body);
        out
    }

    fn string_record(id: u64, text: &str) -> Vec<u8> {
        let mut body = Vec::new();
        put_id(&mut body, id);
        body.extend_from_slice(text.as_bytes());
        record(0x01, &body)
    }

    fn load_class_record(class_id: u64, name_id: u64) -> Vec<u8> {
        let mut body = Vec::new();
        put_u32(&mut body, 0);
        put_id(&mut body, class_id);
        put_u32(&mut body, 0);
        put_id(&mut body, name_id);
        record(0x02, &body)
    }

    fn class_dump(
        class_id: u64,
        super_id: u64,
        instance_size: u32,
        instance_fields: &[(u64, u8)],
    ) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(0x20);
        put_id(&mut out, class_id);
        put_u32(&mut out, 0);
        put_id(&mut out, super_id);
        put_id(&mut out, 0); // loader
        put_id(&mut out, 0); // signers
        put_id(&mut out, 0); // protection domain
        put_id(&mut out, 0); // reserved
        put_id(&mut out, 0); // reserved
        put_u32(&mut out, instance_size);
        put_u16(&mut out, 0); // constant pool
        put_u16(&mut out, 0); // static fields
        put_u16(&mut out, instance_fields.len() as u16);
        for &(name_id, ty) in instance_fields {
            put_id(&mut out, name_id);
            out.push(ty);
        }
        out
    }

    fn instance_dump(id: u64, class_id: u64, refs: &[Option<u64>]) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(0x21);
        put_id(&mut out, id);
        put_u32(&mut out, 0);
        put_id(&mut out, class_id);
        put_u32(&mut out, (refs.len() * 8) as u32);
        for target in refs {
            put_id(&mut out, target.unwrap_or(0));
        }
        out
    }

    fn byte_array_dump(id: u64, length: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(0x23);
        put_id(&mut out, id);
        put_u32(&mut out, 0);
        put_u32(&mut out, length);
        out.push(8); // byte
        out.extend(std::iter::repeat(0u8).take(length as usize));
        out
    }

    fn root_unknown(id: u64) -> Vec<u8> {
        let mut out = vec![0xFF];
        put_id(&mut out, id);
        out
    }

    fn sticky_class(id: u64) -> Vec<u8> {
        let mut out = vec![0x05];
        put_id(&mut out, id);
        out
    }

    fn synthetic_dump() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"JAVA PROFILE 1.0.2\0");
        put_u32(&mut out, 8);
        put_u64(&mut out, 1_700_000_000_000);

        for (id, text) in [
            (S_OBJECT, "java/lang/Object"),
            (S_REFERENCE, "java/lang/ref/Reference"),
            (S_WEAK_REFERENCE, "java/lang/ref/WeakReference"),
            (S_DEMO, "com/example/Demo"),
            (S_REFERENT, "referent"),
            (S_NEXT, "next"),
        ] {
            out.extend(string_record(id, text));
        }
        for (class_id, name_id) in [
            (C_OBJECT, S_OBJECT),
            (C_REFERENCE, S_REFERENCE),
            (C_WEAK_REFERENCE, S_WEAK_REFERENCE),
            (C_DEMO, S_DEMO),
        ] {
            out.extend(load_class_record(class_id, name_id));
        }

        let mut heap = Vec::new();
        heap.extend(class_dump(C_OBJECT, 0, 0, &[]));
        heap.extend(class_dump(
            C_REFERENCE,
            C_OBJECT,
            8,
            &[(S_REFERENT, 2)],
        ));
        heap.extend(class_dump(C_WEAK_REFERENCE, C_REFERENCE, 8, &[]));
        heap.extend(class_dump(C_DEMO, C_OBJECT, 8, &[(S_NEXT, 2)]));

        for class_id in [C_OBJECT, C_REFERENCE, C_WEAK_REFERENCE, C_DEMO] {
            heap.extend(sticky_class(class_id));
        }

        heap.extend(root_unknown(O_A));
        heap.extend(instance_dump(O_A, C_DEMO, &[Some(O_B)]));
        heap.extend(instance_dump(O_B, C_DEMO, &[Some(O_C)]));
        heap.extend(instance_dump(O_C, C_DEMO, &[Some(O_BYTES)]));
        heap.extend(byte_array_dump(O_BYTES, 128));

        heap.extend(root_unknown(O_WEAK_REF));
        heap.extend(instance_dump(
            O_WEAK_REF,
            C_WEAK_REFERENCE,
            &[Some(O_WEAK_TARGET)],
        ));
        heap.extend(instance_dump(O_WEAK_TARGET, C_DEMO, &[None]));

        out.extend(record(0x0C, &heap));
        out
    }

    fn temp_dump_path(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("jcc-hprof-{label}-{}-{unique}.hprof", std::process::id()))
    }

    #[test]
    fn analyzes_synthetic_dump_with_weak_reference_and_dominators() {
        let path = temp_dump_path("synthetic");
        fs::write(&path, synthetic_dump()).expect("write synthetic dump");
        let analysis = analyze_heap_dump(&path).expect("analyse synthetic dump");
        let _ = fs::remove_file(&path);

        assert_eq!(analysis.total_objects, 10);
        assert_eq!(analysis.reachable_objects, 9);
        assert_eq!(analysis.weak_objects, 1);
        assert_eq!(analysis.total_shallow_bytes, 456);
        assert_eq!(analysis.reachable_bytes, 432);
        assert_eq!(analysis.weak_bytes, 24);
        assert_eq!(analysis.class_count, 4);
        assert!(analysis.dominators_computed);

        let a = analysis
            .dominators
            .iter()
            .find(|row| row.object_id == "200")
            .expect("A in dominator list");
        assert_eq!(a.class_name, "com.example.Demo");
        assert_eq!(a.shallow_bytes, 24);
        assert_eq!(a.retained_bytes, Some(216));

        let weak_ref = analysis
            .top_objects
            .iter()
            .find(|row| row.object_id == "204")
            .expect("weak reference in top objects");
        assert_eq!(weak_ref.retained_bytes, Some(24));

        assert!(analysis
            .weak_classes
            .iter()
            .any(|row| row.class_name == "com.example.Demo" && row.instances == 1));
        assert!(analysis
            .classes
            .iter()
            .any(|row| row.class_name == "com.example.Demo" && row.instances == 4));
        assert!(analysis
            .root_counts
            .iter()
            .any(|row| row.kind == "Sticky class" && row.count == 4));
    }

    #[test]
    fn rejects_non_hprof_input() {
        let path = temp_dump_path("invalid");
        fs::write(&path, b"this is not a heap dump").expect("write invalid dump");
        let error = analyze_heap_dump(&path).expect_err("invalid dump must fail");
        let _ = fs::remove_file(&path);
        assert!(error.to_string().contains("not a valid HPROF dump"));
    }
}
