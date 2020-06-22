// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use blockstore::BlockStore;
use crypto::{Signature, SignatureType};
use encoding::Cbor;
use message::{SignedMessage, UnsignedMessage};
use num_bigint::BigUint;
use state_manager::StateManager;
use thiserror::Error;
use wallet::{KeyInfo, KeyStore, Wallet};

#[derive(Debug, Error, PartialEq)]
pub enum Error {
    #[error("{0}")]
    WalletError(String),
}

pub struct WalletApi<DB, T> {
    state_manager: StateManager<DB>,
    wallet: Wallet<T>,
}

impl<DB, T> WalletApi<DB, T>
where
    DB: BlockStore,
    T: KeyStore,
{
    pub fn new(state_manager: StateManager<DB>, wallet: Wallet<T>) -> Self {
        WalletApi {
            state_manager,
            wallet,
        }
    }
    pub fn new_addr(&mut self, typ: SignatureType) -> Result<Address, Error> {
        self.wallet
            .generate_key(typ)
            .map_err(|err| Error::WalletError(err.to_string()))
    }

    pub fn wallet_has_key(&mut self, addr: &Address) -> bool {
        self.wallet.has_key(addr)
    }

    pub fn wallet_list_addrs(&mut self) -> Result<Vec<Address>, Error> {
        self.wallet
            .list_addrs()
            .map_err(|err| Error::WalletError(err.to_string()))
    }

    pub fn wallet_balance(&mut self, addr: &Address) -> Result<BigUint, Error> {
        self.state_manager
            .get_heaviest_balance(addr)
            .map_err(|err| Error::WalletError(err.to_string()))
    }

    pub fn wallet_sign(&mut self, addr: &Address, msg: &[u8]) -> Result<Signature, Error> {
        self.wallet
            .sign(addr, msg)
            .map_err(|err| Error::WalletError(err.to_string()))
    }

    pub fn wallet_sign_message(
        &mut self,
        addr: &Address,
        msg: &UnsignedMessage,
    ) -> Result<SignedMessage, Error> {
        let msg_cid = msg
            .cid()
            .map_err(|err| Error::WalletError(err.to_string()))?;
        let sig = self.wallet_sign(addr, &msg_cid.to_bytes())?;
        Ok(SignedMessage::new_from_fields(msg.clone(), sig))
    }

    pub fn wallet_verify(&mut self, addr: &Address, msg: &[u8], sig: Signature) -> bool {
        sig.verify(msg, addr).is_ok()
    }

    pub fn wallet_default(&self) -> Result<Address, Error> {
        self.wallet
            .get_default()
            .map_err(|err| Error::WalletError(err.to_string()))
    }

    pub fn wallet_set_default(&mut self, addr: Address) -> Result<(), Error> {
        self.wallet
            .set_default(addr)
            .map_err(|err| Error::WalletError(err.to_string()))
    }

    pub fn wallet_export(&mut self, addr: &Address) -> Result<KeyInfo, Error> {
        self.wallet
            .export(addr)
            .map_err(|err| Error::WalletError(err.to_string()))
    }

    pub fn wallet_import(&mut self, key_info: KeyInfo) -> Result<Address, Error> {
        self.wallet
            .import(key_info)
            .map_err(|err| Error::WalletError(err.to_string()))
    }
}
