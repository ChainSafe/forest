// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fs::{read_to_string, OpenOptions},
    path::PathBuf,
    str::FromStr,
};

use boa_engine::{
    object::{FunctionBuilder, JsArray},
    prelude::JsObject,
    property::Attribute,
    syntax::parser::ParseError,
    Context, JsResult, JsValue,
};
use convert_case::{Case, Casing};
use directories::BaseDirs;
use forest_json::message::json::MessageJson;
use forest_rpc_api::mpool_api::MpoolPushMessageResult;
use forest_rpc_client::*;
use fvm_shared::{address::Address, econ::TokenAmount, message::Message, METHOD_SEND};
use num::{BigInt, Zero};
use rustyline::{config::Config as RustyLineConfig, EditMode, Editor};
use serde::Serialize;
use serde_json::Value as JsonValue;

use super::Config;

#[derive(Debug, clap::Args)]
pub struct AttachCommand {
    /// Set a library directory for the Javascript scripts
    #[arg(long)]
    jspath: Option<PathBuf>,

    /// Execute Javascript code non-interactively
    #[arg(long)]
    exec: Option<String>,
}

const PRELUDE_MODULE: &str = include_str!("./js/prelude.js");

fn require(
    _: &JsValue,
    params: &[JsValue],
    context: &mut Context,
    jspath: &Option<PathBuf>,
) -> JsResult<JsValue> {
    if params.is_empty() {
        return context.throw_error("expecting string argument");
    }
    let param = params.get(0).unwrap();

    let module_name = param.to_string(context)?.to_string();
    let path = if let Some(path) = jspath {
        path.join(module_name)
    } else {
        PathBuf::from(module_name)
    };

    let result = if path.exists() {
        read_to_string(path)
    } else {
        Ok(PRELUDE_MODULE.into())
    };
    match result {
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
    fn setup_context(&self, context: &mut Context, token: &Option<String>) {
        // Disable tracing
        context.set_trace(false);

        context.register_global_property("_BOA_VERSION", "0.16.0", Attribute::default());

        // Add custom implementation that mimics `require`
        let require_func = FunctionBuilder::closure_with_captures(
            context,
            |_this, params, jspath, context| require(_this, params, context, jspath),
            self.jspath.clone(),
        )
        .build();
        let attr = Attribute::WRITABLE | Attribute::NON_ENUMERABLE | Attribute::CONFIGURABLE;
        context.register_global_property("require", require_func, attr);

        // Add custom object that mimics `module.exports`
        let moduleobj = JsObject::default();
        moduleobj
            .set("exports", JsValue::from(" "), false, context)
            .unwrap();
        context.register_global_property("module", JsValue::from(moduleobj), Attribute::default());

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
        // TODO: bind wallet_sign, wallet_verify
        bind_func!(context, token, wallet_new);
        bind_func!(context, token, wallet_default_address);
        bind_func!(context, token, wallet_balance);
        bind_func!(context, token, wallet_export);
        bind_func!(context, token, wallet_import);
        bind_func!(context, token, wallet_list);
        bind_func!(context, token, wallet_has);
        bind_func!(context, token, wallet_set_default);

        // Message Pool API
        bind_func!(context, token, mpool_push_message);

        // Bind send_message
        bind_func!(context, token, send_message);
    }

    fn import_prelude(&self, context: &mut Context) -> anyhow::Result<()> {
        const INIT: &str = r"
            const prelude = require('prelude.js')
            prelude.greet();
            if (prelude.showPeers) { showPeers = prelude.showPeers; }
            if (prelude.getPeer) { getPeer = prelude.getPeer; }
            if (prelude.disconnectPeers) { disconnectPeers = prelude.disconnectPeers; }
            if (prelude.isPeerConnected) { isPeerConnected = prelude.isPeerConnected; }
            if (prelude.showWallet) { showWallet = prelude.showWallet; }
            if (prelude.showSyncStatus) { showSyncStatus = prelude.showSyncStatus; }
            if (prelude.sendFIL) { sendFIL = prelude.sendFIL; }
        ";
        let result = context.eval(INIT);
        if let Err(err) = result {
            return Err(anyhow::anyhow!("error {err:?}"));
        }

        Ok(())
    }

    pub fn run(&self, config: Config) -> anyhow::Result<()> {
        let mut context = Context::default();
        self.setup_context(&mut context, &config.client.rpc_token);

        // If only a short execution was requested, evaluate and return
        if let Some(code) = &self.exec {
            match context.eval(code.trim_end()) {
                Ok(v) => match v {
                    JsValue::Undefined => (),
                    _ => println!("{}", v.display()),
                },
                Err(v) => eprintln!("Uncaught: {v:?}"),
            }
            return Ok(());
        }

        self.import_prelude(&mut context)?;

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
