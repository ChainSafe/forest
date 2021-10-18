// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    ExtensionData, Extensions, GraphSyncResponse, MetadataItem, RequestID, ResponseStatusCode,
    EXTENSION_METADATA,
};
use cid::Cid;
use fnv::FnvHashMap;

/// ResponseBuilder captures components of a response message across multiple
/// requests for a given peer and then generates the corresponding GraphSync
/// message components once responses are ready to send.
#[derive(Default)]
pub struct ResponseBuilder {
    /// The actual blocks that will be sent to the peer.
    blocks: Vec<Vec<u8>>,

    /// The combined block size of this message, i.e. the sum of the lengths
    /// of all included blocks.
    /// Used to determine whether this message still has enough space to
    /// store a given block, or that it needs to be added to a new message.
    block_size: usize,

    /// The request IDs of the requests included in this message, as well
    /// as which blocks were present and which ones were missing.
    outgoing_responses: FnvHashMap<RequestID, Vec<MetadataItem>>,

    /// The status codes of the requests that have been completed,
    /// either `RequestCompletedFull` or `RequestCompletedPartial`.
    completed_responses: FnvHashMap<RequestID, ResponseStatusCode>,

    /// Any extension data that was added to this message for any particular request.
    extensions: FnvHashMap<RequestID, Extensions>,
}

impl ResponseBuilder {
    /// Creates a new response builder.
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns the combined block size of this message.
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Adds the given block to the message.
    pub fn add_block(&mut self, block: Vec<u8>) {
        self.block_size += block.len();
        self.blocks.push(block);
    }

    /// Adds the given link and whether its block is present to the response for
    /// the given request ID.
    pub fn add_link(&mut self, id: RequestID, link: Cid, block_is_present: bool) {
        self.outgoing_responses
            .entry(id)
            .or_default()
            .push(MetadataItem {
                link,
                block_is_present,
            })
    }

    /// Marks the given request as completed in the message, as well as whether the
    /// GraphSync request responded with complete or partial data.
    pub fn complete(&mut self, id: RequestID, code: ResponseStatusCode) {
        self.completed_responses.insert(id, code);

        // ensures that this request will be included in the actual message when
        // `build` is called, even if no other data is included for this request
        self.outgoing_responses.entry(id).or_default();
    }

    /// Returns true if there is no content to send.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.outgoing_responses.is_empty()
    }

    /// Adds the given extension data to the response.
    pub fn add_extension_data(
        &mut self,
        id: RequestID,
        ExtensionData { name, data }: ExtensionData,
    ) {
        self.extensions.entry(id).or_default().insert(name, data);
    }

    /// Assembles and encodes response data from the added requests, links, and blocks.
    pub fn build(self) -> Result<(Vec<GraphSyncResponse>, Vec<Vec<u8>>), String> {
        let mut extensions = self.extensions;
        let completed_responses = self.completed_responses;

        let responses = self
            .outgoing_responses
            .into_iter()
            .map(|(id, metadata)| {
                let metadata = forest_encoding::to_vec(&metadata).map_err(|e| e.to_string())?;
                let mut extensions = extensions.remove(&id).unwrap_or_default();
                extensions.insert(EXTENSION_METADATA.to_string(), metadata);
                let status = completed_responses
                    .get(&id)
                    .copied()
                    .unwrap_or(ResponseStatusCode::PartialResponse);

                Ok(GraphSyncResponse {
                    id,
                    status,
                    extensions,
                })
            })
            .collect::<Result<_, String>>()?;

        Ok((responses, self.blocks))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;

    #[test]
    fn message_building() {
        let mut builder = ResponseBuilder::new();

        let (data, links) = test_utils::random_blocks(3, 100);
        let request_ids = [0, 1, 2, 3];

        builder.add_link(request_ids[0], links[0].clone(), true);
        builder.add_link(request_ids[0], links[1].clone(), false);
        builder.add_link(request_ids[0], links[2].clone(), true);
        builder.complete(request_ids[0], ResponseStatusCode::RequestCompletedPartial);

        builder.add_link(request_ids[1], links[1].clone(), true);
        builder.add_link(request_ids[1], links[2].clone(), true);
        builder.add_link(request_ids[1], links[1].clone(), true);
        builder.complete(request_ids[1], ResponseStatusCode::RequestCompletedFull);

        builder.add_link(request_ids[2], links[0].clone(), true);
        builder.add_link(request_ids[2], links[1].clone(), true);

        builder.complete(request_ids[3], ResponseStatusCode::RequestCompletedFull);

        for block in &data {
            builder.add_block(block.clone());
        }

        assert_eq!(builder.block_size(), 300);

        let extension1 = ExtensionData {
            name: "AppleSauce/McGee".to_string(),
            data: test_utils::random_bytes(100),
        };

        let extension2 = ExtensionData {
            name: "HappyLand/Happenstance".to_string(),
            data: test_utils::random_bytes(100),
        };

        builder.add_extension_data(request_ids[0], extension1.clone());
        builder.add_extension_data(request_ids[2], extension2.clone());

        let (mut responses, blocks) = builder.build().unwrap();
        assert_eq!(blocks, data);
        assert_eq!(responses.len(), 4);
        responses.sort_by_key(|r| r.id);

        let (response1, response2, response3, response4) = match &responses[..] {
            [r1, r2, r3, r4] => (r1, r2, r3, r4),
            _ => panic!(),
        };

        assert_eq!(&response1.extensions[&extension1.name], &extension1.data);
        assert_eq!(&response3.extensions[&extension2.name], &extension2.data);

        assert_eq!(
            response1.status,
            ResponseStatusCode::RequestCompletedPartial
        );
        assert_eq!(response2.status, ResponseStatusCode::RequestCompletedFull);
        assert_eq!(response3.status, ResponseStatusCode::PartialResponse);
        assert_eq!(response4.status, ResponseStatusCode::RequestCompletedFull);

        assert_eq!(
            forest_encoding::from_slice::<Vec<MetadataItem>>(
                &response1.extensions[EXTENSION_METADATA]
            )
            .unwrap(),
            &[
                MetadataItem {
                    link: links[0].clone(),
                    block_is_present: true
                },
                MetadataItem {
                    link: links[1].clone(),
                    block_is_present: false
                },
                MetadataItem {
                    link: links[2].clone(),
                    block_is_present: true
                }
            ]
        );

        assert_eq!(
            forest_encoding::from_slice::<Vec<MetadataItem>>(
                &response2.extensions[EXTENSION_METADATA]
            )
            .unwrap(),
            &[
                MetadataItem {
                    link: links[1].clone(),
                    block_is_present: true
                },
                MetadataItem {
                    link: links[2].clone(),
                    block_is_present: true
                },
                MetadataItem {
                    link: links[1].clone(),
                    block_is_present: true
                }
            ]
        );

        assert_eq!(
            forest_encoding::from_slice::<Vec<MetadataItem>>(
                &response3.extensions[EXTENSION_METADATA]
            )
            .unwrap(),
            &[
                MetadataItem {
                    link: links[0].clone(),
                    block_is_present: true
                },
                MetadataItem {
                    link: links[1].clone(),
                    block_is_present: true
                },
            ]
        );
    }
}
