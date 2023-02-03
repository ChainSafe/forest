// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fs::{read_to_string, OpenOptions};
use std::str::FromStr;

use boa_engine::object::{FunctionBuilder, JsArray};
use boa_engine::{prelude::JsObject, property::Attribute, syntax::parser::ParseError};
use boa_engine::{Context, JsResult, JsValue};
use convert_case::{Case, Casing};
use directories::BaseDirs;
use fvm_shared::{address::Address, econ::TokenAmount, message::Message, METHOD_SEND};
use num::{BigInt, Zero};
use rustyline::{config::Config as RustyLineConfig, EditMode, Editor};
use serde::Serialize;
use serde_json::Value as JsonValue;
use structopt::StructOpt;

use super::Config;
use forest_json::message::json::MessageJson;
use forest_rpc_api::mpool_api::MpoolPushMessageResult;
use forest_rpc_client::*;

#[derive(Debug, StructOpt)]
pub struct AttachCommand {}

const ON_INIT_SCRIPT: &str = r#"
    console.log("Welcome to the Forest Javascript console!\n\nTo exit, press ctrl-d or type :quit");

    // // Load filecoin module
    // let filecoin = require("./filecoin.js");

    function showPeers() {
        let ids = netPeers().map((x) => x.ID).sort();
        for (var i = 0; i < ids.length; i++) {
            let id = ids[i];
            console.log(`${i}:\t${id}`);
        }
        console.log("");
    }

    function getPeer(peerID) {
        return netPeers().find((x) => x.ID == peerID);
    }

    function disconnectPeers(count) {
        let ids = netPeers().map((x) => x.ID).sort();
        // clamp
        let new_count = Math.min(count, ids.length);
        for (var i = 0; i < new_count; i++) {
            netDisconnect(ids[i]);
        }
    } 

    function isPeerConnected(peerID) {
        return netPeers().some((x) => x.ID == peerID);
    }

    function showWallet() {
        let addrs = walletList();
        let defaultAddr = walletDefaultAddress();
        let buffer = "";
        buffer = buffer.concat("Address                                         Balance\n");
        for (var i = 0; i < addrs.length; i++) {
            const isDefault = (defaultAddr == addrs[i]);
            if (isDefault) {
                buffer += "\033[1m";
            }
            buffer = buffer.concat(addrs[i], "       ", walletBalance(addrs[i]), " attoFIL\n");
            if (isDefault) {
                buffer += "\033[0m";
            }
        }
        console.log(buffer);
    }
"#;

fn require(_: &JsValue, params: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let param = params.get(0).unwrap();

    let path = param
        .to_string(context)
        .expect("Failed to convert to string")
        .to_string();

    println!("Loading: {path}");
    match read_to_string(path) {
        Ok(buffer) => {
            context.eval(&buffer).unwrap();

            // Access module.exports and return as ResultValue
            let global_obj = context.global_object().to_owned();
            let module = global_obj.get("module", context).unwrap();
            module.as_object().unwrap().get("exports", context)
        }
        Err(err) => {
            eprintln!("Error: {err}");
            Ok(JsValue::Undefined)
        }
    }
}

fn check_result<R>(context: &mut Context, result: Result<R, jsonrpc_v2::Error>) -> JsResult<JsValue>
where
    R: Serialize,
{
    match result {
        Ok(v) => {
            // TODO: check if unwrap is safe here
            let value: JsonValue = serde_json::to_value(v).unwrap();
            JsValue::from_json(&value, context)
        }
        Err(err) => {
            let message = match err {
                jsonrpc_v2::Error::Full { code, message, .. } => {
                    format!("JSON RPC Error: Code: {code}, Message: {message}")
                }
                jsonrpc_v2::Error::Provided { code, message } => {
                    format!("JSON RPC Error: Code: {code}, Message: {message}")
                }
            };
            eprintln!("Error: {message}");
            Ok(JsValue::Undefined)
        }
    }
}

macro_rules! bind_func {
    ($context:expr, $token:expr, $func:ident) => {
        let js_func_name = stringify!($func).to_case(Case::Camel);
        let js_func = FunctionBuilder::closure_with_captures(
            $context,
            |_this, params, token, context| {
                let handle = tokio::runtime::Handle::current();

                let result = tokio::task::block_in_place(|| {
                    let value = if params.is_empty() {
                        JsValue::Null
                    } else {
                        let arr = JsArray::from_iter(params.to_vec(), context);
                        let obj: JsObject = arr.into();
                        JsValue::from(obj)
                    };
                    // TODO: check if unwrap is safe here
                    let args = serde_json::from_value(value.to_json(context).unwrap())?;
                    handle.block_on($func(args, token))
                });
                check_result(context, result)
            },
            $token.clone(),
        )
        .name(js_func_name.clone())
        .build();
        let attr = Attribute::WRITABLE | Attribute::NON_ENUMERABLE | Attribute::CONFIGURABLE;
        $context.register_global_property(js_func_name, js_func, attr);
    };
}

