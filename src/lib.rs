use std::convert::{TryFrom, TryInto};

use chrono::{DateTime, Utc};

use bson::{decimal128::Decimal128, document::ValueAccessError, oid, spec::ElementType, Bson};

pub mod de;
pub mod elem;

#[cfg(test)]
mod props;

/// Error to indicate that either a value was empty or it contained an unexpected
/// type, for use with the direct getters.
#[derive(Debug, PartialEq)]
pub enum RawError {
    /// Found a Bson value with the specified key, but not with the expected type
    UnexpectedType,

    /// The found value was not well-formed
    MalformedValue(String),

    /// Found a value where a utf-8 string was expected, but it was not valid
    /// utf-8.  The error value contains the malformed data as a string.
    Utf8EncodingError(Vec<u8>),
}

type RawResult<T> = Result<T, RawError>;
type OptResult<T> = RawResult<Option<T>>;

impl<'a> From<RawError> for ValueAccessError {
    fn from(src: RawError) -> ValueAccessError {
        match src {
            RawError::UnexpectedType => ValueAccessError::UnexpectedType,
            RawError::MalformedValue(_) => ValueAccessError::UnexpectedType,
            RawError::Utf8EncodingError(_) => ValueAccessError::UnexpectedType,
        }
    }
}

impl<'a> From<ValueAccessError> for RawError {
    fn from(src: ValueAccessError) -> RawError {
        match src {
            ValueAccessError::NotPresent => unreachable!("This should be converted to an Option"),
            ValueAccessError::UnexpectedType => RawError::UnexpectedType,
            _ => RawError::UnexpectedType,
        }
    }
}

#[derive(Clone)]
pub struct DocBuf {
    data: Vec<u8>,
}

impl DocBuf {
    pub fn as_docref(&self) -> DocRef<'_> {
        let &DocBuf { ref data } = self;
        DocRef { data }
    }

    pub fn new(data: Vec<u8>) -> RawResult<DocBuf> {
        if data.len() < 5 {
            return Err(RawError::MalformedValue("document too short".into()));
        }
        let length = i32_from_slice(&data[..4]);
        if data.len() as i32 != length {
            return Err(RawError::MalformedValue("document length incorrect".into()));
        }
        if data[data.len() - 1] != 0 {
            return Err(RawError::MalformedValue(
                "document not null-terminated".into(),
            ));
        }
        Ok(unsafe { DocBuf::new_unchecked(data) })
    }

    pub fn from_document(doc: &bson::Document) -> DocBuf {
        let mut data = Vec::new();
        doc.to_writer(&mut data).unwrap();
        unsafe { DocBuf::new_unchecked(data) }
    }

    /// Create a DocumentBuf from an owned Vec<u8>.
    ///
    /// # Safety
    ///
    /// The provided bytes must be a valid bson document
    pub unsafe fn new_unchecked(data: Vec<u8>) -> DocBuf {
        DocBuf { data }
    }

    pub fn get<'a>(&'a self, key: &str) -> OptResult<elem::Element<'a>> {
        self.as_docref().get(key)
    }

    pub fn get_f64(&self, key: &str) -> OptResult<f64> {
        self.as_docref().get_f64(key)
    }

    pub fn get_str<'a>(&'a self, key: &str) -> OptResult<&'a str> {
        self.as_docref().get_str(key)
    }

    pub fn get_document<'a>(&'a self, key: &str) -> OptResult<DocRef<'a>> {
        self.as_docref().get_document(key)
    }

    pub fn get_array<'a>(&'a self, key: &str) -> OptResult<ArrayRef<'a>> {
        self.as_docref().get_array(key)
    }

    pub fn get_binary<'a>(&'a self, key: &str) -> OptResult<elem::RawBsonBinary<'a>> {
        self.as_docref().get_binary(key)
    }

    pub fn get_object_id(&self, key: &str) -> OptResult<oid::ObjectId> {
        self.as_docref().get_object_id(key)
    }

    pub fn get_bool(&self, key: &str) -> OptResult<bool> {
        self.as_docref().get_bool(key)
    }

    pub fn get_datetime(&self, key: &str) -> OptResult<DateTime<Utc>> {
        self.as_docref().get_datetime(key)
    }

    pub fn get_null(&self, key: &str) -> OptResult<()> {
        self.as_docref().get_null(key)
    }

    pub fn get_regex<'a>(&'a self, key: &str) -> OptResult<elem::RawBsonRegex<'a>> {
        self.as_docref().get_regex(key)
    }

    pub fn get_javascript<'a>(&'a self, key: &str) -> OptResult<&'a str> {
        self.as_docref().get_javascript(key)
    }

    pub fn get_symbol<'a>(&'a self, key: &str) -> OptResult<&'a str> {
        self.as_docref().get_symbol(key)
    }

    pub fn get_javascript_with_scope<'a>(&'a self, key: &str) -> OptResult<(&'a str, DocRef<'a>)> {
        self.as_docref().get_javascript_with_scope(key)
    }

    pub fn get_i32(&self, key: &str) -> OptResult<i32> {
        self.as_docref().get_i32(key)
    }

    pub fn get_timestamp<'a>(&'a self, key: &str) -> OptResult<elem::RawBsonTimestamp<'a>> {
        self.as_docref().get_timestamp(key)
    }

    pub fn get_i64(&self, key: &str) -> OptResult<i64> {
        self.as_docref().get_i64(key)
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.data
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

impl TryFrom<DocBuf> for bson::Document {
    type Error = RawError;

    fn try_from(rawdoc: DocBuf) -> RawResult<bson::Document> {
        bson::Document::try_from(rawdoc.as_docref())
    }
}

impl<'a> IntoIterator for &'a DocBuf {
    type IntoIter = DocIter<'a>;
    type Item = RawResult<(&'a str, elem::Element<'a>)>;

    fn into_iter(self) -> DocIter<'a> {
        DocIter {
            doc: self.as_docref(),
            offset: 4,
        }
    }
}

