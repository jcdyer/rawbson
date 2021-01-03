use std::convert::{TryFrom, TryInto};
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};

use bson::{
    document::ValueAccessError,
    oid,
    spec::{BinarySubtype, ElementType},
    Bson, JavaScriptCodeWithScope, Regex, Timestamp,
};

use bson::decimal128::Decimal128;

mod props;

/// Error to indicate that either a value was empty or it contained an unexpected
/// type, for use with the direct getters.
#[derive(Debug, PartialEq)]
pub enum RawError {
    /// Cannot find a field with the specified key
    NotPresent,

    /// Found a Bson value with the specified key, but not with the expected type
    UnexpectedType,

    /// The found value was not well-formed
    MalformedValue(String),

    /// Found a value where a utf-8 string was expected, but it was not valid
    /// utf-8.  The error value contains the malformed data as a string.
    Utf8EncodingError(Vec<u8>),
}

type RawResult<T> = Result<T, RawError>;

impl<'a> From<RawError> for ValueAccessError {
    fn from(src: RawError) -> ValueAccessError {
        match src {
            RawError::NotPresent => ValueAccessError::NotPresent,
            RawError::UnexpectedType => ValueAccessError::UnexpectedType,
            RawError::MalformedValue(_) => ValueAccessError::UnexpectedType,
            RawError::Utf8EncodingError(_) => ValueAccessError::UnexpectedType,
        }
    }
}

impl<'a> From<ValueAccessError> for RawError {
    fn from(src: ValueAccessError) -> RawError {
        match src {
            ValueAccessError::NotPresent => RawError::NotPresent,
            ValueAccessError::UnexpectedType => RawError::UnexpectedType,
            _ => RawError::UnexpectedType,
        }
    }
}

#[derive(Clone)]
pub struct RawBsonDocBuf {
    data: Vec<u8>,
}

impl RawBsonDocBuf {
    pub fn as_ref(&self) -> RawBsonDoc<'_> {
        let &RawBsonDocBuf { ref data } = self;
        RawBsonDoc { data }
    }

    pub fn new(data: Vec<u8>) -> RawResult<RawBsonDocBuf> {
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
        Ok(RawBsonDocBuf::new_unchecked(data))
    }

    pub fn new_unchecked(data: Vec<u8>) -> RawBsonDocBuf {
        RawBsonDocBuf { data }
    }

    pub fn get<'a>(&'a self, key: &str) -> RawResult<RawBson<'a>> {
        self.as_ref().get(key)
    }

    pub fn get_f64(&self, key: &str) -> RawResult<f64> {
        self.as_ref().get_f64(key)
    }

    pub fn get_str<'a>(&'a self, key: &str) -> RawResult<&'a str> {
        self.as_ref().get_str(key)
    }

    pub fn get_document<'a>(&'a self, key: &str) -> RawResult<RawBsonDoc<'a>> {
        self.as_ref().get_document(key)
    }

    pub fn get_array<'a>(&'a self, key: &str) -> RawResult<RawBsonArray<'a>> {
        self.as_ref().get_array(key)
    }

    pub fn get_binary<'a>(&'a self, key: &str) -> RawResult<RawBsonBinary<'a>> {
        self.as_ref().get_binary(key)
    }

    pub fn get_object_id(&self, key: &str) -> RawResult<oid::ObjectId> {
        self.as_ref().get_object_id(key)
    }

    pub fn get_bool(&self, key: &str) -> RawResult<bool> {
        self.as_ref().get_bool(key)
    }

    pub fn get_utc_date_time(&self, key: &str) -> RawResult<DateTime<Utc>> {
        self.as_ref().get_utc_date_time(key)
    }

    pub fn get_null(&self, key: &str) -> RawResult<()> {
        self.as_ref().get_null(key)
    }

    pub fn get_regexp<'a>(&'a self, key: &str) -> RawResult<RawBsonRegexp<'a>> {
        self.as_ref().get_regexp(key)
    }

    pub fn get_javascript<'a>(&'a self, key: &str) -> RawResult<&'a str> {
        self.as_ref().get_javascript(key)
    }

    pub fn get_symbol<'a>(&'a self, key: &str) -> RawResult<&'a str> {
        self.as_ref().get_symbol(key)
    }

    pub fn get_javascript_with_scope<'a>(
        &'a self,
        key: &str,
    ) -> RawResult<(&'a str, RawBsonDoc<'a>)> {
        self.as_ref().get_javascript_with_scope(key)
    }

    pub fn get_i32(&self, key: &str) -> RawResult<i32> {
        self.as_ref().get_i32(key)
    }

    pub fn get_timestamp<'a>(&'a self, key: &str) -> RawResult<RawBsonTimestamp<'a>> {
        self.as_ref().get_timestamp(key)
    }

    pub fn get_i64(&self, key: &str) -> RawResult<i64> {
        self.as_ref().get_i64(key)
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.data
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

impl TryFrom<RawBsonDocBuf> for bson::Document {
    type Error = RawError;

    fn try_from(rawdoc: RawBsonDocBuf) -> RawResult<bson::Document> {
        bson::Document::try_from(rawdoc.as_ref())
    }
}

impl<'a> IntoIterator for &'a RawBsonDocBuf {
    type IntoIter = RawBsonDocIterator<'a>;
    type Item = RawResult<(&'a str, RawBson<'a>)>;

    fn into_iter(self) -> RawBsonDocIterator<'a> {
        RawBsonDocIterator {
            doc: self.as_ref(),
            offset: 4,
        }
    }
}

