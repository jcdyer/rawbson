use bson::doc;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rawbson::{Doc, DocBuf};
use std::convert::TryInto;
use std::io::{Cursor, Read};

fn construct_deep_doc(depth: usize) -> bson::Document {
    let mut doc = doc! {"value": 23i64};
    for _ in 0..depth {
        doc = doc! {"value": doc};
    }
    doc
}

fn construct_broad_doc(size: usize) -> bson::Document {
    let mut doc = bson::Document::new();
    for i in 0..size {
        doc.insert(format!("key {}", i), "lorem ipsum");
    }
    doc
}

/// Measure the time to access an int64 value in a document, nested in N
/// levels of documents.
///
/// This benchmark starts from a Vec<u8> of bytes in bson format, and
/// then times the following steps:
/// 1.  Constructing the type (bson::Bson, for parsed,
///     rawbson::DocumentBuf for raw).
/// 2.  Unwrapping the layers of the document in a while let loop
/// 3.  Accessing the element, converting it to an i64 and uwrapping
///     the result.
fn access_deep_from_bytes(c: &mut Criterion) {
    let mut group = c.benchmark_group("access-deep-from-bytes");
    for depth in &[10, 100, 1000] {
        let depth = *depth;
        let inbytes = {
            let doc = construct_deep_doc(depth);
            let mut bytes = Vec::new();
            doc.to_writer(&mut bytes).unwrap();
            bytes
        };
        group.bench_with_input(BenchmarkId::new("raw", depth), &inbytes, |b, inbytes| {
            b.iter(|| {
                let mut reader = Cursor::new(inbytes);
                let mut bytes = Vec::new();
                reader.read_to_end(&mut bytes).unwrap();
                let rawdocbuf = DocBuf::new(bytes).expect("invalid document");
                let mut rawdoc = rawdocbuf.as_ref();
                while let Ok(Some(val)) = rawdoc.get_document("value") {
                    rawdoc = val;
                }
                rawdoc.get_i64("value").unwrap();
            })
        });
        group.bench_with_input(BenchmarkId::new("parsed", depth), &inbytes, |b, inbytes| {
            b.iter(|| {
                let mut reader = Cursor::new(inbytes);
                let doc = bson::Document::from_reader(&mut reader).unwrap();
                let mut doc = &doc;
                while let Ok(val) = doc.get_document("value") {
                    doc = val;
                }
                doc.get_i64("value").unwrap();
            })
        });
    }
    group.finish();
}

/// Measure the time to access an int64 value in a document, nested in N
/// levels of documents.
///
/// This benchmark starts from an object of the appropriate type (bson::Bson,
/// for parsed, rawbson::DocumentBuf for raw), and then times the following
/// steps:
/// 1.  Unwrapping the layers of the document in a while let loop.
/// 2.  Accessing the element, converting it to an i64 and uwrapping the
///     result.
fn access_deep_from_type(c: &mut Criterion) {
    let mut group = c.benchmark_group("access-deep-from-type");
    for depth in &[10, 100, 1000] {
        let depth = *depth;
        let inbytes = {
            let doc = construct_deep_doc(depth);
            let mut bytes = Vec::new();
            doc.to_writer(&mut bytes).unwrap();
            bytes
        };
        group.bench_with_input(BenchmarkId::new("raw", depth), &inbytes, |b, inbytes| {
            let bytes = inbytes.clone();
            let rawdocbuf = DocBuf::new(bytes).expect("invalid document");
            b.iter(|| {
                let mut rawdoc = rawdocbuf.as_ref();
                while let Ok(Some(val)) = rawdoc.get_document("value") {
                    rawdoc = val;
                }
                rawdoc.get_i64("value").unwrap();
            })
        });
        group.bench_with_input(BenchmarkId::new("parsed", depth), &inbytes, |b, inbytes| {
            let mut reader = Cursor::new(inbytes);
            let doc = bson::Document::from_reader(&mut reader).unwrap();
            b.iter(|| {
                let mut doc = &doc;
                while let Ok(val) = doc.get_document("value") {
                    doc = val;
                }
                doc.get_i64("value").unwrap();
            })
        });
    }
    group.finish();
}