#[derive(Clone, Copy)]
pub struct DocRef<'a> {
    data: &'a [u8],
}

impl<'a> DocRef<'a> {
    pub fn new(data: &'a [u8]) -> RawResult<DocRef<'a>> {
        if data.len() < 5 {
            return Err(RawError::MalformedValue("document too short".into()));
        }
        let length = i32_from_slice(&data[..4]);
        if data.len() as i32 != length {
            return Err(RawError::MalformedValue("document length incorrect".into()));
        }
        if data[data.len() - 1] != 0 {
            return Err(RawError::MalformedValue(
                "document not null-terminated".into(),
            ));
        }
        Ok(DocRef::new_unchecked(data))
    }

    pub fn new_unchecked(data: &'a [u8]) -> DocRef<'a> {
        DocRef { data }
    }

    pub fn get(self, key: &str) -> OptResult<elem::Element<'a>> {
        for result in self.into_iter() {
            let (thiskey, bson) = result?;
            if thiskey == key {
                return Ok(Some(bson));
            }
        }
        Ok(None)
    }

    fn get_with<T>(self, key: &str, f: impl FnOnce(elem::Element<'a>) -> RawResult<T>) -> OptResult<T> {
        self.get(key)?.map(f).transpose()
    }

    pub fn get_f64(self, key: &str) -> OptResult<f64> {
        self.get_with(key, elem::Element::as_f64)
    }

    pub fn get_str(self, key: &str) -> OptResult<&'a str> {
        self.get_with(key, elem::Element::as_str)
    }

    pub fn get_document(self, key: &str) -> OptResult<DocRef<'a>> {
        self.get_with(key, elem::Element::as_document)
    }

    pub fn get_array(self, key: &str) -> OptResult<ArrayRef<'a>> {
        self.get_with(key, elem::Element::as_array)
    }

    pub fn get_binary(self, key: &str) -> OptResult<elem::RawBsonBinary<'a>> {
        self.get_with(key, elem::Element::as_binary)
    }

    pub fn get_object_id(self, key: &str) -> OptResult<oid::ObjectId> {
        self.get_with(key, elem::Element::as_object_id)
    }

    pub fn get_bool(self, key: &str) -> OptResult<bool> {
        self.get_with(key, elem::Element::as_bool)
    }

    pub fn get_datetime(self, key: &str) -> OptResult<DateTime<Utc>> {
        self.get_with(key, elem::Element::as_datetime)
    }

    pub fn get_null(self, key: &str) -> OptResult<()> {
        self.get_with(key, elem::Element::as_null)
    }

    pub fn get_regex(self, key: &str) -> OptResult<elem::RawBsonRegex<'a>> {
        self.get_with(key, elem::Element::as_regex)
    }

    pub fn get_javascript(self, key: &str) -> OptResult<&'a str> {
        self.get_with(key, elem::Element::as_javascript)
    }

    pub fn get_symbol(self, key: &str) -> OptResult<&'a str> {
        self.get_with(key, elem::Element::as_symbol)
    }

    pub fn get_javascript_with_scope(self, key: &str) -> OptResult<(&'a str, DocRef<'a>)> {
        self.get_with(key, elem::Element::as_javascript_with_scope)
    }

    pub fn get_i32(self, key: &str) -> OptResult<i32> {
        self.get_with(key, elem::Element::as_i32)
    }

    pub fn get_timestamp(self, key: &str) -> OptResult<elem::RawBsonTimestamp<'a>> {
        self.get_with(key, elem::Element::as_timestamp)
    }

    pub fn get_i64(self, key: &str) -> OptResult<i64> {
        self.get_with(key, elem::Element::as_i64)
    }

    pub fn as_bytes(self) -> &'a [u8] {
        self.data
    }
}

