use crate::utils::cid::CidCborExt;
use cid::Cid;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::Hasher;

fn hash_cid(cid: Cid) -> usize {
    usize::from_le_bytes(cid.hash().digest()[0..8].try_into().unwrap())
}

#[derive(Debug)]
pub struct ProbingHashtableBuilder {
    table: Vec<Option<(usize, u64)>>,
}

impl ProbingHashtableBuilder {
    fn new(values: &[(Cid, u64)]) -> ProbingHashtableBuilder {
        let size = values.len() * 100 / 90;
        println!("Entries: {}, size: {}, buffer: {}", values.len(), size, size - values.len());
        let mut vec = Vec::with_capacity(size);
        vec.resize(size, None);
        let mut table = ProbingHashtableBuilder { table: vec };
        for (cid, val) in values.into_iter().cloned() {
            table.insert((hash_cid(cid), val))
        }
        table
    }

    fn insert(&mut self, mut entry: (usize, u64)) {
        let entry_offset = entry.0 % self.table.len();
        let mut at = entry_offset;
        loop {
            match self.table[at] {
                None => {
                    self.table[at] = Some(entry);
                    break;
                }
                Some(other_entry) => {
                    let other_offset = other_entry.0 % self.table.len();
                    if entry_offset < other_offset {
                        self.table[at] = Some(entry);
                        entry = other_entry;
                    }
                    at = (at + 1) % self.table.len();
                }
            }
        }
    }

    fn read_misses(&self) -> BTreeMap<usize, usize> {
        let mut map = BTreeMap::new();
        for (n, elt) in self.table.iter().enumerate() {
            if let Some(entry) = elt {
                let best_position = entry.0 % self.table.len();
                let diff = (n as isize - best_position as isize)
                    .rem_euclid(self.table.len() as isize) as usize;

                map.entry(diff).and_modify(|n| *n += 1).or_insert(1);
            }
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_insert() {
        let table = ProbingHashtableBuilder::new(
            &(1..=1_000_000)
                .map(|i| (Cid::from_cbor_blake2b256(&i).unwrap(), i))
                .collect::<Vec<_>>(),
        );
        // dbg!(&table);
        dbg!(table.read_misses());
    }
}
