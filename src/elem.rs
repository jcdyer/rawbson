use std::{convert::{TryFrom, TryInto}, time::Duration};

use bson::oid;
pub use bson::spec::{BinarySubtype, ElementType};
use chrono::{DateTime, TimeZone, Utc};

use crate::{ArrayRef, DocRef, RawError, RawResult, d128_from_slice, i32_from_slice, i64_from_slice, read_lenencoded, read_nullterminated, u32_from_slice};



#[derive(Clone, Copy)]
pub struct Element<'a> {
    element_type: ElementType,
    data: &'a [u8],
}

impl<'a> Element<'a> {

    // This is not public.  An Element object can only be created by iterating over a bson document method
    // on RawBsonDoc
    pub(super) fn new(element_type: ElementType, data: &'a [u8]) -> Element<'a> {
        Element { element_type, data }
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

    pub fn as_document(self) -> RawResult<DocRef<'a>> {
        if let ElementType::EmbeddedDocument = self.element_type {
            DocRef::new(self.data)
        } else {
            Err(RawError::UnexpectedType)
        }
    }

    pub fn as_array(self) -> RawResult<ArrayRef<'a>> {
        if let ElementType::Array = self.element_type {
            ArrayRef::new(self.data)
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

    pub fn as_datetime(self) -> RawResult<DateTime<Utc>> {
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

    pub fn as_regex(self) -> RawResult<RawBsonRegex<'a>> {
        if let ElementType::RegularExpression = self.element_type {
            RawBsonRegex::new(self.data)
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

    pub fn as_javascript_with_scope(self) -> RawResult<(&'a str, DocRef<'a>)> {
        if let ElementType::JavaScriptCodeWithScope = self.element_type {
            let length = i32_from_slice(&self.data[..4]);
            assert_eq!(self.data.len() as i32, length);

            let js = read_lenencoded(&self.data[4..])?;
            let doc = DocRef::new(&self.data[9 + js.len()..])?;

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

    pub fn as_decimal128(self) -> RawResult<bson::Decimal128> {
        if let ElementType::Decimal128 = self.element_type {
            assert_eq!(self.data.len(), 16);
            Ok(d128_from_slice(self.data))
        } else {
            Err(RawError::UnexpectedType)
        }
    }
}

impl<'a> TryFrom<Element<'a>> for bson::Bson {
    type Error = RawError;

    fn try_from(rawbson: Element<'a>) -> RawResult<bson::Bson> {
        Ok(match rawbson.element_type {
            ElementType::Double => bson::Bson::Double(rawbson.as_f64()?),
            ElementType::String => bson::Bson::String(String::from(rawbson.as_str()?)),
            ElementType::EmbeddedDocument => {
                let rawdoc = rawbson.as_document()?;
                let doc = rawdoc.try_into()?;
                bson::Bson::Document(doc)
            }
            ElementType::Array => {
                let rawarray = rawbson.as_array()?;
                let v = rawarray.try_into()?;
                bson::Bson::Array(v)
            }
            ElementType::Binary => {
                let RawBsonBinary { subtype, data } = rawbson.as_binary()?;
                bson::Bson::Binary(bson::Binary {
                    subtype,
                    bytes: data.to_vec(),
                })
            }
            ElementType::ObjectId => bson::Bson::ObjectId(rawbson.as_object_id()?),
            ElementType::Boolean => bson::Bson::Boolean(rawbson.as_bool()?),
            ElementType::DateTime => bson::Bson::DateTime(rawbson.as_datetime()?),
            ElementType::Null => bson::Bson::Null,
            ElementType::RegularExpression => {
                let rawregex = rawbson.as_regex()?;
                bson::Bson::RegularExpression(bson::Regex {
                    pattern: String::from(rawregex.pattern()),
                    options: String::from(rawregex.options()),
                })
            }
            ElementType::JavaScriptCode => {
                bson::Bson::JavaScriptCode(String::from(rawbson.as_javascript()?))
            }
            ElementType::Int32 => bson::Bson::Int32(rawbson.as_i32()?),
            ElementType::Timestamp => {
                // RawBson::as_timestamp() returns u64, but bson::Bson::Timestamp expects i64
                let ts = rawbson.as_timestamp()?;
                bson::Bson::Timestamp(bson::Timestamp {
                    time: ts.time()?,
                    increment: ts.increment()?,
                })
            }
            ElementType::Int64 => bson::Bson::Int64(rawbson.as_i64()?),
            ElementType::Undefined => bson::Bson::Null,
            ElementType::DbPointer => panic!("Uh oh. Maybe this should be a TryFrom"),
            ElementType::Symbol => bson::Bson::Symbol(String::from(rawbson.as_symbol()?)),
            ElementType::JavaScriptCodeWithScope => {
                let (js, scope) = rawbson.as_javascript_with_scope()?;
                bson::Bson::JavaScriptCodeWithScope(bson::JavaScriptCodeWithScope {
                    code: String::from(js),
                    scope: scope.try_into()?,
                })
            }
            ElementType::Decimal128 => bson::Bson::Decimal128(rawbson.as_decimal128()?),
            ElementType::MaxKey => unimplemented!(),
            ElementType::MinKey => unimplemented!(),
        })
    }
}

#[derive(Clone, Copy)]
pub struct RawBsonBinary<'a> {
    pub(super) subtype: BinarySubtype,
    pub(super) data: &'a [u8],
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
pub struct RawBsonRegex<'a> {
    pub(super) pattern: &'a str,
    pub(super) options: &'a str,
}

impl<'a> RawBsonRegex<'a> {
    pub fn new(data: &'a [u8]) -> RawResult<RawBsonRegex<'a>> {
        let pattern = read_nullterminated(data)?;
        let opts = read_nullterminated(&data[pattern.len() + 1..])?;
        if pattern.len() + opts.len() == data.len() - 2 {
            Ok(RawBsonRegex {
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
    pub(super) data: &'a [u8],
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
