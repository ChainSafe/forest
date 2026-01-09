// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use libp2p::Multiaddr;

pub trait MultiaddrExt: Sized {
    fn without_p2p(self) -> Self;
}

impl MultiaddrExt for Multiaddr {
    fn without_p2p(mut self) -> Self {
        if let Some(multiaddr::Protocol::P2p(_)) = self.iter().last() {
            self.pop();
            self
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr as _;

    #[test]
    fn test_without_p2p_positive() {
        let ma = Multiaddr::from_str("/dns/bootstrap-calibnet-1.chainsafe-fil.io/tcp/34000/p2p/12D3KooWS3ZRhMYL67b4bD5XQ6fcpTyVQXnDe8H89LvwrDqaSbiT").unwrap();
        assert_eq!(
            ma.without_p2p().to_string().as_str(),
            "/dns/bootstrap-calibnet-1.chainsafe-fil.io/tcp/34000"
        );
    }

    #[test]
    fn test_without_p2p_negative() {
        let ma =
            Multiaddr::from_str("/dns/bootstrap-calibnet-1.chainsafe-fil.io/tcp/34000").unwrap();
        assert_eq!(
            ma.without_p2p().to_string().as_str(),
            "/dns/bootstrap-calibnet-1.chainsafe-fil.io/tcp/34000"
        );
    }
}
