use cid::Cid;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

// Turn [(Cid, u64)]> into Vec<[u8]>

type Table = Vec<Option<(Cid, u64)>>;

fn hash_cid(cid: Cid) -> u64 {
    u64::from_le_bytes(cid.hash().digest().try_into().unwrap())
}

fn insert(table: &mut Table, entry: (Cid, u64)) {
    let start_offset = hash_cid(entry.0) as usize % table.len();
    loop {
        match table[start_offset] {
            None => {
                table[start_offset] = Some(entry);
                break;
            }
            Some(other_entry) => {

            }
        }
    }
}

fn to_index(values: Vec<(Cid, u64)>) -> Table {
    let size = values.len() * 10 / 9;
    let mut vec = Vec::with_capacity(values.len() * 10 / 9);
    vec.resize(size, None);
    vec
}
