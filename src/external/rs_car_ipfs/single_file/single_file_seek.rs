use super::super::super::rs_car::{CarReader, Cid};
use futures::{
    AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt, StreamExt,
};
use std::{collections::HashMap, io::SeekFrom};

use super::super::pb::{FlatUnixFs, UnixFsType};

use super::{
    util::{assert_header_single_file, links_to_cids},
    ReadSingleFileError,
};

/// Read CAR stream from `car_input` as a single file without buffering the block dag in memory,
/// reading de-duplicated blocks from `out`.
///
/// # Examples
///
/// ```ignore
/// use rs_car_ipfs::{Cid, single_file::read_single_file_seek};
/// use futures::io::Cursor;
///
/// #[async_std::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///   let mut input = async_std::fs::File::open("tests/example.car").await?;
///   let mut out = async_std::fs::File::create("tests/data/helloworld.txt").await?;
///   let root_cid = Cid::try_from("QmUU2HcUBVSXkfWPUc3WUSeCMrWWeEJTuAgR9uyWBhh9Nf")?;
///
///   read_single_file_seek(&mut input, &mut out, Some(&root_cid)).await?;
///   Ok(())
/// }
/// ```
pub async fn read_single_file_seek<
    R: AsyncRead + Send + Unpin,
    W: AsyncSeek + AsyncRead + AsyncWrite + Unpin,
>(
    car_input: &mut R,
    out: &mut W,
    root_cid: Option<&Cid>,
) -> Result<(), ReadSingleFileError> {
    let mut streamer = CarReader::new(car_input, true).await?;

    // Optional verification of the root_cid
    let root_cid = assert_header_single_file(&streamer.header, root_cid)?;

    // In-memory buffer of nodes, except the data contents of data nodes
    let mut nodes = HashMap::new();
    let mut sorted_links = SortedLinks::new(root_cid);
    let mut out_ptr = 0;

    while let Some(item) = streamer.next().await {
        let (cid, block) = item?;

        let inner = FlatUnixFs::try_from(block.as_slice())
            .map_err(|err| ReadSingleFileError::InvalidUnixFs(err.to_string()))?;

        // Check that the root CID is a file for sanity
        if cid == root_cid && inner.data.Type != UnixFsType::File {
            return Err(ReadSingleFileError::RootCidIsNotFile);
        }

        let node = if inner.links.is_empty() {
            // Leaf data node
            // - Only write nodes that are the next possible write
            // - If the CID of the data node is not known, discard
            // - If the CID of the node is known but is not the first, error
            match sorted_links.find(cid) {
                FindResult::IsNext => {} // Ok
                // This check is unnecessary for correctness but would allow to detect
                // a corrupt CAR stream. Otherwise this function would error with PendingLinksAtEOF
                FindResult::NotNext => return Err(ReadSingleFileError::DataNodesNotSorted),
                FindResult::Unknown => continue,
            }

            let data = inner.data.Data.ok_or(ReadSingleFileError::InvalidUnixFs(
                "unixfs data node has not Data field".to_string(),
            ))?;

            // Write data now, and keep a record for potential future writes
            out.write_all(&data).await?;

            // Wrote `cid` advance write ptr and sorted links pointer
            let size = data.len();
            let start = out_ptr;
            out_ptr += size;
            sorted_links.advance()?;

            UnixFsNode::DataPtr { start, size }
        } else {
            // Intermediary node (links)
            UnixFsNode::Links(links_to_cids(&inner.links)?)
        };

        nodes.insert(cid, node);

        // Attempt to progress on potential pending nodes
        // See module docs for a more detailed explanation
        while let Some(first) = sorted_links.first() {
            match nodes.get(first) {
                // Next node in the file layout is an existing node of already written data.
                // Use AsyncSeek to read from disk and write into new location
                Some(UnixFsNode::DataPtr { start, size }) => {
                    copy_from_to_itself(out, *start, out_ptr, *size).await?;

                    // Wrote `cid` advance write ptr and sorted links pointer
                    out_ptr += size;
                    sorted_links.advance()?;
                }
                // Next node in the file layout is an existing links node, apply insert_replace
                Some(UnixFsNode::Links(links)) => {
                    sorted_links.insert_replace(&first.clone(), links.clone())
                }
                // Next node is not yet known, continue
                None => break,
            }
        }
    }

    match sorted_links.remaining() {
        Some(links) => Err(ReadSingleFileError::PendingLinksAtEOF(links.to_vec())),
        None => Ok(()),
    }
}

/// Tracks the unixfs links progressively building the linear layout of the target file
/// New links are inserted in place recursively expanding the tree to its leafs.
struct SortedLinks<T: PartialEq + Clone> {
    pub sorted_items: Vec<T>,
    items_ptr: usize,
}

impl<T: PartialEq + Clone> SortedLinks<T> {
    fn new(root: T) -> Self {
        Self {
            sorted_items: vec![root],
            items_ptr: 0,
        }
    }

    fn find(&self, item: T) -> FindResult {
        match self
            .sorted_items
            .iter()
            .skip(self.items_ptr)
            // Note: position index is relative to the skipped elements
            .position(|x| *x == item)
        {
            Some(0) => FindResult::IsNext,
            Some(_) => FindResult::NotNext,
            None => FindResult::Unknown,
        }
    }

    fn first(&self) -> Option<&T> {
        self.sorted_items.get(self.items_ptr)
    }

    fn advance(&mut self) -> Result<(), ReadSingleFileError> {
        // items_ptr max value is the Vec len() to signal that all items are consumed
        if self.items_ptr >= self.sorted_items.len() {
            return Err(ReadSingleFileError::InternalError(
                "attempting to increase items_ptr beyond items length".to_string(),
            ));
        }

        self.items_ptr += 1;

        Ok(())
    }

    fn remaining(&self) -> Option<&[T]> {
        if self.items_ptr >= self.sorted_items.len() {
            None
        } else {
            Some(self.sorted_items.split_at(self.items_ptr).1)
        }
    }

    /// Replace the item of `root` with `children`
    fn insert_replace(&mut self, root: &T, children: Vec<T>) {
        if let Some(index) = self.sorted_items.iter().position(|x| x == root) {
            self.sorted_items.splice(index..index + 1, children);
        }
    }
}

enum FindResult {
    IsNext,
    NotNext,
    Unknown,
}

enum UnixFsNode {
    Links(Vec<Cid>),
    DataPtr { start: usize, size: usize },
}

async fn copy_from_to_itself<W: AsyncSeek + AsyncRead + AsyncWrite + Unpin>(
    r: &mut W,
    src_offset: usize,
    dest_offset: usize,
    size: usize,
) -> Result<(), std::io::Error> {
    r.seek(SeekFrom::Start(src_offset as u64)).await?;

    let mut buffer = vec![0; size];
    r.read_exact(&mut buffer).await?;

    r.seek(SeekFrom::Start(dest_offset as u64)).await?;

    r.write_all(&buffer).await?;

    Ok(())
}