impl<'a> TryFrom<DocRef<'a>> for bson::Document {
    type Error = RawError;

    fn try_from(rawdoc: DocRef<'a>) -> RawResult<bson::Document> {
        rawdoc
            .into_iter()
            .map(|res| res.and_then(|(k, v)| Ok((k.to_owned(), v.try_into()?))))
            .collect()
    }
}

impl<'a> IntoIterator for DocRef<'a> {
    type IntoIter = DocIter<'a>;
    type Item = RawResult<(&'a str, elem::Element<'a>)>;

    fn into_iter(self) -> DocIter<'a> {
        DocIter {
            doc: self,
            offset: 4,
        }
    }
}

pub struct DocIter<'a> {
    doc: DocRef<'a>,
    offset: usize,
}

impl<'a> Iterator for DocIter<'a> {
    type Item = RawResult<(&'a str, elem::Element<'a>)>;

    fn next(&mut self) -> Option<RawResult<(&'a str, elem::Element<'a>)>> {
        if self.offset == self.doc.data.len() - 1 {
            if self.doc.data[self.offset] == 0 {
                // end of document marker
                return None;
            } else {
                return Some(Err(RawError::MalformedValue(
                    "document not null terminated".into(),
                )));
            }
        }
        let key = match read_nullterminated(&self.doc.data[self.offset + 1..]) {
            Ok(key) => key,
            Err(err) => return Some(Err(err)),
        };
        let valueoffset = self.offset + 1 + key.len() + 1; // type specifier + key + \0
        let element_type = match ElementType::from(self.doc.data[self.offset]) {
            Some(et) => et,
            None => {
                return Some(Err(RawError::MalformedValue(format!(
                    "invalid tag: {}",
                    self.doc.data[self.offset]
                ))))
            }
        };
        let element_size = match element_type {
            ElementType::Double => 8,
            ElementType::String => {
                let size =
                    4 + i32_from_slice(&self.doc.data[valueoffset..valueoffset + 4]) as usize;
                if self.doc.data[valueoffset + size - 1] != 0 {
                    return Some(Err(RawError::MalformedValue(
                        "string not null terminated".into(),
                    )));
                }
                size
            }
            ElementType::EmbeddedDocument => {
                let size = i32_from_slice(&self.doc.data[valueoffset..valueoffset + 4]) as usize;
                if self.doc.data[valueoffset + size - 1] != 0 {
                    return Some(Err(RawError::MalformedValue(
                        "document not null terminated".into(),
                    )));
                }
                size
            }
            ElementType::Array => {
                let size = i32_from_slice(&self.doc.data[valueoffset..valueoffset + 4]) as usize;
                if self.doc.data[valueoffset + size - 1] != 0 {
                    return Some(Err(RawError::MalformedValue(
                        "array not null terminated".into(),
                    )));
                }
                size
            }
            ElementType::Binary => {
                5 + i32_from_slice(&self.doc.data[valueoffset..valueoffset + 4]) as usize
            }
            ElementType::Undefined => 0,
            ElementType::ObjectId => 12,
            ElementType::Boolean => 1,
            ElementType::DateTime => 8,
            ElementType::Null => 0,
            ElementType::RegularExpression => {
                let regex = match read_nullterminated(&self.doc.data[valueoffset..]) {
                    Ok(regex) => regex,
                    Err(err) => return Some(Err(err)),
                };
                let options =
                    match read_nullterminated(&self.doc.data[valueoffset + regex.len() + 1..]) {
                        Ok(options) => options,
                        Err(err) => return Some(Err(err)),
                    };
                regex.len() + options.len() + 2
            }
            ElementType::DbPointer => {
                let string_size =
                    4 + i32_from_slice(&self.doc.data[valueoffset..valueoffset + 4]) as usize;
                let id_size = 12;
                if self.doc.data[valueoffset + string_size - 1] != 0 {
                    return Some(Err(RawError::MalformedValue(
                        "DBPointer string not null-terminated".into(),
                    )));
                }
                string_size + id_size
            }
            ElementType::JavaScriptCode => {
                let size =
                    4 + i32_from_slice(&self.doc.data[valueoffset..valueoffset + 4]) as usize;
                if self.doc.data[valueoffset + size - 1] != 0 {
                    return Some(Err(RawError::MalformedValue(
                        "javascript code not null-terminated".into(),
                    )));
                }
                size
            }
            ElementType::Symbol => {
                4 + i32_from_slice(&self.doc.data[valueoffset..valueoffset + 4]) as usize
            }
            ElementType::JavaScriptCodeWithScope => {
                let size = i32_from_slice(&self.doc.data[valueoffset..valueoffset + 4]) as usize;
                if self.doc.data[valueoffset + size - 1] != 0 {
                    return Some(Err(RawError::MalformedValue(
                        "javascript with scope not null-terminated".into(),
                    )));
                }
                size
            }
            ElementType::Int32 => 4,
            ElementType::Timestamp => 8,
            ElementType::Int64 => 8,
            ElementType::Decimal128 => 16,
            ElementType::MaxKey => 0,
            ElementType::MinKey => 0,
        };
        let nextoffset = valueoffset + element_size;
        self.offset = nextoffset;
        Some(Ok((
            key,
            elem::Element::new(element_type, &self.doc.data[valueoffset..nextoffset]),
        )))
    }
}