/// Measure the time to access string values in a document with a large
/// number of keys.
///
/// In this benchmark, we construct a flat document of 1000 keys, and
/// adjust the number of elements to fetch from the document.  We always
/// fetch the last N elements, which are the least performant for a
/// rawbson::DocumentBuf since we have to iterate through the document
/// to find the relevant keys.
///
/// This benchmark starts from a Vec<u8> of bytes in bson format, and
/// a list of keys to fetch, and then times the following steps:
/// 1.  Constructing the type (bson::Bson, for parsed,
///     rawbson::DocumentBuf for raw).
/// 2.  Looping over the requested N keys
/// 3.  For each key, accessing the value, converting it to a string
///     and unwrapping the result.
fn access_broad_from_bytes(c: &mut Criterion) {
    const SIZE: usize = 1000;
    let mut group = c.benchmark_group("access-broad-from-bytes");
    let inbytes: Vec<u8> = {
        let doc = construct_broad_doc(SIZE);
        let mut bytes = Vec::new();
        doc.to_writer(&mut bytes).unwrap();
        bytes
    };
    let inbytes = &inbytes;
    for count in &[1, 10, 20, 30, 40, 50] {
        let count = *count;
        let keys_to_get: Vec<_> = ((SIZE - count)..SIZE)
            .map(|i| format!("key {}", i))
            .collect();
        group.bench_with_input(
            BenchmarkId::new("raw", count),
            &keys_to_get,
            |b, keys_to_get| {
                b.iter(|| {
                    let mut reader = Cursor::new(inbytes);
                    let mut bytes = Vec::new();
                    reader.read_to_end(&mut bytes).unwrap();
                    let rawdoc = DocBuf::new(bytes).expect("invalid document");
                    for key in keys_to_get {
                        rawdoc.get_str(&key).unwrap();
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("parsed", count),
            &keys_to_get,
            |b, keys_to_get| {
                b.iter(|| {
                    let mut reader = Cursor::new(inbytes);
                    let doc = bson::Document::from_reader(&mut reader).unwrap();
                    for key in keys_to_get {
                        doc.get_str(&key).unwrap();
                    }
                });
            },
        );
    }
    group.finish();
}

/// Measure the time to access string values in a document with a large
/// number of keys.
///
/// In this benchmark, we construct a flat document of 1000 keys, and
/// adjust the number of elements to fetch from the document.  We always
/// fetch the last N elements, which are the least performant for a
/// rawbson::DocumentBuf since we have to iterate through the document
/// to find the relevant keys.
///
/// This benchmark starts from an object of the appropriate type (bson::Bson,
/// for parsed, rawbson::DocumentBuf for raw), and a list of keys to fetch,
/// and then times the following steps:
/// 1.  Looping over the requested N keys
/// 2.  For each key, accessing the value, converting it to a string
///     and unwrapping the result.
fn access_broad_from_type(c: &mut Criterion) {
    const SIZE: usize = 1000;
    let mut group = c.benchmark_group("access-broad-from-type");
    let inbytes: Vec<u8> = {
        let doc = construct_broad_doc(SIZE);
        let mut bytes = Vec::new();
        doc.to_writer(&mut bytes).unwrap();
        bytes
    };
    let inbytes = &inbytes;
    for count in &[1, 10, 20, 30, 40, 50] {
        let count = *count;
        let keys_to_get: Vec<_> = ((SIZE - count)..SIZE)
            .map(|i| format!("key {}", i))
            .collect();
        group.bench_with_input(
            BenchmarkId::new("raw", count),
            &keys_to_get,
            |b, keys_to_get| {
                let rawdoc = DocBuf::new(inbytes.clone()).expect("invalid document");
                b.iter(|| {
                    for key in keys_to_get {
                        rawdoc.get_str(&key).unwrap();
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("parsed", count),
            &keys_to_get,
            |b, keys_to_get| {
                let mut reader = Cursor::new(inbytes);
                let doc = bson::Document::from_reader(&mut reader).unwrap();

                b.iter(|| {
                    for key in keys_to_get {
                        doc.get_str(&key).unwrap();
                    }
                });
            },
        );
    }
    group.finish();
}

/// Measure the time to iterate over string values in a document with
/// a large number of keys.
///
/// In this benchmark, we construct a flat document of 1000 keys, and
/// iterate over the entire document, converting each value to a string.
///
/// This benchmark starts from a Vec<u8> of bytes in bson format and
/// then times the following steps:
/// 1.  Constructing the type (bson::Bson, for parsed,
///     rawbson::DocumentBuf for raw).
/// 2.  Iterating through the entire document with a for-loop.
/// 3.  Unwrapping each value
/// 3.  For each key, accessing the value, converting it to a string
///     and unwrapping the result.
fn iter_broad_from_bytes(c: &mut Criterion) {
    const SIZE: usize = 1000;
    let mut group = c.benchmark_group("iter-broad-from-bytes");
    let inbytes: Vec<u8> = {
        let doc = construct_broad_doc(SIZE);
        let mut bytes = Vec::new();
        doc.to_writer(&mut bytes).unwrap();
        bytes
    };
    let inbytes = &inbytes;
    const EXPECTEDSIZE: usize = 11000;
    group.bench_function("raw", |b| {
        b.iter(|| {
            let mut rawsize = 0;
            let mut reader = Cursor::new(inbytes);
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).unwrap();
            let rawdoc = DocBuf::new(bytes).expect("invalid document");
            for result in &rawdoc {
                if let Ok((key, value)) = result {
                    if let Ok(s) = value.as_str() {
                        rawsize += s.len();
                    } else {
                        eprintln!("raw error in {} {:?}", key, value.element_type());
                    }
                } else if let Err(err) = result {
                    eprintln!("raw error in {:?}", err);
                }
            }

            assert_eq!(rawsize, EXPECTEDSIZE);
        })
    });
    group.bench_function("parsed", |b| {
        b.iter(|| {
            let mut parsedsize = 0;
            let mut reader = Cursor::new(&inbytes);
            let doc = bson::Document::from_reader(&mut reader).unwrap();
            for (_key, value) in doc {
                if let Some(s) = value.as_str() {
                    parsedsize += s.len();
                } else {
                    eprintln!("parsed error in {:?}", value);
                }
            }
            assert_eq!(parsedsize, EXPECTEDSIZE);
        })
    });
    group.finish();
}

/// Measure the time to iterate over string values in a document with
/// a large number of keys.
///
/// In this benchmark, we construct a flat document of 1000 keys, and
/// iterate over the entire document, converting each value to a string.
///
/// This benchmark starts from an object of the appropriate type (bson::Bson,
/// for parsed, rawbson::DocumentBuf for raw), and then times the following
/// steps.
/// 1.  Iterating through the entire document with a for-loop.
/// 2.  Unwrapping each value
/// 3.  For each key, accessing the value, converting it to a string
///     and unwrapping the result.
fn iter_broad_from_type(c: &mut Criterion) {
    const SIZE: usize = 1000;
    let mut group = c.benchmark_group("iter-broad-from-type");
    let inbytes: Vec<u8> = {
        let doc = construct_broad_doc(SIZE);
        let mut bytes = Vec::new();
        doc.to_writer(&mut bytes).unwrap();
        bytes
    };
    let inbytes = &inbytes;
    let mut rawsize = 0;
    let mut parsedsize = 0;
    group.bench_function("raw", |b| {
        let rawdoc = DocBuf::new(inbytes.clone()).expect("invalid document");
        b.iter(|| {
            for result in &rawdoc {
                let (_key, value) = result.expect("invalid bson");
                if let Ok(s) = value.as_str() {
                    rawsize += s.len();
                }
            }
        });
    });
    group.bench_function("parsed", |b| {
        let mut reader = Cursor::new(inbytes);
        let doc = bson::Document::from_reader(&mut reader).unwrap();
        let doc = &doc;
        b.iter(|| {
            for (_key, value) in doc {
                if let Some(s) = value.as_str() {
                    parsedsize += s.len();
                }
            }
        })
    });
    group.finish();
}

/// Measure the time to deserialize a struct with a single field from document with
/// a large number of keys.
///
/// In this benchmark, we construct a flat document of 1000 keys, and
/// iterate over the entire document, converting each value to a string.
///
/// This benchmark starts from a Vec<u8> of bytes in bson format and
/// then times the following steps:
/// 1.  Constructing the type (bson::Bson, for parsed,
///     rawbson::DocumentBuf for raw).
/// 2.  Deserializing to a struct.
/// 3.  Unwrapping each value
/// 4.  Accessing the field in the struct to verify its contents.
fn deserialize_broad_from_bytes(c: &mut Criterion) {
    const SIZE: usize = 1000;

    #[derive(serde::Deserialize, PartialEq, Eq, Debug)]
    struct Target {
        #[serde(rename = "key 999")]
        key: String,
    }

    let expected = Target {
        key: String::from("lorem ipsum"),
    };
    let mut group = c.benchmark_group("deserialize-broad-from-bytes");
    let inbytes: Vec<u8> = {
        let doc = construct_broad_doc(SIZE);
        let mut bytes = Vec::new();
        doc.to_writer(&mut bytes).unwrap();
        bytes
    };
    let inbytes = &inbytes;
    group.bench_function("raw", |b| {
        b.iter(|| {
            let rawdoc = Doc::new(inbytes).expect("invalid document");
            let t: Target = rawbson::de::from_doc(rawdoc).unwrap();
            assert_eq!(&t, &expected);
        })
    });
    group.bench_function("parsed", |b| {
        b.iter(|| {
            let mut reader = Cursor::new(&inbytes);
            let doc = bson::Document::from_reader(&mut reader).unwrap();
            let t: Target = bson::from_document(doc).unwrap();
            assert_eq!(&t, &expected)
        })
    });
    group.finish();
}

/// Measure the time to deserialize a struct with a single field from document with
/// a large number of keys.
///
/// In this benchmark, we construct a flat document of 1000 keys, and
/// iterate over the entire document, converting each value to a string.
///
/// This benchmark starts from a rawbson::DocBuf or bson::Document and
/// then times the following steps:
/// 1.  Constructing the type (bson::Bson, for parsed,
///     rawbson::DocumentBuf for raw).
/// 2.  Deserializing to a struct.
/// 3.  Unwrapping each value
/// 4.  Accessing the field in the struct to verify its contents.
fn deserialize_broad_from_type(c: &mut Criterion) {
    const SIZE: usize = 1000;

    #[derive(serde::Deserialize, PartialEq, Eq, Debug)]
    struct Target {
        #[serde(rename = "key 999")]
        key: String,
    }

    let expected = Target {
        key: String::from("lorem ipsum"),
    };
    let mut group = c.benchmark_group("deserialize-broad-from-type");
    let inbytes: Vec<u8> = {
        let doc = construct_broad_doc(SIZE);
        let mut bytes = Vec::new();
        doc.to_writer(&mut bytes).unwrap();
        bytes
    };
    let inbytes = &inbytes;
    group.bench_function("raw", |b| {
        let rawdoc = DocBuf::new(inbytes.clone()).unwrap();
        b.iter(|| {
            let t: Target = rawbson::de::from_doc(&rawdoc).unwrap();
            assert_eq!(&t, &expected);
        })
    });
    group.bench_function("parsed", |b| {
        let mut reader = Cursor::new(inbytes);
        let doc = bson::Document::from_reader(&mut reader).unwrap();
        b.iter_with_setup(
            // clone is required here, since from_document takes ownership.
            || doc.clone(),
            |doc| {
                let t: Target = bson::from_document(doc).unwrap();
                assert_eq!(&t, &expected)
            },
        )
    });
    group.finish();
}

fn construct_bson_deep(c: &mut Criterion) {
    const SIZE: usize = 1000;
    let mut group = c.benchmark_group("construct-bson-deep");
    let inbytes: Vec<u8> = {
        let doc = construct_deep_doc(SIZE);
        let mut bytes = Vec::new();
        doc.to_writer(&mut bytes).unwrap();
        bytes
    };
    let inbytes = &inbytes;
    group.bench_function("direct", |b| {
        b.iter(|| {
            let mut reader = Cursor::new(&inbytes);
            let _doc = bson::Document::from_reader(&mut reader).unwrap();
        })
    });
    group.bench_function("via-raw", |b| {
        b.iter(|| {
            let mut reader = Cursor::new(inbytes);
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).unwrap();
            let rawdoc = DocBuf::new(bytes).expect("invalid document");
            let _: bson::Document = rawdoc.try_into().expect("could not convert document");
        })
    });
    group.finish();
}

fn construct_bson_broad(c: &mut Criterion) {
    const SIZE: usize = 1000;
    let mut group = c.benchmark_group("construct-bson-broad");
    let inbytes: Vec<u8> = {
        let doc = construct_broad_doc(SIZE);
        let mut bytes = Vec::new();
        doc.to_writer(&mut bytes).unwrap();
        bytes
    };
    let inbytes = &inbytes;
    group.bench_function("direct", |b| {
        b.iter(|| {
            let mut reader = Cursor::new(&inbytes);
            let _doc = bson::Document::from_reader(&mut reader).unwrap();
        })
    });
    group.bench_function("via-raw", |b| {
        b.iter(|| {
            let mut reader = Cursor::new(inbytes);
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).unwrap();
            let rawdoc = DocBuf::new(bytes).expect("invalid document");
            let _doc: bson::Document = rawdoc.try_into().expect("invalid document");
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    access_deep_from_bytes,
    access_deep_from_type,
    access_broad_from_bytes,
    access_broad_from_type,
    iter_broad_from_bytes,
    iter_broad_from_type,
    construct_bson_deep,
    construct_bson_broad,
    deserialize_broad_from_bytes,
    deserialize_broad_from_type,
);

criterion_main!(benches);
