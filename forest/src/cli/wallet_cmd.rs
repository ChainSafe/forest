use forest_crypto::signature::{json::signature_type::SignatureTypeJson, SignatureType};
use jsonrpc_v2::Data;
use rpc::wallet_api;
use rpc_client::new_client;
use std::path::PathBuf;
use structopt::StructOpt;

use super::stringify_rpc_err;

#[derive(Debug, StructOpt)]
pub enum WalletCommands {
    #[structopt(about = "Create a new wallet")]
    New {
        #[structopt(short, help = "path to forest-db directory")]
        path: String,
        #[structopt(
            short,
            help = "The signature type to use. One of Secp256k1, or BLS. Defaults to BLS"
        )]
        signature_type: String,
    },
}

impl WalletCommands {
    pub async fn run(&self) {
        match self {
            Self::New {
                path,
                signature_type,
            } => {
                let path = PathBuf::from(path);
                #[cfg(all(feature = "sled", not(feature = "rocksdb")))]
                let db = db::sled::SledDb::open(path).unwrap();

                #[cfg(feature = "rocksdb")]
                let db = db::rocks::RocksDb::open(path).unwrap();

                let db = Data::new(db);

                let signature_type = match signature_type.to_lowercase().as_str() {
                    "secp256k1" => SignatureType::Secp256k1,
                    _ => SignatureType::BLS,
                };

                let signature_type_json = SignatureTypeJson(signature_type);

                let mut client = new_client();

                let obj = wallet_api::wallet_new(db, (SignatureTypeJson,))
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", obj);
            }
            WalletCommands::New {
                path,
                signature_type,
            } => {}
        }
    }
}