fn setup_context(context: &mut Context, token: &Option<String>) {
    // Disable tracing
    context.set_trace(false);

    context.register_global_property("_BOA_VERSION", "0.16.0", Attribute::default());

    // Add custom implementation that mimics `require`
    context.register_global_function("require", 0, require);

    // Add custom object that mimics `module.exports`
    let moduleobj = JsObject::default();
    moduleobj
        .set("exports", JsValue::from(" "), false, context)
        .unwrap();
    context.register_global_property("module", JsValue::from(moduleobj), Attribute::default());

    context
        .eval(ON_INIT_SCRIPT)
        .expect("ON_INIT_SCRIPT script should work");

    // Chain API
    bind_func!(context, token, chain_get_name);

    // Net API
    bind_func!(context, token, net_addrs_listen);
    bind_func!(context, token, net_peers);
    bind_func!(context, token, net_disconnect);
    bind_func!(context, token, net_connect);

    // Sync API
    bind_func!(context, token, sync_check_bad);
    bind_func!(context, token, sync_mark_bad);
    bind_func!(context, token, sync_status);

    // Wallet API
    // TODO: bind wallet_export, wallet_import, wallet_has, wallet_sign, wallet_verify
    bind_func!(context, token, wallet_new);
    bind_func!(context, token, wallet_default_address);
    bind_func!(context, token, wallet_balance);
    bind_func!(context, token, wallet_list);
    bind_func!(context, token, wallet_set_default);

    // Message Pool API
    bind_func!(context, token, mpool_push_message);

    // Bind send_message
    bind_func!(context, token, send_message);
}

type SendMessageParams = (String, String, String);

async fn send_message(
    params: SendMessageParams,
    auth_token: &Option<String>,
) -> Result<MpoolPushMessageResult, jsonrpc_v2::Error> {
    let (from, to, value) = params;

    let message = Message {
        from: Address::from_str(&from)?,
        to: Address::from_str(&to)?,
        value: TokenAmount::from_atto(BigInt::from_str(&value)?),
        method_num: METHOD_SEND,
        gas_limit: 0,
        gas_fee_cap: TokenAmount::from_atto(BigInt::zero()),
        gas_premium: TokenAmount::from_atto(BigInt::zero()),
        ..Default::default()
    };

    let json_message = MessageJson(message);
    mpool_push_message((json_message, None), auth_token).await
}

impl AttachCommand {
    pub fn run(&self, config: Config) -> anyhow::Result<()> {
        let mut context = Context::default();
        setup_context(&mut context, &config.client.rpc_token);

        let config = RustyLineConfig::builder()
            .keyseq_timeout(1)
            .edit_mode(EditMode::Emacs)
            .build();

        let mut editor: Editor<()> = Editor::with_config(config)?;

        let history_path = if let Some(dirs) = BaseDirs::new() {
            let path = dirs.home_dir().join(".forest_history");

            // Check if the history file exists. If it does not, create it.
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&path)?;

            // This is safe to call at this point
            editor.load_history(&path).unwrap();

            Some(path)
        } else {
            None
        };

        'main: loop {
            let mut prompt = "> ";
            let mut buffer = String::new();
            loop {
                match editor.readline(prompt) {
                    Ok(input) => {
                        if input == ":quit" {
                            break 'main;
                        }
                        if input == ":clear" {
                            editor.clear_history();
                            break;
                        }
                        if buffer.is_empty() && input.is_empty() {
                            // No-op
                            continue 'main;
                        }
                        buffer.push_str(&input)
                    }
                    Err(_) => break 'main,
                }
                match context.parse(buffer.trim_end()) {
                    Ok(_v) => {
                        // println!("Parse tree:\n{:#?}", v);
                        editor.add_history_entry(&buffer);
                        match context.eval(buffer.trim_end()) {
                            Ok(v) => match v {
                                JsValue::Undefined => (),
                                _ => println!("{}", v.display()),
                            },
                            Err(v) => eprintln!("Uncaught: {v:?}"),
                        }
                        break;
                    }
                    Err(err) => {
                        match err {
                            ParseError::Expected {
                                expected,
                                found,
                                span: _,
                                context: _,
                            } => {
                                eprintln!("Expecting token {expected:?} but got {found}");
                                break 'main;
                            }
                            _ => {
                                // Continue reading input and append it to buffer
                                buffer.push('\n');
                                prompt = ">> ";
                            }
                        }
                    }
                }
            }
        }

        if let Some(path) = history_path {
            editor
                .save_history(&path)
                .expect("save_history should work");
        }

        Ok(())
    }
}