#[derive(Clone, Copy)]
pub struct RawBsonDoc<'a> {
    data: &'a [u8],
}

impl<'a> RawBsonDoc<'a> {
    pub fn new(data: &'a [u8]) -> RawResult<RawBsonDoc<'a>> {
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
        Ok(RawBsonDoc::new_unchecked(data))
    }

    pub fn new_unchecked(data: &'a [u8]) -> RawBsonDoc<'a> {
        RawBsonDoc { data }
    }

    pub fn get(self, key: &str) -> RawResult<RawBson<'a>> {
        for result in self.into_iter() {
            let (thiskey, bson) = result?;
            if thiskey == key {
                return Ok(bson);
            }
        }
        Err(RawError::NotPresent)
    }

    pub fn get_f64(self, key: &str) -> RawResult<f64> {
        self.get(key)?.as_f64()
    }

    pub fn get_str(self, key: &str) -> RawResult<&'a str> {
        self.get(key)?.as_str()
    }

    pub fn get_document(self, key: &str) -> RawResult<RawBsonDoc<'a>> {
        self.get(key)?.as_document()
    }

    pub fn get_array(self, key: &str) -> RawResult<RawBsonArray<'a>> {
        self.get(key)?.as_array()
    }

    pub fn get_binary(self, key: &str) -> RawResult<RawBsonBinary<'a>> {
        self.get(key)?.as_binary()
    }

    pub fn get_object_id(self, key: &str) -> RawResult<oid::ObjectId> {
        self.get(key)?.as_object_id()
    }

    pub fn get_bool(self, key: &str) -> RawResult<bool> {
        self.get(key)?.as_bool()
    }

    pub fn get_utc_date_time(self, key: &str) -> RawResult<DateTime<Utc>> {
        self.get(key)?.as_utc_date_time()
    }

    pub fn get_null(self, key: &str) -> RawResult<()> {
        self.get(key)?.as_null()
    }

    pub fn get_regexp(self, key: &str) -> RawResult<RawBsonRegexp<'a>> {
        self.get(key)?.as_regexp()
    }

    pub fn get_javascript(self, key: &str) -> RawResult<&'a str> {
        self.get(key)?.as_javascript()
    }

    pub fn get_symbol(self, key: &str) -> RawResult<&'a str> {
        self.get(key)?.as_symbol()
    }

    pub fn get_javascript_with_scope(self, key: &str) -> RawResult<(&'a str, RawBsonDoc<'a>)> {
        self.get(key)?.as_javascript_with_scope()
    }

    pub fn get_i32(self, key: &str) -> RawResult<i32> {
        self.get(key)?.as_i32()
    }

    pub fn get_timestamp(self, key: &str) -> RawResult<RawBsonTimestamp<'a>> {
        self.get(key)?.as_timestamp()
    }

    pub fn get_i64(self, key: &str) -> RawResult<i64> {
        self.get(key)?.as_i64()
    }

    pub fn as_bytes(self) -> &'a [u8] {
        self.data
    }
}

