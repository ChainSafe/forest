use crate::utils::cid::CidCborExt;
use cid::Cid;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

// Turn [(Cid, u64)]> into Vec<[u8]>

type Table = Vec<Option<(usize, u64)>>;

fn hash_cid(cid: Cid) -> usize {
    usize::from_le_bytes(cid.hash().digest()[0..8].try_into().unwrap())
}

fn insert(table: &mut Table, mut entry: (usize, u64)) {
    // let entry_offset = hash_cid(entry.0) % table.len();
    let entry_offset = entry.0 % table.len();
    let mut at = entry_offset;
    loop {
        match table[at] {
            None => {
                table[at] = Some(entry);
                break;
            }
            Some(other_entry) => {
                let other_offset = other_entry.0 % table.len();
                if entry_offset < other_offset {
                    table[at] = Some(entry);
                    entry = other_entry;
                }
                at = (at + 1) % table.len();
            }
        }
    }
}

fn to_index(values: Vec<(Cid, u64)>) -> Table {
    let size = values.len() * 10 / 9;
    let mut vec = Vec::with_capacity(values.len() * 10 / 9);
    vec.resize(size, None);
    for (cid, val) in values.into_iter() {
        insert(&mut vec, (hash_cid(cid), val))
    }
    vec
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_insert() {
        let table = to_index(
            (0..=50)
                .map(|i| (Cid::from_cbor_blake2b256(&i).unwrap(), i))
                .collect(),
        );
        dbg!(table);
    }
}
