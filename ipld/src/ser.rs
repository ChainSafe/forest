// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Copyright 2017 Serde Developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::BTreeMap;

use super::{to_ipld, Error, Ipld};
use cid::Cid;
use encoding::to_vec;
use serde::{self, Serialize};
use std::convert::TryFrom;

impl serde::Serialize for Ipld {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Ipld::Integer(v) => serializer.serialize_i128(*v),
            Ipld::Bytes(v) => serializer.serialize_bytes(&v),
            Ipld::String(v) => serializer.serialize_str(&v),
            Ipld::List(v) => v.serialize(serializer),
            Ipld::Map(v) => v.serialize(serializer),
            Ipld::Link(cid) => cid.serialize(serializer),
            Ipld::Float(v) => serializer.serialize_f64(*v),
            Ipld::Bool(v) => serializer.serialize_bool(*v),
            Ipld::Null => serializer.serialize_unit(),
        }
    }
}

pub(super) struct Serializer;

impl serde::Serializer for Serializer {
    type Ok = Ipld;
    type Error = Error;

    type SerializeSeq = SerializeVec;
    type SerializeTuple = SerializeVec;
    type SerializeTupleStruct = SerializeVec;
    type SerializeTupleVariant = SerializeTupleVariant;
    type SerializeMap = SerializeMap;
    type SerializeStruct = SerializeMap;
    type SerializeStructVariant = SerializeStructVariant;

    #[inline]
    fn serialize_bool(self, ipld: bool) -> Result<Ipld, Error> {
        Ok(Ipld::Bool(ipld))
    }

    #[inline]
    fn serialize_i8(self, ipld: i8) -> Result<Ipld, Error> {
        self.serialize_i64(i64::from(ipld))
    }

    #[inline]
    fn serialize_i16(self, ipld: i16) -> Result<Ipld, Error> {
        self.serialize_i64(i64::from(ipld))
    }

    #[inline]
    fn serialize_i32(self, ipld: i32) -> Result<Ipld, Error> {
        self.serialize_i64(i64::from(ipld))
    }

    #[inline]
    fn serialize_i64(self, ipld: i64) -> Result<Ipld, Error> {
        self.serialize_i128(i128::from(ipld))
    }

    fn serialize_i128(self, ipld: i128) -> Result<Ipld, Error> {
        Ok(Ipld::Integer(ipld))
    }

    #[inline]
    fn serialize_u8(self, ipld: u8) -> Result<Ipld, Error> {
        self.serialize_u64(u64::from(ipld))
    }

    #[inline]
    fn serialize_u16(self, ipld: u16) -> Result<Ipld, Error> {
        self.serialize_u64(u64::from(ipld))
    }

    #[inline]
    fn serialize_u32(self, ipld: u32) -> Result<Ipld, Error> {
        self.serialize_u64(u64::from(ipld))
    }

    #[inline]
    fn serialize_u64(self, ipld: u64) -> Result<Ipld, Error> {
        Ok(Ipld::Integer(ipld.into()))
    }

    #[inline]
    fn serialize_f32(self, ipld: f32) -> Result<Ipld, Error> {
        self.serialize_f64(f64::from(ipld))
    }

    #[inline]
    fn serialize_f64(self, ipld: f64) -> Result<Ipld, Error> {
        Ok(Ipld::Float(ipld))
    }

    #[inline]
    fn serialize_char(self, ipld: char) -> Result<Ipld, Error> {
        let mut s = String::new();
        s.push(ipld);
        self.serialize_str(&s)
    }

    #[inline]
    fn serialize_str(self, ipld: &str) -> Result<Ipld, Error> {
        Ok(Ipld::String(ipld.to_owned()))
    }

    fn serialize_bytes(self, ipld: &[u8]) -> Result<Ipld, Error> {
        Ok(Ipld::Bytes(ipld.to_vec()))
    }

    #[inline]
    fn serialize_unit(self) -> Result<Ipld, Error> {
        Ok(Ipld::Null)
    }

    #[inline]
    fn serialize_unit_struct(self, _name: &'static str) -> Result<Ipld, Error> {
        self.serialize_unit()
    }

    #[inline]
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Ipld, Error> {
        self.serialize_str(variant)
    }

    #[inline]
    fn serialize_newtype_struct<T: ?Sized>(
        self,
        name: &'static str,
        ipld: &T,
    ) -> Result<Ipld, Error>
    where
        T: Serialize,
    {
        // TODO revisit this, necessary workaround to allow Cids to be converted to Ipld
        // but is not very clean to use and requires the bytes buffer. The reason this is
        // necessary is because Cids serialize through newtype_struct.
        if name == "\0cbor_tag" {
            let bz = to_vec(&ipld)?;
            let mut sl = &bz[..];

            if bz.len() < 3 {
                return Err(Error::Encoding("Invalid tag for Ipld".to_owned()));
            }

            // Index past the cbor and multibase prefix for Cid deserialization
            match sl[0] {
                0x40..=0x57 => sl = &sl[2..],
                0x58 => sl = &sl[3..],  // extra u8
                0x59 => sl = &sl[4..],  // extra u16
                0x5a => sl = &sl[6..],  // extra u32
                0x5b => sl = &sl[10..], // extra u64
                _ => return Err(Error::Encoding("Invalid cbor tag".to_owned())),
            }

            return Ok(Ipld::Link(
                Cid::try_from(sl).map_err(|e| Error::Encoding(e.to_string()))?,
            ));
        }
        ipld.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        ipld: &T,
    ) -> Result<Ipld, Error>
    where
        T: Serialize,
    {
        let mut values = BTreeMap::new();
        values.insert(variant.to_owned(), to_ipld(&ipld)?);
        Ok(Ipld::Map(values))
    }

