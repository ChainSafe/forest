use super::super::super::rs_car::{CarReader, Cid};
use futures::{AsyncRead, AsyncWrite, AsyncWriteExt, StreamExt};
use std::collections::HashMap;

use super::super::pb::{FlatUnixFs, UnixFsType};

use super::{
    util::{assert_header_single_file, links_to_cids},
    ReadSingleFileError,
};

/// Read CAR stream from `car_input` as a single file buffering the block dag in memory
///
/// # Examples
///
/// ```ignore
/// use rs_car_ipfs::{Cid, single_file::read_single_file_buffer};
/// use futures::io::Cursor;
///
/// #[async_std::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///   let mut input = async_std::fs::File::open("tests/example.car").await?;
///   let mut out = async_std::fs::File::create("tests/data/helloworld.txt").await?;
///   let root_cid = Cid::try_from("QmUU2HcUBVSXkfWPUc3WUSeCMrWWeEJTuAgR9uyWBhh9Nf")?;
///   let max_buffer = 10_000_000; // 10MB
///
///   read_single_file_buffer(&mut input, &mut out, Some(&root_cid), Some(max_buffer)).await?;
///   Ok(())
/// }
/// ```
pub async fn read_single_file_buffer<R: AsyncRead + Send + Unpin, W: AsyncWrite + Unpin>(
    car_input: &mut R,
    out: &mut W,
    root_cid: Option<&Cid>,
    max_buffer: Option<usize>,
) -> Result<(), ReadSingleFileError> {
    let mut streamer = CarReader::new(car_input, true).await?;

    // Optional verification of the root_cid
    let root_cid = assert_header_single_file(&streamer.header, root_cid)?;

    // In-memory buffer of data nodes
    let mut nodes = HashMap::new();
    let mut buffered_data_len: usize = 0;

    // Can the same data block be referenced multiple times? Say in a file with lots of duplicate content

    while let Some(item) = streamer.next().await {
        let (cid, block) = item?;

        let inner = FlatUnixFs::try_from(block.as_slice())
            .map_err(|err| ReadSingleFileError::InvalidUnixFs(err.to_string()))?;

        // Check that the root CID is a file for sanity
        if cid == root_cid && inner.data.Type != UnixFsType::File {
            return Err(ReadSingleFileError::RootCidIsNotFile);
        }

        if inner.links.is_empty() {
            // Leaf data node
            let data = inner.data.Data.ok_or(ReadSingleFileError::InvalidUnixFs(
                "UnixFS data node has not Data field".to_string(),
            ))?;

            // Allow to limit max buffered data to prevent OOM
            if let Some(max_buffer) = max_buffer {
                buffered_data_len += data.len();
                if buffered_data_len > max_buffer {
                    return Err(ReadSingleFileError::MaxBufferedData(max_buffer));
                }
            }

            nodes.insert(cid, UnixFsNode::Data(data.to_vec()));
        } else {
            // Intermediary node (links)
            nodes.insert(cid, UnixFsNode::Links(links_to_cids(&inner.links)?));
        };
    }

    for data in flatten_tree(&nodes, &root_cid)? {
        out.write_all(data).await?
    }

    Ok(())
}

fn flatten_tree<'a>(
    nodes: &'a HashMap<Cid, UnixFsNode>,
    root_cid: &Cid,
) -> Result<Vec<&'a Vec<u8>>, ReadSingleFileError> {
    let node = nodes
        .get(root_cid)
        .ok_or(ReadSingleFileError::MissingNode(*root_cid))?;

    Ok(match node {
        UnixFsNode::Data(data) => vec![data],
        UnixFsNode::Links(links) => {
            let mut out = vec![];
            for link in links {
                for data in flatten_tree(nodes, link)? {
                    out.push(data);
                }
            }
            out
        }
    })
}

enum UnixFsNode {
    Links(Vec<Cid>),
    Data(Vec<u8>),
}