impl<'a> TryFrom<RawBsonDoc<'a>> for bson::Document {
    type Error = RawError;

    fn try_from(rawdoc: RawBsonDoc<'a>) -> RawResult<bson::Document> {
        rawdoc
            .into_iter()
            .map(|res| res.and_then(|(k, v)| Ok((k.to_owned(), v.try_into()?))))
            .collect()
    }
}

impl<'a> IntoIterator for RawBsonDoc<'a> {
    type IntoIter = RawBsonDocIterator<'a>;
    type Item = RawResult<(&'a str, RawBson<'a>)>;

    fn into_iter(self) -> RawBsonDocIterator<'a> {
        RawBsonDocIterator {
            doc: self,
            offset: 4,
        }
    }
}

pub struct RawBsonDocIterator<'a> {
    doc: RawBsonDoc<'a>,
    offset: usize,
}

impl<'a> Iterator for RawBsonDocIterator<'a> {
    type Item = RawResult<(&'a str, RawBson<'a>)>;

    fn next(&mut self) -> Option<RawResult<(&'a str, RawBson<'a>)>> {
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
            RawBson::new(element_type, &self.doc.data[valueoffset..nextoffset]),
        )))
    }
}

#[derive(Clone, Copy)]
pub struct RawBsonArray<'a> {
    doc: RawBsonDoc<'a>,
}

impl<'a> RawBsonArray<'a> {
    pub fn new(data: &'a [u8]) -> RawResult<RawBsonArray<'a>> {
        Ok(RawBsonArray::from_doc(RawBsonDoc::new(data)?))
    }

    pub fn from_doc(doc: RawBsonDoc<'a>) -> RawBsonArray<'a> {
        RawBsonArray { doc }
    }

    pub fn get(self, index: usize) -> RawResult<RawBson<'a>> {
        self.into_iter().nth(index).ok_or(RawError::NotPresent)?
    }

    pub fn get_f64(self, index: usize) -> RawResult<f64> {
        self.get(index)?.as_f64()
    }

    pub fn get_str(self, index: usize) -> RawResult<&'a str> {
        self.get(index)?.as_str()
    }

    pub fn get_document(self, index: usize) -> RawResult<RawBsonDoc<'a>> {
        self.get(index)?.as_document()
    }

    pub fn get_array(self, index: usize) -> RawResult<RawBsonArray<'a>> {
        self.get(index)?.as_array()
    }

    pub fn get_binary(self, index: usize) -> RawResult<RawBsonBinary<'a>> {
        self.get(index)?.as_binary()
    }

    pub fn get_object_id(self, index: usize) -> RawResult<oid::ObjectId> {
        self.get(index)?.as_object_id()
    }

    pub fn get_bool(self, index: usize) -> RawResult<bool> {
        self.get(index)?.as_bool()
    }

    pub fn get_utc_date_time(self, index: usize) -> RawResult<DateTime<Utc>> {
        self.get(index)?.as_utc_date_time()
    }

    pub fn get_null(self, index: usize) -> RawResult<()> {
        self.get(index)?.as_null()
    }

    pub fn get_regexp(self, index: usize) -> RawResult<RawBsonRegexp<'a>> {
        self.get(index)?.as_regexp()
    }

    pub fn get_javascript(self, index: usize) -> RawResult<&'a str> {
        self.get(index)?.as_javascript()
    }

    pub fn get_symbol(self, index: usize) -> RawResult<&'a str> {
        self.get(index)?.as_symbol()
    }

    pub fn get_javascript_with_scope(self, index: usize) -> RawResult<(&'a str, RawBsonDoc<'a>)> {
        self.get(index)?.as_javascript_with_scope()
    }

    pub fn get_i32(self, index: usize) -> RawResult<i32> {
        self.get(index)?.as_i32()
    }

    pub fn get_timestamp(self, index: usize) -> RawResult<RawBsonTimestamp<'a>> {
        self.get(index)?.as_timestamp()
    }

    pub fn get_i64(self, index: usize) -> RawResult<i64> {
        self.get(index)?.as_i64()
    }

    pub fn to_vec(self) -> RawResult<Vec<RawBson<'a>>> {
        self.into_iter().collect()
    }

    pub fn as_bytes(self) -> &'a [u8] {
        self.doc.as_bytes()
    }
}

