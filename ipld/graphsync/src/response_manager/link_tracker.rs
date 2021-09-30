// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::RequestID;
use cid::Cid;
use fnv::FnvHashMap;
use std::collections::{HashMap, HashSet};

/// LinkTracker records links being traversed to determine useful information
/// in crafting responses for a peer. Specifically, if any in progress request
/// has already sent a block for a given link, don't send it again.
/// Second, keep track of whether blocks are missing so you can determine
/// at the end if a complete response has been transmitted.
#[derive(Default)]
pub struct LinkTracker {
    /// The links traversed by any given request which corresponding blocks were present.
    present_blocks: FnvHashMap<RequestID, Vec<Cid>>,

    /// The links traversed by any given request which corresponding blocks were missing.
    missing_blocks: FnvHashMap<RequestID, HashSet<Cid>>,

    /// The number of times any given link has been traversed by in-progress requests.
    in_progress_traversal_counts: HashMap<Cid, u32>,
}

impl LinkTracker {
    /// Creates a new link tracker.
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns the number of times a present block has been traversed by in progress requests.
    /// Used to determine whether the block corresponding to this link needs to be sent to the
    /// peer or not.
    pub fn block_ref_count(&self, link: &Cid) -> u32 {
        self.in_progress_traversal_counts
            .get(link)
            .copied()
            .unwrap_or(0)
    }

    /// Returns whether for a given request, the block corresponding to the given link is missing.
    #[allow(unused)]
    pub fn is_known_missing_link(&self, id: RequestID, link: &Cid) -> bool {
        self.missing_blocks
            .get(&id)
            .map_or(false, |cids| cids.contains(link))
    }

    /// Records that we traversed a link during a request, and whether we had the block when we did it.
    pub fn record_link_traversal(&mut self, id: RequestID, link: Cid, block_is_present: bool) {
        if block_is_present {
            self.present_blocks
                .entry(id)
                .or_default()
                .push(link.clone());
            *self.in_progress_traversal_counts.entry(link).or_insert(0) += 1;
        } else {
            self.missing_blocks.entry(id).or_default().insert(link);
        }
    }

    /// Records that we have completed the given request, and returns true if all
    /// links traversed had blocks present.
    pub fn finish_request(&mut self, id: RequestID) -> bool {
        let has_all_blocks = self.missing_blocks.remove(&id).is_none();
        if let Some(cids) = self.present_blocks.remove(&id) {
            for cid in cids {
                // acquire an OccupiedEntry for the traversal count of this cid
                use std::collections::hash_map::Entry;
                let mut entry = match self.in_progress_traversal_counts.entry(cid) {
                    Entry::Occupied(entry) => entry,
                    Entry::Vacant(_) => continue,
                };

                // decrement the count, and remove the entry altogether if
                // the current request was the last ongoing request that
                // traversed this link (for this peer)
                *entry.get_mut() -= 1;
                if *entry.get() == 0 {
                    entry.remove();
                }
            }
        }
        has_all_blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;

    #[test]
    fn block_ref_count() {
        struct Request<'a> {
            // true if the block was present, false if it was missing, one entry for each traversal
            traversals: &'a [bool],
            is_finished: bool,
        }

        let link = test_utils::random_cid();

        let ref_count = |requests: &[Request<'_>]| {
            let mut link_tracker = LinkTracker::new();
            for (id, request) in (0..).zip(requests) {
                for &block_is_present in request.traversals {
                    link_tracker.record_link_traversal(id, link.clone(), block_is_present);
                }
                if request.is_finished {
                    link_tracker.finish_request(id);
                }
            }
            link_tracker.block_ref_count(&link)
        };

        // not traversed
        assert_eq!(ref_count(&[]), 0);

        // traversed once, block present
        assert_eq!(
            ref_count(&[Request {
                traversals: &[true],
                is_finished: false
            }]),
            1
        );

        // traversed once, block missing
        assert_eq!(
            ref_count(&[Request {
                traversals: &[false],
                is_finished: false
            }]),
            0
        );

        // traversed twice, different requests
        assert_eq!(
            ref_count(&[
                Request {
                    traversals: &[true],
                    is_finished: false
                },
                Request {
                    traversals: &[true],
                    is_finished: false
                }
            ]),
            2
        );

        // traversed twice, same request
        assert_eq!(
            ref_count(&[Request {
                traversals: &[true, true],
                is_finished: false
            }]),
            2
        );

        // traversed twice, same request, block available after missing
        assert_eq!(
            ref_count(&[Request {
                traversals: &[false, true],
                is_finished: false
            }]),
            1
        );

        // traversed once, block present, request finished
        assert_eq!(
            ref_count(&[Request {
                traversals: &[true],
                is_finished: true
            }]),
            0
        );

        // traversed twice, different requests, one request finished
        assert_eq!(
            ref_count(&[
                Request {
                    traversals: &[true],
                    is_finished: true
                },
                Request {
                    traversals: &[true],
                    is_finished: false
                }
            ]),
            1
        );

        // traversed twice, same request, request finished
        assert_eq!(
            ref_count(&[Request {
                traversals: &[true, true],
                is_finished: true
            }]),
            0
        );

        // traversed twice, same request, block available after missing, request finished
        assert_eq!(
            ref_count(&[Request {
                traversals: &[false, true],
                is_finished: true
            }]),
            0
        );
    }

    #[test]
    fn finish_request() {
        struct Traversal<'a> {
            link: &'a Cid,
            block_is_present: bool,
        }

        let link1 = test_utils::random_cid();
        let link2 = test_utils::random_cid();
        let request_id = 0;

        let all_blocks_present = |traversals: &[Traversal<'_>]| {
            let mut link_tracker = LinkTracker::new();
            for &Traversal {
                link,
                block_is_present,
            } in traversals
            {
                link_tracker.record_link_traversal(request_id, link.clone(), block_is_present);
            }
            link_tracker.finish_request(request_id)
        };

        // block is missing
        assert!(!all_blocks_present(&[
            Traversal {
                link: &link1,
                block_is_present: true
            },
            Traversal {
                link: &link2,
                block_is_present: false
            }
        ]));

        // all blocks are present
        assert!(all_blocks_present(&[Traversal {
            link: &link1,
            block_is_present: true
        }]));

        // block becomes available after being missing
        assert!(!all_blocks_present(&[
            Traversal {
                link: &link1,
                block_is_present: false
            },
            Traversal {
                link: &link1,
                block_is_present: true
            }
        ]));
    }

    #[test]
    fn is_known_missing_link() {
        let is_known_missing_link = |traversals: &[bool]| -> bool {
            let mut link_tracker = LinkTracker::new();
            let request_id = 0;
            let link = test_utils::random_cid();

            for &block_is_present in traversals {
                link_tracker.record_link_traversal(request_id, link.clone(), block_is_present);
            }
            link_tracker.is_known_missing_link(request_id, &link)
        };

        // no traversals
        assert!(!is_known_missing_link(&[]));

        // traversed once, block present
        assert!(!is_known_missing_link(&[true]));

        // traversed once, block missing
        assert!(is_known_missing_link(&[false]));

        // traversed twice, missing then found
        assert!(is_known_missing_link(&[false, true]));
    }
}
