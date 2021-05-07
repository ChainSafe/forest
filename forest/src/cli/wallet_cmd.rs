use forest_crypto::signature::{json::signature_type::SignatureTypeJson, SignatureType};
use rpc_client::{new_client, wallet_ops};
use structopt::StructOpt;

use super::stringify_rpc_err;

#[derive(Debug, StructOpt)]
pub enum WalletCommands {
    #[structopt(about = "Create a new wallet")]
    New {
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
            Self::New { signature_type } => {
                let signature_type = match signature_type.as_str() {
                    "secp256k1" => SignatureType::Secp256k1,
                    _ => SignatureType::BLS,
                };

                let signature_type_json = SignatureTypeJson(signature_type);

                let mut client = new_client();

                let obj = wallet_ops::wallet_new(&mut client, signature_type_json)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", obj);
            }
        }
    }
}