impl<'a> TryFrom<RawBsonArray<'a>> for Vec<Bson> {
    type Error = RawError;

    fn try_from(arr: RawBsonArray<'a>) -> RawResult<Vec<Bson>> {
        arr.into_iter()
            .map(|result| {
                let rawbson = result?;
                Bson::try_from(rawbson)
            })
            .collect()
    }
}

impl<'a> IntoIterator for RawBsonArray<'a> {
    type IntoIter = RawBsonArrayIterator<'a>;
    type Item = RawResult<RawBson<'a>>;

    fn into_iter(self) -> RawBsonArrayIterator<'a> {
        RawBsonArrayIterator {
            dociter: self.doc.into_iter(),
            index: 0,
        }
    }
}

pub struct RawBsonArrayIterator<'a> {
    dociter: RawBsonDocIterator<'a>,
    index: usize,
}

impl<'a> Iterator for RawBsonArrayIterator<'a> {
    type Item = RawResult<RawBson<'a>>;

    fn next(&mut self) -> Option<RawResult<RawBson<'a>>> {
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

#[derive(Clone, Copy)]
pub struct RawBsonBinary<'a> {
    subtype: BinarySubtype,
    data: &'a [u8],
}

impl<'a> RawBsonBinary<'a> {
    pub fn new(subtype: BinarySubtype, data: &'a [u8]) -> RawBsonBinary<'a> {
        RawBsonBinary { subtype, data }
    }

    pub fn subtype(self) -> BinarySubtype {
        self.subtype
    }

    pub fn as_bytes(self) -> &'a [u8] {
        self.data
    }
}

#[derive(Clone, Copy)]
pub struct RawBsonRegexp<'a> {
    pattern: &'a str,
    options: &'a str,
}

impl<'a> RawBsonRegexp<'a> {
    pub fn new(data: &'a [u8]) -> RawResult<RawBsonRegexp<'a>> {
        let pattern = read_nullterminated(data)?;
        let opts = read_nullterminated(&data[pattern.len() + 1..])?;
        if pattern.len() + opts.len() == data.len() - 2 {
            Ok(RawBsonRegexp {
                pattern,
                options: opts,
            })
        } else {
            Err(RawError::MalformedValue(
                "expected two null-terminated strings".into(),
            ))
        }
    }

    pub fn pattern(self) -> &'a str {
        self.pattern
    }

    pub fn options(self) -> &'a str {
        self.options
    }
}

#[derive(Clone, Copy)]
pub struct RawBsonTimestamp<'a> {
    data: &'a [u8],
}

impl<'a> RawBsonTimestamp<'a> {
    pub fn increment(&self) -> RawResult<u32> {
        self.data
            .get(0..4)
            .map(u32_from_slice)
            .ok_or_else(|| RawError::MalformedValue("wrong length timestamp".into()))
    }

    pub fn time(&self) -> RawResult<u32> {
        self.data
            .get(4..8)
            .map(u32_from_slice)
            .ok_or_else(|| RawError::MalformedValue("wrong length timestamp".into()))
    }
}

#[derive(Clone, Copy)]
pub struct RawBson<'a> {
    element_type: ElementType,
    data: &'a [u8],
}

impl<'a> RawBson<'a> {
    // This is not public.  A RawBson object can only be created by the .get() method
    // on RawBsonDoc
    fn new(element_type: ElementType, data: &'a [u8]) -> RawBson<'a> {
        RawBson { element_type, data }
    }

    pub fn element_type(self) -> ElementType {
        self.element_type
    }

    pub fn as_bytes(self) -> &'a [u8] {
        self.data
    }

