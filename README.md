# rawbson

[![version](https://img.shields.io/crates/v/rawbson.svg)](https://crates.io/crates/rawbson)
[![license](https://img.shields.io/crates/l/rawbson.svg)](https://crates.io/crates/rawbson)

`rawbson` provides zero-copy manipulation of BSON data.

## Usage

A rawbson document can be created from a `Vec<u8>` containing raw BSON data, and elements
accessed via methods similar to those in the [bson-rust](https://crates.io/crate/bson-rust)
crate.  Note that rawbson returns a Result<Option<T>>, since the bytes contained in the
document are not fully validated until trying to access the contained data.

```rust
use rawbson::{
    DocBuf,
    elem,
};

// \x16\x00\x00\x00                   // total document size
// \x02                               // 0x02 = type String
// hello\x00                          // field name
// \x06\x00\x00\x00world\x00          // field value
// \x00

let doc = DocBuf::new(b"\x16\x00\x00\x00\x02hello\x00\x06\x00\x00\x00world\x00\x00".to_vec())?;
let elem: Option<elem::Element> = doc.get("hello")?;
assert_eq!(
    elem.unwrap().as_str()?,
    "world",
);
# Ok::<(), rawbson::RawError>(())
```

### bson-rust interop

This crate is designed to interoperate smoothly with the bson crate.

A [`DocBuf`] can be created from a [`bson::document::Document`].  Internally, this
serializes the `Document` to a `Vec<u8>`, and then includes those bytes in the [`DocBuf`].

```rust
use bson::doc;
use rawbson::{
    DocBuf,
};

let document = doc!{"goodbye": {"cruel": "world"}};
let raw = DocBuf::from_document(&document);
let value: Option<&str> = raw.get_document("goodbye")?
    .map(|docref| docref.get_str("cruel"))
    .transpose()?
    .flatten();

assert_eq!(
    value,
    Some("world"),
);
# Ok::<(), rawbson::RawError>(())
```

### Reference types

A BSON document can also be accessed with the [`Doc`] reference type,
which is an unsized type that represents the BSON payload as a `[u8]`.
This allows accessing nested documents without reallocation.  [Doc]
must always be accessed via a pointer type, similarly to `[T]` and `str`.

This type will coexist with the now deprecated [DocRef] type for at
least one minor release.

The below example constructs a bson document in a stack-based array,
and extracts a &str from it, performing no heap allocation.

```rust
use rawbson::Doc;

let bytes = b"\x13\x00\x00\x00\x02hi\x00\x06\x00\x00\x00y'all\x00\x00";
assert_eq!(Doc::new(bytes)?.get_str("hi")?, Some("y'all"));
# Ok::<(), rawbson::RawError>(())
```

### Iteration

[`Doc`] implements [`IntoIterator`](std::iter::IntoIterator), which can also
be accessed via [`DocBuf::iter`], or the deprecated [`DocRef::into_iter`]

```rust
use bson::doc;
use rawbson::{DocBuf, elem::Element};

let doc = DocBuf::from_document(&doc! {"crate": "rawbson", "license": "MIT"});
let mut dociter = doc.iter();

let (key, value): (&str, Element) = dociter.next().unwrap()?;
assert_eq!(key, "crate");
assert_eq!(value.as_str()?, "rawbson");

let (key, value): (&str, Element) = dociter.next().unwrap()?;
assert_eq!(key, "license");
assert_eq!(value.as_str()?, "MIT");
# Ok::<(), rawbson::RawError>(())
```

### serde support

There is also serde deserialization support.

Serde serialization support (with rawbson as the target output) is not yet 
provided.  For now, use [`bson::to_document`] instead, and then serialize it 
out using [`bson::Document::to_writer`] or [`DocBuf::from_document`].

Serialization from a rawbson `DocBuf` to `Vec<u8>` is trivially done via 
the `into_inner()` method.

```rust
use serde::Deserialize;
use bson::{doc, Document, oid::ObjectId, DateTime};
use rawbson::{DocBuf, de::from_docbuf};

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct User {
    #[serde(rename = "_id")]
    id: ObjectId,
    first_name: String,
    last_name: String,
    birthdate: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(flatten)]
    extra: Document,
}

let doc = DocBuf::from_document(&doc!{
    "_id": ObjectId::with_string("543254325432543254325432")?,
    "firstName": "John",
    "lastName": "Doe",
    "birthdate": null,
    "luckyNumbers": [3, 60, 2147483647],
    "nickname": "Red",
});

let user: User = from_docbuf(&doc)?;
assert_eq!(user.id.to_hex(), "543254325432543254325432");
assert_eq!(user.first_name, "John");
assert_eq!(user.last_name, "Doe");
assert_eq!(user.extra.get_str("nickname")?, "Red");
assert!(user.birthdate.is_none());
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Performance

*TODO:* Replace this section with more rigorous analysis of the benchmarks.

Because rawbson doesn't have to parse a BSON payload or allocate space for each
element within the document, accessing individual elements within a document
and iterating over the elements in order are much faster operations than with
the [`bson::Document`] type.

Deserializing raw bytes to custom types is also significantly faster than using the
deserialization methods provided in the bson crate, since those all deserialize first
to the parsed Bson type.

On the other hand, since finding a particular key requires traversing the document
from the beginning, creating a parsed [`bson::Document`], which has O(1) element access
becomes faster when repeatedly accessing random elements within the document.

This crate provides a [criterion](https://github.com/bheisler/criterion.rs/) benchmark
suite to support these assertions.  The output of running those benchmarks on my
Thinkpad X1 Carbon (Gen 5) can be found in the ./criterion-report directory.

Suggestions for improving the quality of these benchmarks is appreciated.
