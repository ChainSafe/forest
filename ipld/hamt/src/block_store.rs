// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::Error;
use cid::Cid;
use db::{MemoryDB, Read, RocksDb, Write};
use encoding::{de::DeserializeOwned, from_slice, ser::Serialize, to_vec};

/// Wrapper for database to handle inserting and retrieving data from AMT with Cids
pub trait BlockStore: Read + Write {
    /// Get bytes from block store by Cid
    fn get_bytes(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.read(cid.to_bytes())?)
    }
    /// Get typed object from block store by Cid
    fn get<T>(&self, cid: &Cid) -> Result<Option<T>, Error>
    where
        T: DeserializeOwned,
    {
        match self.get_bytes(cid)? {
            Some(bz) => Ok(Some(from_slice(&bz)?)),
            None => Ok(None),
        }
    }
    /// Put an object in the block store and return the Cid identifier
    fn put<S>(&self, obj: &S) -> Result<Cid, Error>
    where
        S: Serialize,
    {
        let bz = to_vec(obj)?;
        let cid = Cid::from_bytes_default(&bz)?;
        self.write(cid.to_bytes(), bz)?;
        Ok(cid)
    }
}

impl BlockStore for MemoryDB {}
impl BlockStore for RocksDb {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hamt;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::str::FromStr;

    #[test]
    fn test_memory() {
        let store = db::MemoryDB::default();

        let c1 = store.put(&("hello".to_string(), 3)).unwrap();
        let back = store.get(&c1).unwrap();
        assert_eq!(back, Some(("hello".to_string(), 3)));
    }

    #[test]
    fn test_memory_interop() {
        let store = db::MemoryDB::default();

        let mut thingy1 = HashMap::new();
        thingy1.insert("cat".to_string(), "dog".to_string());

        let c1 = store.put(&thingy1).unwrap();

        assert_eq!(
            c1,
            Cid::from_str("zdpuAqYjGuvUBhcmyFhHjh9mZbBW5MYLD2eUcXTWqmj73dHXD")
                .unwrap()
                .into()
        );

        #[derive(Debug, Serialize, Deserialize)]
        struct Thingy2 {
            one: Cid,
            foo: String,
        }

        let thingy2 = Thingy2 {
            one: c1.clone().into(),
            foo: "bar".into(),
        };

        let c2 = store.put(&thingy2).unwrap();
        println!("{}", hex::encode(store.get_bytes(&c2).unwrap().unwrap()));

        assert_eq!(
            c2,
            Cid::from_str("zdpuAt1cw4ZvvLnXL9KFbEkM3vXibtwiJek8d3o4h1fPkEgMX")
                .unwrap()
                .into()
        );

        let mut hamt: Hamt<String, Thingy2, _> = Hamt::new(&store);
        hamt.insert("cat".to_string(), thingy2);

        let c3 = store.put(&hamt).unwrap();
        println!(
            "c3: {}",
            hex::encode(store.get_bytes(&c3).unwrap().unwrap())
        );
        println!("{:#?}", &hamt);

        // Not quite there yet
        // assert_eq!(
        //     c3,
        //     Cid::from("zdpuApTKRtVAtwquN7f3A5bZBXnsLkmpLQfF7CVAeGDbkL5Zo")
        //         .unwrap()
        //         .into()
        // );
    }
}