#[derive(Clone, Copy)]
pub struct ArrayRef<'a> {
    doc: DocRef<'a>,
}

impl<'a> ArrayRef<'a> {
    pub fn new(data: &'a [u8]) -> RawResult<ArrayRef<'a>> {
        Ok(ArrayRef::from_doc(DocRef::new(data)?))
    }

    pub fn from_doc(doc: DocRef<'a>) -> ArrayRef<'a> {
        ArrayRef { doc }
    }

    pub fn get(self, index: usize) -> OptResult<elem::Element<'a>> {
        self.into_iter().nth(index).transpose()
    }

    fn get_with<T>(
        self,
        index: usize,
        f: impl FnOnce(elem::Element<'a>) -> RawResult<T>,
    ) -> OptResult<T> {
        self.get(index)?.map(f).transpose()
    }

    pub fn get_f64(self, index: usize) -> OptResult<f64> {
        self.get_with(index, elem::Element::as_f64)
    }

    pub fn get_str(self, index: usize) -> OptResult<&'a str> {
        self.get_with(index, elem::Element::as_str)
    }

    pub fn get_document(self, index: usize) -> OptResult<DocRef<'a>> {
        self.get_with(index, elem::Element::as_document)
    }

    pub fn get_array(self, index: usize) -> OptResult<ArrayRef<'a>> {
        self.get_with(index, elem::Element::as_array)
    }

    pub fn get_binary(self, index: usize) -> OptResult<elem::RawBsonBinary<'a>> {
        self.get_with(index, elem::Element::as_binary)
    }

    pub fn get_object_id(self, index: usize) -> OptResult<oid::ObjectId> {
        self.get_with(index, elem::Element::as_object_id)
    }

    pub fn get_bool(self, index: usize) -> OptResult<bool> {
        self.get_with(index, elem::Element::as_bool)
    }

    pub fn get_datetime(self, index: usize) -> OptResult<DateTime<Utc>> {
        self.get_with(index, elem::Element::as_datetime)
    }

    pub fn get_null(self, index: usize) -> OptResult<()> {
        self.get_with(index, elem::Element::as_null)
    }

    pub fn get_regex(self, index: usize) -> OptResult<elem::RawBsonRegex<'a>> {
        self.get_with(index, elem::Element::as_regex)
    }

    pub fn get_javascript(self, index: usize) -> OptResult<&'a str> {
        self.get_with(index, elem::Element::as_javascript)
    }

    pub fn get_symbol(self, index: usize) -> OptResult<&'a str> {
        self.get_with(index, elem::Element::as_symbol)
    }

    pub fn get_javascript_with_scope(self, index: usize) -> OptResult<(&'a str, DocRef<'a>)> {
        self.get_with(index, elem::Element::as_javascript_with_scope)
    }

    pub fn get_i32(self, index: usize) -> OptResult<i32> {
        self.get_with(index, elem::Element::as_i32)
    }

    pub fn get_timestamp(self, index: usize) -> OptResult<elem::RawBsonTimestamp<'a>> {
        self.get_with(index, elem::Element::as_timestamp)
    }

    pub fn get_i64(self, index: usize) -> OptResult<i64> {
        self.get_with(index, elem::Element::as_i64)
    }

    pub fn to_vec(self) -> RawResult<Vec<elem::Element<'a>>> {
        self.into_iter().collect()
    }

    pub fn as_bytes(self) -> &'a [u8] {
        self.doc.as_bytes()
    }
}

impl<'a> TryFrom<ArrayRef<'a>> for Vec<Bson> {
    type Error = RawError;

