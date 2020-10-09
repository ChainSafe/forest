use super::BlockStore;
use cid::Cid;
use forest_ipld::Ipld;
use std::error::Error as StdError;

/// Resolves link to recursively resolved Ipld with no hash links.
pub fn resolve_cids_recursive<BS>(bs: &BS, cid: &Cid) -> Result<Ipld, Box<dyn StdError>>
where
    BS: BlockStore,
{
    let mut ipld = bs
        .get(cid)?
        .ok_or_else(|| "Cid does not exist in blockstore")?;

    resolve_ipld(bs, &mut ipld)?;

    Ok(ipld)
}

/// Resolves Ipld links recursively, building an Ipld structure with no hash links.
pub fn resolve_ipld<BS>(bs: &BS, ipld: &mut Ipld) -> Result<(), Box<dyn StdError>>
where
    BS: BlockStore,
{
    match ipld {
        Ipld::Map(m) => {
            for (_, v) in m.iter_mut() {
                resolve_ipld(bs, v)?;
            }
        }
        Ipld::List(list) => {
            for v in list.iter_mut() {
                resolve_ipld(bs, v)?;
            }
        }
        link @ Ipld::Link(_) => {
            let resolved: Option<Ipld> = if let Ipld::Link(cid) = link {
                bs.get(cid)?
            } else {
                unreachable!()
            };

            if let Some(ipld) = resolved {
                *link = ipld;
            }
        }
        _ => (),
    }
    Ok(())
}