    #[inline]
    fn serialize_none(self) -> Result<Ipld, Error> {
        self.serialize_unit()
    }

    #[inline]
    fn serialize_some<T: ?Sized>(self, ipld: &T) -> Result<Ipld, Error>
    where
        T: Serialize,
    {
        ipld.serialize(self)
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Error> {
        Ok(SerializeVec {
            vec: Vec::with_capacity(len.unwrap_or(0)),
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Error> {
        self.serialize_tuple(len)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Error> {
        Ok(SerializeTupleVariant {
            name: String::from(variant),
            vec: Vec::with_capacity(len),
        })
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Error> {
        Ok(SerializeMap {
            map: BTreeMap::new(),
            next_key: None,
        })
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Error> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Error> {
        Ok(SerializeStructVariant {
            name: String::from(variant),
            map: BTreeMap::new(),
        })
    }

    #[inline]
    fn is_human_readable(&self) -> bool {
        false
    }
}

pub struct SerializeVec {
    vec: Vec<Ipld>,
}

pub struct SerializeTupleVariant {
    name: String,
    vec: Vec<Ipld>,
}

pub struct SerializeMap {
    map: BTreeMap<String, Ipld>,
    next_key: Option<String>,
}

pub struct SerializeStructVariant {
    name: String,
    map: BTreeMap<String, Ipld>,
}

impl serde::ser::SerializeSeq for SerializeVec {
    type Ok = Ipld;
    type Error = Error;

    fn serialize_element<T: ?Sized>(&mut self, ipld: &T) -> Result<(), Error>
    where
        T: Serialize,
    {
        self.vec.push(to_ipld(&ipld)?);
        Ok(())
    }

    fn end(self) -> Result<Ipld, Error> {
        Ok(Ipld::List(self.vec))
    }
}

impl serde::ser::SerializeTuple for SerializeVec {
    type Ok = Ipld;
    type Error = Error;

    fn serialize_element<T: ?Sized>(&mut self, ipld: &T) -> Result<(), Error>
    where
        T: Serialize,
    {
        serde::ser::SerializeSeq::serialize_element(self, ipld)
    }

    fn end(self) -> Result<Ipld, Error> {
        serde::ser::SerializeSeq::end(self)
    }
}

impl serde::ser::SerializeTupleStruct for SerializeVec {
    type Ok = Ipld;
    type Error = Error;

    fn serialize_field<T: ?Sized>(&mut self, ipld: &T) -> Result<(), Error>
    where
        T: Serialize,
    {
        serde::ser::SerializeSeq::serialize_element(self, ipld)
    }

    fn end(self) -> Result<Ipld, Error> {
        serde::ser::SerializeSeq::end(self)
    }
}

impl serde::ser::SerializeTupleVariant for SerializeTupleVariant {
    type Ok = Ipld;
    type Error = Error;

    fn serialize_field<T: ?Sized>(&mut self, ipld: &T) -> Result<(), Error>
    where
        T: Serialize,
    {
        self.vec.push(to_ipld(&ipld)?);
        Ok(())
    }

    fn end(self) -> Result<Ipld, Error> {
        let mut object = BTreeMap::new();

        object.insert(self.name, Ipld::List(self.vec));

        Ok(Ipld::Map(object))
    }
}

impl serde::ser::SerializeMap for SerializeMap {
    type Ok = Ipld;
    type Error = Error;

    fn serialize_key<T: ?Sized>(&mut self, key: &T) -> Result<(), Error>
    where
        T: Serialize,
    {
        if let Ipld::String(key_s) = to_ipld(&key)? {
            self.next_key = Some(key_s);
        } else {
            return Err(Error::Encoding(
                "Map keys must be a string in IPLD".to_owned(),
            ));
        }
        Ok(())
    }

    fn serialize_value<T: ?Sized>(&mut self, ipld: &T) -> Result<(), Error>
    where
        T: Serialize,
    {
        let key = self.next_key.take();
        // Panic because this indicates a bug in the program rather than an
        // expected failure.
        let key = key.expect("serialize_value called before serialize_key");
        self.map.insert(key, to_ipld(&ipld)?);
        Ok(())
    }

    fn end(self) -> Result<Ipld, Error> {
        Ok(Ipld::Map(self.map))
    }
}

impl serde::ser::SerializeStruct for SerializeMap {
    type Ok = Ipld;
    type Error = Error;

    fn serialize_field<T: ?Sized>(&mut self, key: &'static str, ipld: &T) -> Result<(), Error>
    where
        T: Serialize,
    {
        serde::ser::SerializeMap::serialize_key(self, key)?;
        serde::ser::SerializeMap::serialize_value(self, ipld)
    }

    fn end(self) -> Result<Ipld, Error> {
        serde::ser::SerializeMap::end(self)
    }
}

impl serde::ser::SerializeStructVariant for SerializeStructVariant {
    type Ok = Ipld;
    type Error = Error;

    fn serialize_field<T: ?Sized>(&mut self, key: &'static str, ipld: &T) -> Result<(), Error>
    where
        T: Serialize,
    {
        self.map.insert(String::from(key), to_ipld(&ipld)?);
        Ok(())
    }

    fn end(self) -> Result<Ipld, Error> {
        let mut object = BTreeMap::new();

        object.insert(self.name, Ipld::Map(self.map));

        Ok(Ipld::Map(object))
    }
}
