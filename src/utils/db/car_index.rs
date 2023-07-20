// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! # TLDR;
//! 
//! [`CarIndex`] is equivalent to `HashMap<Cid, Vec<BlockPosition>>`. It can be
//! built in `O(n)` time, loaded from a reader in `O(1)` time, has `O(1)`
//! lookups, and uses no caches by default.
//! 
//! # Context: Binary search
//! 
//! You can find any word in a dictionary in `O(logn)` steps by binary search.
//! However, no human would look for `aardvark` in the middle of a dictionary.
//! It's reasonable to assume that a word starting with two `A`s will appear
//! closer to the beginning of the dictionary rather than the end.
//! 
//! Dictionary words are not evenly distributed, though, as there are more words
//! starting with 'c' than with 'a'. This makes it difficult to know exactly
//! where in the dictionary to start looking for a word.
//! 
//! If we don't care about ordering, we can hash the words and thus be sure that
//! the words are spread evenly across the dictionary. No hash key is more or
//! less likely than any other.
//! 
//! # Context: Hash tables and linear probing
//! 
//! We could take the dictionary words and put them in different buckets
//! according to their hash value. Looking up a word requires finding the right
//! bucket and sifting through its content. With more buckets and words, each
//! bucket will, on average, not contain more than a single word.
//! 
//! Let's simplify things and put the buckets in a single array. The contents of
//! bucket 0 starts at offset 0, bucket 1 starts at offset 1, etc. Some buckets
//! are empty and leave unfilled slots, other buckets have multiple entries and
//! spill into slots meant for someone else.
//! 
//! To look up a value in this array, we go to the expected offset of the bucket
//! and then skip all keys that have spilled buckets with a smaller offset. The
//! number of keys to skip will always be small and it is fast to linearly scan
//! an array.
//! 
//! This technique is called [linear
//! probing](https://en.wikipedia.org/wiki/Linear_probing).
//! 
//! ## Examples of linear probing
//! 
//! 
//! 

mod block_position;
mod car_index;
mod car_index_builder;
mod hash;
mod key_value_pair;
mod slot;

use block_position::BlockPosition;
pub use car_index::CarIndex;
pub use car_index_builder::CarIndexBuilder;
use hash::Hash;
use key_value_pair::KeyValuePair;
use slot::Slot;

#[cfg(test)]
mod tests;