    fn try_from(arr: ArrayRef<'a>) -> RawResult<Vec<Bson>> {
        arr.into_iter()
            .map(|result| {
                let rawbson = result?;
                Bson::try_from(rawbson)
            })
            .collect()
    }
}

impl<'a> IntoIterator for ArrayRef<'a> {
    type IntoIter = ArrayIter<'a>;
    type Item = RawResult<elem::Element<'a>>;

    fn into_iter(self) -> ArrayIter<'a> {
        ArrayIter {
            dociter: self.doc.into_iter(),
            index: 0,
        }
    }
}

pub struct ArrayIter<'a> {
    dociter: DocIter<'a>,
    index: usize,
}

impl<'a> Iterator for ArrayIter<'a> {
    type Item = RawResult<elem::Element<'a>>;

    fn next(&mut self) -> Option<RawResult<elem::Element<'a>>> {
        let value = self.dociter.next().map(|result| {
            let (key, bson) = match result {
                Ok(value) => value,
                Err(err) => return Err(err),
            };

            let index: usize = key
                .parse()
                .map_err(|_| RawError::MalformedValue("non-integer array index found".into()))?;

            if index == self.index {
                Ok(bson)
            } else {
                Err(RawError::MalformedValue("wrong array index found".into()))
            }
        });
        self.index += 1;
        value
    }
}
/// Given a 4 byte u8 slice, return an i32 calculated from the bytes in
/// little endian order
///
/// # Panics
///
/// This function panics if given a slice that is not four bytes long.
fn i32_from_slice(val: &[u8]) -> i32 {
    i32::from_le_bytes(val.try_into().expect("i32 is four bytes"))
}

/// Given an 8 byte u8 slice, return an i64 calculated from the bytes in
/// little endian order
///
/// # Panics
///
/// This function panics if given a slice that is not eight bytes long.
fn i64_from_slice(val: &[u8]) -> i64 {
    i64::from_le_bytes(val.try_into().expect("i64 is eight bytes"))
}

/// Given a 4 byte u8 slice, return a u32 calculated from the bytes in
/// little endian order
///
/// # Panics
///
/// This function panics if given a slice that is not four bytes long.
fn u32_from_slice(val: &[u8]) -> u32 {
    u32::from_le_bytes(val.try_into().expect("u32 is four bytes"))
}

fn d128_from_slice(val: &[u8]) -> Decimal128 {
    // TODO: Handle Big Endian platforms
    let d =
        unsafe { decimal::d128::from_raw_bytes(val.try_into().expect("d128 is sixteen bytes")) };
    Decimal128::from(d)
}

fn read_nullterminated(buf: &[u8]) -> RawResult<&str> {
    let mut splits = buf.splitn(2, |x| *x == 0);
    let value = splits
        .next()
        .ok_or_else(|| RawError::MalformedValue("no value".into()))?;
    if splits.next().is_some() {
        Ok(try_to_str(value)?)
    } else {
        Err(RawError::MalformedValue("expected null terminator".into()))
    }
}

fn read_lenencoded(buf: &[u8]) -> RawResult<&str> {
    let length = i32_from_slice(&buf[..4]);
    assert!(buf.len() as i32 >= length + 4);
    try_to_str(&buf[4..4 + length as usize - 1])
}

