use cid::Cid;

type Deferred = Vec<u8>;

pub struct Node<'a> {
    _bmap: Vec<u8>,
    _links: Vec<Cid>,
    _values: Vec<Deferred>, // TODO switch to pointer if necessary

    _exp_links: Vec<Cid>,
    _exp_vals: Vec<Deferred>,
    _cache: Vec<&'a Node<'a>>,
}