    pub fn as_f64(self) -> RawResult<f64> {
        if let ElementType::Double = self.element_type {
            Ok(f64::from_bits(u64::from_le_bytes(
                self.data
                    .try_into()
                    .map_err(|_| RawError::MalformedValue("f64 should be 8 bytes long".into()))?,
            )))
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_str(self) -> RawResult<&'a str> {
        if let ElementType::String = self.element_type {
            read_lenencoded(self.data)
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_document(self) -> RawResult<RawBsonDoc<'a>> {
        if let ElementType::EmbeddedDocument = self.element_type {
            RawBsonDoc::new(self.data)
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_array(self) -> RawResult<RawBsonArray<'a>> {
        if let ElementType::Array = self.element_type {
            RawBsonArray::new(self.data)
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_binary(self) -> RawResult<RawBsonBinary<'a>> {
        if let ElementType::Binary = self.element_type {
            let length = i32_from_slice(&self.data[0..4]);
            let subtype = BinarySubtype::from(self.data[4]); // TODO: This mishandles reserved values
            if self.data.len() as i32 != length + 5 {
                return Err(RawError::MalformedValue(
                    "binary bson has wrong declared length".into(),
                ));
            }
            let data = match subtype {
                BinarySubtype::BinaryOld => {
                    if length < 4 {
                        return Err(RawError::MalformedValue(
                            "old binary subtype has no inner declared length".into(),
                        ));
                    }
                    let oldlength = i32_from_slice(&self.data[5..9]);
                    if oldlength + 4 != length {
                        return Err(RawError::MalformedValue(
                            "old binary subtype has wrong inner declared length".into(),
                        ));
                    }
                    &self.data[9..]
                }
                _ => &self.data[5..],
            };
            Ok(RawBsonBinary::new(subtype, data))
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_object_id(self) -> RawResult<oid::ObjectId> {
        if let ElementType::ObjectId = self.element_type {
            Ok(oid::ObjectId::with_bytes(self.data.try_into().map_err(
                |_| RawError::MalformedValue("object id should be 12 bytes long".into()),
            )?))
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_bool(self) -> RawResult<bool> {
        if let ElementType::Boolean = self.element_type {
            if self.data.len() != 1 {
                Err(RawError::MalformedValue("boolean has length != 1".into()))
            } else {
                match self.data[0] {
                    0 => Ok(false),
                    1 => Ok(true),
                    _ => Err(RawError::MalformedValue(
                        "boolean value was not 0 or 1".into(),
                    )),
                }
            }
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_utc_date_time(self) -> RawResult<DateTime<Utc>> {
        if let ElementType::DateTime = self.element_type {
            let millis = i64_from_slice(self.data);
            if millis >= 0 {
                let duration = Duration::from_millis(millis as u64);
                Ok(Utc.timestamp(
                    duration.as_secs().try_into().unwrap(),
                    duration.subsec_nanos(),
                ))
            } else {
                let duration = Duration::from_millis((-millis).try_into().unwrap());
                let mut secs: i64 = duration.as_secs().try_into().unwrap();
                secs *= -1;
                let mut nanos = duration.subsec_nanos();
                if nanos > 0 {
                    secs -= 1;
                    nanos = 1_000_000_000 - nanos;
                }
                Ok(Utc.timestamp(secs, nanos))
            }
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_null(self) -> RawResult<()> {
        if let ElementType::Null = self.element_type {
            Ok(())
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_regexp(self) -> RawResult<RawBsonRegexp<'a>> {
        if let ElementType::RegularExpression = self.element_type {
            RawBsonRegexp::new(self.data)
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_javascript(self) -> RawResult<&'a str> {
        if let ElementType::JavaScriptCode = self.element_type {
            read_lenencoded(self.data)
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_symbol(self) -> RawResult<&'a str> {
        if let ElementType::Symbol = self.element_type {
            read_lenencoded(self.data)
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_javascript_with_scope(self) -> RawResult<(&'a str, RawBsonDoc<'a>)> {
        if let ElementType::JavaScriptCodeWithScope = self.element_type {
            let length = i32_from_slice(&self.data[..4]);
            assert_eq!(self.data.len() as i32, length);

            let js = read_lenencoded(&self.data[4..])?;
            let doc = RawBsonDoc::new(&self.data[9 + js.len()..])?;

            Ok((js, doc))
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_i32(self) -> RawResult<i32> {
        if let ElementType::Int32 = self.element_type {
            assert_eq!(self.data.len(), 4);
            Ok(i32_from_slice(self.data))
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_timestamp(self) -> RawResult<RawBsonTimestamp<'a>> {
        if let ElementType::Timestamp = self.element_type {
            assert_eq!(self.data.len(), 8);
            Ok(RawBsonTimestamp { data: self.data })
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_i64(self) -> RawResult<i64> {
        if let ElementType::Int64 = self.element_type {
            assert_eq!(self.data.len(), 8);
            Ok(i64_from_slice(self.data))
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_decimal128(self) -> RawResult<Decimal128> {
        if let ElementType::Decimal128 = self.element_type {
            assert_eq!(self.data.len(), 16);
            Ok(d128_from_slice(self.data))
        } else {
            Err(RawError::UnexpectedType)
        }
    }
}

fn try_to_str(data: &[u8]) -> RawResult<&str> {
    match std::str::from_utf8(data) {
        Ok(s) => Ok(s),
        Err(_) => Err(RawError::Utf8EncodingError(data.into())),
    }
}

impl<'a> TryFrom<RawBson<'a>> for Bson {
    type Error = RawError;

    fn try_from(rawbson: RawBson<'a>) -> RawResult<Bson> {
        Ok(match rawbson.element_type {
            ElementType::Double => Bson::Double(rawbson.as_f64()?),
            ElementType::String => Bson::String(String::from(rawbson.as_str()?)),
            ElementType::EmbeddedDocument => {
                let rawdoc = rawbson.as_document()?;
                let doc = rawdoc.try_into()?;
                Bson::Document(doc)
            }
            ElementType::Array => {
                let rawarray = rawbson.as_array()?;
                let v = rawarray.try_into()?;
                Bson::Array(v)
            }
            ElementType::Binary => {
                let RawBsonBinary { subtype, data } = rawbson.as_binary()?;
                Bson::Binary(bson::Binary {
                    subtype,
                    bytes: data.to_vec(),
                })
            }
            ElementType::ObjectId => Bson::ObjectId(rawbson.as_object_id()?),
            ElementType::Boolean => Bson::Boolean(rawbson.as_bool()?),
            ElementType::DateTime => Bson::DateTime(rawbson.as_utc_date_time()?),
            ElementType::Null => Bson::Null,
            ElementType::RegularExpression => {
                let rawregex = rawbson.as_regexp()?;
                Bson::RegularExpression(Regex {
                    pattern: String::from(rawregex.pattern),
                    options: String::from(rawregex.options),
                })
            }
            ElementType::JavaScriptCode => {
                Bson::JavaScriptCode(String::from(rawbson.as_javascript()?))
            }
            ElementType::Int32 => Bson::Int32(rawbson.as_i32()?),
            ElementType::Timestamp => {
                // RawBson::as_timestamp() returns u64, but Bson::Timestamp expects i64
                let ts = rawbson.as_timestamp()?;
                Bson::Timestamp(Timestamp {
                    time: ts.time()?,
                    increment: ts.increment()?,
                })
            }
            ElementType::Int64 => Bson::Int64(rawbson.as_i64()?),
            ElementType::Undefined => Bson::Null,
            ElementType::DbPointer => panic!("Uh oh. Maybe this should be a TryFrom"),
            ElementType::Symbol => Bson::Symbol(String::from(rawbson.as_symbol()?)),
            ElementType::JavaScriptCodeWithScope => {
                let (js, scope) = rawbson.as_javascript_with_scope()?;
                Bson::JavaScriptCodeWithScope(JavaScriptCodeWithScope {
                    code: String::from(js),
                    scope: scope.try_into()?,
                })
            }
            ElementType::Decimal128 => Bson::Decimal128(rawbson.as_decimal128()?),
            ElementType::MaxKey => unimplemented!(),
            ElementType::MinKey => unimplemented!(),
        })
    }
}

// Given a 4 byte u8 slice, return an i32 calculated from the bytes in
// little endian order
//
// # Panics
//
// This function panics if given a slice that is not four bytes long.
fn i32_from_slice(val: &[u8]) -> i32 {
    i32::from_le_bytes(val.try_into().expect("i32 is four bytes"))
}

// Given an 8 byte u8 slice, return an i64 calculated from the bytes in
// little endian order
//
// # Panics
//
// This function panics if given a slice that is not eight bytes long.
fn i64_from_slice(val: &[u8]) -> i64 {
    i64::from_le_bytes(val.try_into().expect("i64 is eight bytes"))
}

// Given a 4 byte u8 slice, return a u32 calculated from the bytes in
// little endian order
//
// # Panics
//
// This function panics if given a slice that is not four bytes long.
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

#[cfg(test)]
mod tests {
    use super::*;
    use bson::{doc, Binary, Bson, Document, JavaScriptCodeWithScope, Regex, Timestamp};

    fn to_bytes(doc: &Document) -> Vec<u8> {
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
        let rawdoc = RawBsonDoc::new(&docbytes).unwrap();
        assert_eq!(rawdoc.get("that").unwrap().as_str().unwrap(), "second",);
    }

    #[test]
    fn nested_document() {
        let docbytes = to_bytes(&doc! {
            "outer": {
                "inner": "surprise",
            },
        });
        let rawdoc = RawBsonDoc::new(&docbytes).unwrap();
        assert_eq!(
            rawdoc
                .get("outer")
                .expect("get doc")
                .as_document()
                .expect("as doc")
                .get("inner")
                .expect("get str")
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
        let rawdoc = RawBsonDoc::new(&docbytes).expect("malformed bson document");
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

        let rawdoc = RawBsonDoc::new_unchecked(&docbytes);
        let _doc: Document = rawdoc.try_into().expect("invalid bson");
    }

    #[test]
    fn f64() {
        #![allow(clippy::float_cmp)]

        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {"f64": 2.5}));
        assert_eq!(
            rawdoc
                .get("f64")
                .expect("no key f64")
                .as_f64()
                .expect("result was not a f64"),
            2.5,
        );
    }

    #[test]
    fn string() {
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {"string": "hello"}));

        assert_eq!(
            rawdoc
                .get("string")
                .expect("no key string")
                .as_str()
                .expect("result was not a string"),
            "hello",
        );
    }
    #[test]
    fn document() {
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {"document": {}}));

        let doc = rawdoc
            .get("document")
            .expect("no key document")
            .as_document()
            .expect("result was not a document");
        assert_eq!(doc.data, &[5, 0, 0, 0, 0]); // Empty document
    }

    #[test]
    fn array() {
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(
            &doc! { "array": ["binary", "serialized", "object", "notation"]},
        ));

        let array: RawBsonArray<'_> = rawdoc
            .get("array")
            .expect("no key array")
            .as_array()
            .expect("result was not an array");
        assert_eq!(array.get_str(0), Ok("binary"));
        assert_eq!(array.get_str(3), Ok("notation"));
        assert_eq!(array.get_str(4), Err(RawError::NotPresent));
    }

    #[test]
    fn binary() {
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {
            "binary": Binary { subtype: BinarySubtype::Generic, bytes: vec![1u8, 2, 3] }
        }));
        let binary: RawBsonBinary<'_> = rawdoc
            .get("binary")
            .expect("no key binary")
            .as_binary()
            .expect("result was not a binary object");
        assert_eq!(binary.subtype, BinarySubtype::Generic);
        assert_eq!(binary.data, &[1, 2, 3]);
    }

    #[test]
    fn object_id() {
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {
            "object_id": oid::ObjectId::with_bytes([1, 2, 3, 4, 5,6,7,8,9,10, 11,12]),
        }));
        let oid = rawdoc
            .get("object_id")
            .expect("no key object_id")
            .as_object_id()
            .expect("result was not an object id");
        assert_eq!(oid.to_hex(), "0102030405060708090a0b0c");
    }

    #[test]
    fn boolean() {
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {
            "boolean": true,
        }));

        let boolean = rawdoc
            .get("boolean")
            .expect("no key boolean")
            .as_bool()
            .expect("result was not boolean");

        assert_eq!(boolean, true);
    }

    #[test]
    fn datetime() {
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {
            "boolean": true,
            "datetime": Utc.ymd(2000,10,31).and_hms(12, 30, 45),
        }));
        let datetime = rawdoc
            .get("datetime")
            .expect("no key datetime")
            .as_utc_date_time()
            .expect("result was not datetime");
        assert_eq!(datetime.to_rfc3339(), "2000-10-31T12:30:45+00:00");
    }

    #[test]
    fn null() {
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {
            "null": null,
        }));
        let () = rawdoc
            .get("null")
            .expect("no key null")
            .as_null()
            .expect("was not null");
    }

    #[test]
    fn regex() {
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {
            "regex": Bson::RegularExpression(Regex { pattern: String::from(r"end\s*$"), options: String::from("i")}),
        }));
        let regex = rawdoc
            .get("regex")
            .expect("no key regex")
            .as_regexp()
            .expect("was not regex");
        assert_eq!(regex.pattern, r"end\s*$");
        assert_eq!(regex.options, "i");
    }
    #[test]
    fn javascript() {
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {
            "javascript": Bson::JavaScriptCode(String::from("console.log(console);")),
        }));
        let js = rawdoc
            .get("javascript")
            .expect("no key javascript")
            .as_javascript()
            .expect("was not javascript");
        assert_eq!(js, "console.log(console);");
    }

    #[test]
    fn symbol() {
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {
            "symbol": Bson::Symbol(String::from("artist-formerly-known-as")),
        }));

        let symbol = rawdoc
            .get("symbol")
            .expect("no key symbol")
            .as_symbol()
            .expect("was not symbol");
        assert_eq!(symbol, "artist-formerly-known-as");
    }

    #[test]
    fn javascript_with_scope() {
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {
            "javascript_with_scope": Bson::JavaScriptCodeWithScope(JavaScriptCodeWithScope{ code: String::from("console.log(msg);"), scope: doc!{"ok": true}}),
        }));
        let (js, scopedoc) = rawdoc
            .get("javascript_with_scope")
            .expect("no key javascript_with-scope")
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
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {
            "int32": 23i32,
        }));
        let int32 = rawdoc
            .get("int32")
            .expect("no key int32")
            .as_i32()
            .expect("was not int32");
        assert_eq!(int32, 23i32);
    }

    #[test]
    fn timestamp() {
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {
            "timestamp": Bson::Timestamp(Timestamp { time: 3542578, increment: 0 }),
        }));
        let ts = rawdoc
            .get("timestamp")
            .expect("no key timestamp")
            .as_timestamp()
            .expect("was not a timestamp");

        assert_eq!(ts.increment().expect("timestamp has invalid increment"), 0);
        assert_eq!(ts.time().expect("timestamp has invalid time"), 3542578);
    }

    #[test]
    fn int64() {
        let rawdoc = RawBsonDocBuf::new_unchecked(to_bytes(&doc! {
            "int64": 46i64,
        }));
        let int64 = rawdoc
            .get("int64")
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
        let rawdoc = RawBsonDoc::new_unchecked(&docbytes);

        assert_eq!(rawdoc.into_iter().collect::<Result<Vec<(&str, _)>, RawError>>().expect("collecting iterated doc").len(), 17);
        let end = rawdoc
            .get("end")
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
        let rawbson = RawBson::new(ElementType::EmbeddedDocument, &docbytes);
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

    use super::RawBsonDocBuf;
    use crate::props::arbitrary_bson;
    use bson::{doc, Document};

    fn to_bytes(doc: &Document) -> Vec<u8> {
        let mut docbytes = Vec::new();
        doc.to_writer(&mut docbytes).unwrap();
        docbytes
    }

    proptest! {
        #[test]
        fn no_crashes(s: Vec<u8>) {
            let _ = RawBsonDocBuf::new(s);
        }

        #[test]
        fn roundtrip_bson(bson in arbitrary_bson()) {
            println!("{:?}", bson);
            let doc = doc!{"bson": bson};
            let raw = to_bytes(&doc);
            let raw = RawBsonDocBuf::new(raw);
            prop_assert!(raw.is_ok());
            let raw = raw.unwrap();
            let roundtrip: Result<Document, _> = raw.try_into();
            prop_assert!(roundtrip.is_ok());
            let roundtrip = roundtrip.unwrap();
            prop_assert_eq!(doc, roundtrip);
        }
    }
}