fn try_to_str(data: &[u8]) -> RawResult<&str> {
    match std::str::from_utf8(data) {
        Ok(s) => Ok(s),
        Err(_) => Err(RawError::Utf8EncodingError(data.into())),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use bson::{Binary, Bson, JavaScriptCodeWithScope, Regex, Timestamp, doc, spec::BinarySubtype};
    use chrono::TimeZone;

    fn to_bytes(doc: &bson::Document) -> Vec<u8> {
        let mut docbytes = Vec::new();
        doc.to_writer(&mut docbytes).unwrap();
        docbytes
    }

    #[test]
    fn string_from_document() {
        let docbytes = to_bytes(&doc! {
            "this": "first",
            "that": "second",
            "something": "else",
        });
        let rawdoc = DocRef::new(&docbytes).unwrap();
        assert_eq!(
            rawdoc.get("that").unwrap().unwrap().as_str().unwrap(),
            "second",
        );
    }

    #[test]
    fn nested_document() {
        let docbytes = to_bytes(&doc! {
            "outer": {
                "inner": "surprise",
            },
        });
        let rawdoc = DocRef::new(&docbytes).unwrap();
        assert_eq!(
            rawdoc
                .get("outer")
                .expect("get doc result")
                .expect("get doc option")
                .as_document()
                .expect("as doc")
                .get("inner")
                .expect("get str result")
                .expect("get str option")
                .as_str()
                .expect("as str"),
            "surprise",
        );
    }

    #[test]
    fn iterate() {
        let docbytes = to_bytes(&doc! {
            "apples": "oranges",
            "peanut butter": "chocolate",
            "easy as": {"do": 1, "re": 2, "mi": 3},
        });
        let rawdoc = DocRef::new(&docbytes).expect("malformed bson document");
        let mut dociter = rawdoc.into_iter();
        let next = dociter.next().expect("no result").expect("invalid bson");
        assert_eq!(next.0, "apples");
        assert_eq!(next.1.as_str().expect("result was not a str"), "oranges");
        let next = dociter.next().expect("no result").expect("invalid bson");
        assert_eq!(next.0, "peanut butter");
        assert_eq!(next.1.as_str().expect("result was not a str"), "chocolate");
        let next = dociter.next().expect("no result").expect("invalid bson");
        assert_eq!(next.0, "easy as");
        let _doc = next.1.as_document().expect("result was a not a document");
        let next = dociter.next();
        assert!(next.is_none());
    }

    #[test]
    fn rawdoc_to_doc() {
        let docbytes = to_bytes(&doc! {
            "f64": 2.5,
            "string": "hello",
            "document": {},
            "array": ["binary", "serialized", "object", "notation"],
            "binary": Binary { subtype: BinarySubtype::Generic, bytes: vec![1u8, 2, 3] },
            "object_id": oid::ObjectId::with_bytes([1, 2, 3, 4, 5,6,7,8,9,10, 11,12]),
            "boolean": true,
            "datetime": Utc::now(),
            "null": Bson::Null,
            "regex": Bson::RegularExpression(Regex { pattern: String::from(r"end\s*$"), options: String::from("i")}),
            "javascript": Bson::JavaScriptCode(String::from("console.log(console);")),
            "symbol": Bson::Symbol(String::from("artist-formerly-known-as")),
            "javascript_with_scope": Bson::JavaScriptCodeWithScope(JavaScriptCodeWithScope{ code: String::from("console.log(msg);"), scope: doc!{"ok": true}}),
            "int32": 23i32,
            "timestamp": Bson::Timestamp(Timestamp { time: 3542578, increment: 0 }),
            "int64": 46i64,
            "end": "END",
        });

        let rawdoc = DocRef::new_unchecked(&docbytes);
        let _doc: bson::Document = rawdoc.try_into().expect("invalid bson");
    }

    #[test]
    fn f64() {
        #![allow(clippy::float_cmp)]

        let rawdoc = DocBuf::from_document(&doc! {"f64": 2.5});
        assert_eq!(
            rawdoc
                .get("f64")
                .expect("error finding key f64")
                .expect("no key f64")
                .as_f64()
                .expect("result was not a f64"),
            2.5,
        );
    }

    #[test]
    fn string() {
        let rawdoc = DocBuf::from_document(&doc! {"string": "hello"});

        assert_eq!(
            rawdoc
                .get("string")
                .expect("error finding key string")
                .expect("no key string")
                .as_str()
                .expect("result was not a string"),
            "hello",
        );
    }
    #[test]
    fn document() {
        let rawdoc = DocBuf::from_document(&doc! {"document": {}});

        let doc = rawdoc
            .get("document")
            .expect("error finding key document")
            .expect("no key document")
            .as_document()
            .expect("result was not a document");
        assert_eq!(doc.data, &[5, 0, 0, 0, 0]); // Empty document
    }

    #[test]
    fn array() {
        let rawdoc =
            DocBuf::from_document(&doc! { "array": ["binary", "serialized", "object", "notation"]});

        let array: ArrayRef<'_> = rawdoc
            .get("array")
            .expect("error finding key array")
            .expect("no key array")
            .as_array()
            .expect("result was not an array");
        assert_eq!(array.get_str(0), Ok(Some("binary")));
        assert_eq!(array.get_str(3), Ok(Some("notation")));
        assert_eq!(array.get_str(4), Ok(None));
    }

    #[test]
    fn binary() {
        let rawdoc = DocBuf::from_document(&doc! {
            "binary": Binary { subtype: BinarySubtype::Generic, bytes: vec![1u8, 2, 3] }
        });
        let binary: elem::RawBsonBinary<'_> = rawdoc
            .get("binary")
            .expect("error finding key binary")
            .expect("no key binary")
            .as_binary()
            .expect("result was not a binary object");
        assert_eq!(binary.subtype, BinarySubtype::Generic);
        assert_eq!(binary.data, &[1, 2, 3]);
    }

    #[test]
    fn object_id() {
        let rawdoc = DocBuf::from_document(&doc! {
            "object_id": oid::ObjectId::with_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]),
        });
        let oid = rawdoc
            .get("object_id")
            .expect("error finding key object_id")
            .expect("no key object_id")
            .as_object_id()
            .expect("result was not an object id");
        assert_eq!(oid.to_hex(), "0102030405060708090a0b0c");
    }

    #[test]
    fn boolean() {
        let rawdoc = DocBuf::from_document(&doc! {
            "boolean": true,
        });

        let boolean = rawdoc
            .get("boolean")
            .expect("error finding key boolean")
            .expect("no key boolean")
            .as_bool()
            .expect("result was not boolean");

        assert_eq!(boolean, true);
    }

    #[test]
    fn datetime() {
        let rawdoc = DocBuf::from_document(&doc! {
            "boolean": true,
            "datetime": Utc.ymd(2000,10,31).and_hms(12, 30, 45),
        });
        let datetime = rawdoc
            .get("datetime")
            .expect("error finding key datetime")
            .expect("no key datetime")
            .as_datetime()
            .expect("result was not datetime");
        assert_eq!(datetime.to_rfc3339(), "2000-10-31T12:30:45+00:00");
    }

    #[test]
    fn null() {
        let rawdoc = DocBuf::from_document(&doc! {
            "null": null,
        });
        let () = rawdoc
            .get("null")
            .expect("error finding key null")
            .expect("no key null")
            .as_null()
            .expect("was not null");
    }

    #[test]
    fn regex() {
        let rawdoc = DocBuf::from_document(&doc! {
            "regex": Bson::RegularExpression(Regex { pattern: String::from(r"end\s*$"), options: String::from("i")}),
        });
        let regex = rawdoc
            .get("regex")
            .expect("error finding key regex")
            .expect("no key regex")
            .as_regex()
            .expect("was not regex");
        assert_eq!(regex.pattern, r"end\s*$");
        assert_eq!(regex.options, "i");
    }
    #[test]
    fn javascript() {
        let rawdoc = DocBuf::from_document(&doc! {
            "javascript": Bson::JavaScriptCode(String::from("console.log(console);")),
        });
        let js = rawdoc
            .get("javascript")
            .expect("error finding key javascript")
            .expect("no key javascript")
            .as_javascript()
            .expect("was not javascript");
        assert_eq!(js, "console.log(console);");
    }

    #[test]
    fn symbol() {
        let rawdoc = DocBuf::from_document(&doc! {
            "symbol": Bson::Symbol(String::from("artist-formerly-known-as")),
        });

        let symbol = rawdoc
            .get("symbol")
            .expect("error finding key symbol")
            .expect("no key symbol")
            .as_symbol()
            .expect("was not symbol");
        assert_eq!(symbol, "artist-formerly-known-as");
    }

    #[test]
    fn javascript_with_scope() {
        let rawdoc = DocBuf::from_document(&doc! {
            "javascript_with_scope": Bson::JavaScriptCodeWithScope(JavaScriptCodeWithScope{ code: String::from("console.log(msg);"), scope: doc!{"ok": true}}),
        });
        let (js, scopedoc) = rawdoc
            .get("javascript_with_scope")
            .expect("error finding key javascript_with_scope")
            .expect("no key javascript_with_scope")
            .as_javascript_with_scope()
            .expect("was not javascript with scope");
        assert_eq!(js, "console.log(msg);");
        let (scope_key, scope_value_bson) = scopedoc
            .into_iter()
            .next()
            .expect("no next value in scope")
            .expect("invalid element");
        assert_eq!(scope_key, "ok");
        let scope_value = scope_value_bson.as_bool().expect("not a boolean");
        assert_eq!(scope_value, true);
    }

    #[test]
    fn int32() {
        let rawdoc = DocBuf::from_document(&doc! {
            "int32": 23i32,
        });
        let int32 = rawdoc
            .get("int32")
            .expect("error finding key int32")
            .expect("no key int32")
            .as_i32()
            .expect("was not int32");
        assert_eq!(int32, 23i32);
    }

    #[test]
    fn timestamp() {
        let rawdoc = DocBuf::from_document(&doc! {
            "timestamp": Bson::Timestamp(Timestamp { time: 3542578, increment: 0 }),
        });
        let ts = rawdoc
            .get("timestamp")
            .expect("error finding key timestamp")
            .expect("no key timestamp")
            .as_timestamp()
            .expect("was not a timestamp");

        assert_eq!(ts.increment().expect("timestamp has invalid increment"), 0);
        assert_eq!(ts.time().expect("timestamp has invalid time"), 3542578);
    }

    #[test]
    fn int64() {
        let rawdoc = DocBuf::from_document(&doc! {
            "int64": 46i64,
        });
        let int64 = rawdoc
            .get("int64")
            .expect("error finding key int64")
            .expect("no key int64")
            .as_i64()
            .expect("was not int64");
        assert_eq!(int64, 46i64);
    }
    #[test]
    fn document_iteration() {
        let docbytes = to_bytes(&doc! {
            "f64": 2.5,
            "string": "hello",
            "document": {},
            "array": ["binary", "serialized", "object", "notation"],
            "binary": Binary { subtype: BinarySubtype::Generic, bytes: vec![1u8, 2, 3] },
            "object_id": oid::ObjectId::with_bytes([1, 2, 3, 4, 5,6,7,8,9,10, 11,12]),
            "boolean": true,
            "datetime": Utc::now(),
            "null": Bson::Null,
            "regex": Bson::RegularExpression(Regex { pattern: String::from(r"end\s*$"), options: String::from("i")}),
            "javascript": Bson::JavaScriptCode(String::from("console.log(console);")),
            "symbol": Bson::Symbol(String::from("artist-formerly-known-as")),
            "javascript_with_scope": Bson::JavaScriptCodeWithScope(JavaScriptCodeWithScope{ code: String::from("console.log(msg);"), scope: doc!{"ok": true}}),
            "int32": 23i32,
            "timestamp": Bson::Timestamp(Timestamp { time: 3542578, increment: 0 }),
            "int64": 46i64,
            "end": "END",
        });
        let rawdoc = DocRef::new_unchecked(&docbytes);

        assert_eq!(
            rawdoc
                .into_iter()
                .collect::<Result<Vec<(&str, _)>, RawError>>()
                .expect("collecting iterated doc")
                .len(),
            17
        );
        let end = rawdoc
            .get("end")
            .expect("error finding key end")
            .expect("no key end")
            .as_str()
            .expect("was not str");
        assert_eq!(end, "END");
    }

    #[test]
    fn into_bson_conversion() {
        let docbytes = to_bytes(&doc! {
            "f64": 2.5,
            "string": "hello",
            "document": {},
            "array": ["binary", "serialized", "object", "notation"],
            "object_id": oid::ObjectId::with_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]),
            "binary": Binary { subtype: BinarySubtype::Generic, bytes: vec![1u8, 2, 3] },
            "boolean": false,
        });
        let rawbson = elem::Element::new(ElementType::EmbeddedDocument, &docbytes);
        let b: Bson = rawbson.try_into().expect("invalid bson");
        let doc = b.as_document().expect("not a document");
        assert_eq!(*doc.get("f64").expect("f64 not found"), Bson::Double(2.5));
        assert_eq!(
            *doc.get("string").expect("string not found"),
            Bson::String(String::from("hello"))
        );
        assert_eq!(
            *doc.get("document").expect("document not found"),
            Bson::Document(doc! {})
        );
        assert_eq!(
            *doc.get("array").expect("array not found"),
            Bson::Array(
                vec!["binary", "serialized", "object", "notation"]
                    .into_iter()
                    .map(|s| Bson::String(String::from(s)))
                    .collect()
            )
        );
        assert_eq!(
            *doc.get("object_id").expect("object_id not found"),
            Bson::ObjectId(oid::ObjectId::with_bytes([
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12
            ]))
        );
        assert_eq!(
            *doc.get("binary").expect("binary not found"),
            Bson::Binary(Binary {
                subtype: BinarySubtype::Generic,
                bytes: vec![1, 2, 3]
            })
        );
        assert_eq!(
            *doc.get("boolean").expect("boolean not found"),
            Bson::Boolean(false)
        );
    }
}

#[cfg(test)]
mod proptests {
    use proptest::prelude::*;
    use std::convert::TryInto;

    use super::DocBuf;
    use crate::props::arbitrary_bson;
    use bson::doc;

    fn to_bytes(doc: &bson::Document) -> Vec<u8> {
        let mut docbytes = Vec::new();
        doc.to_writer(&mut docbytes).unwrap();
        docbytes
    }

    proptest! {
        #[test]
        fn no_crashes(s: Vec<u8>) {
            let _ = DocBuf::new(s);
        }

        #[test]
        fn roundtrip_bson(bson in arbitrary_bson()) {
            println!("{:?}", bson);
            let doc = doc!{"bson": bson};
            let raw = to_bytes(&doc);
            let raw = DocBuf::new(raw);
            prop_assert!(raw.is_ok());
            let raw = raw.unwrap();
            let roundtrip: Result<bson::Document, _> = raw.try_into();
            prop_assert!(roundtrip.is_ok());
            let roundtrip = roundtrip.unwrap();
            prop_assert_eq!(doc, roundtrip);
        }
    }
}
