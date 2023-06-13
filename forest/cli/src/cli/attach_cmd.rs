// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fs::{canonicalize, read_to_string, OpenOptions},
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
use forest_chain_sync::SyncStage;
use forest_json::message::json::MessageJson;
use forest_rpc_api::mpool_api::MpoolPushMessageResult;
use forest_rpc_client::*;
use forest_shim::{address::Address, message::Message_v3};
use fvm_shared::{clock::ChainEpoch, METHOD_SEND};
use rustyline::{config::Config as RustyLineConfig, EditMode, Editor};
use serde::Serialize;
use serde_json::Value as JsonValue;
use tokio::time;

use super::Config;
use crate::humantoken;

#[derive(Debug, clap::Args)]
pub struct AttachCommand {
    /// Set a library directory for the JavaScript scripts
    #[arg(long)]
    jspath: Option<PathBuf>,

    /// Execute JavaScript code non-interactively
    #[arg(long)]
    exec: Option<String>,
}

const PRELUDE_PATH: &str = include_str!("./js/prelude.js");

fn set_module(context: &mut Context) {
    let module = JsObject::default();
    module
        .set("exports", JsObject::default(), false, context)
        .unwrap();
    context.register_global_property("module", JsValue::from(module), Attribute::default());
}

fn to_position(err: ParseError) -> Option<(u32, u32)> {
    match err {
        ParseError::Expected {
            expected: _,
            found: _,
            span,
            context: _,
        } => Some((span.start().line_number(), span.start().column_number())),
        ParseError::Unexpected {
            found: _,
            span,
            message: _,
        } => Some((span.start().line_number(), span.start().column_number())),
        ParseError::General {
            message: _,
            position,
        } => Some((position.line_number(), position.column_number())),
        ParseError::Unimplemented {
            message: _,
            position,
        } => Some((position.line_number(), position.column_number())),
        _ => None,
    }
}

fn eval(code: &str, context: &mut Context) {
    match context.eval(code) {
        Ok(v) => match v {
            JsValue::Undefined => (),
            _ => println!("{}", v.display()),
        },
        Err(v) => {
            let msg = v.to_string(context).expect("to_string must succeed");
            eprintln!("Uncaught {msg}");
        }
    }
}

fn require(
    _: &JsValue,
    params: &[JsValue],
    context: &mut Context,
    jspath: &Option<PathBuf>,
) -> JsResult<JsValue> {
    let param = if let Some(p) = params.first() {
        p
    } else {
        return context.throw_error("expecting string argument");
    };

    // Resolve module path
    let module_name = param.to_string(context)?.to_string();
    let mut path = if let Some(path) = jspath {
        path.join(module_name)
    } else {
        PathBuf::from(module_name)
    };
    // Check if path does not exist and append .js if file has no extension
    if !path.exists() && path.extension().is_none() {
        path.set_extension("js");
    }
    let result = if path.exists() {
        read_to_string(path.clone())
    } else if path == PathBuf::from("prelude.js") {
        Ok(PRELUDE_PATH.into())
    } else {
        return context.throw_error("expecting valid module path");
    };
    match result {
        Ok(buffer) => {
            if let Err(err) = context.parse(&buffer) {
                let canonical_path = canonicalize(path.clone()).unwrap_or(path.clone());
                eprintln!("{}", canonical_path.display());

                if let Some((line, column)) = to_position(err) {
                    // Display a few lines for context
                    const MAX_WINDOW: usize = 3;
                    let start_index = 0.max(line as isize - MAX_WINDOW as isize) as usize;
                    let window_len = line as usize - start_index;
                    for l in buffer.split('\n').skip(start_index).take(window_len) {
                        println!("{l}");
                    }
                    // Column is always strictly superior to zero
                    println!("{}^", " ".to_owned().repeat(column as usize - 1));
                }
                println!();
            }
            context.eval(&buffer)?;

            // Access module.exports and return as ResultValue
            let global_obj = context.global_object().to_owned();
            let module = global_obj.get("module", context).expect("get must succeed");
            let exports = module
                .as_object()
                .expect("as_object must succeed")
                .get("exports", context);

            // Reset module to avoid side effects
            set_module(context);
            exports
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

    let value = humantoken::parse(&value)?;

    let message = Message_v3 {
        from: Address::from_str(&from)?.into(),
        to: Address::from_str(&to)?.into(),
        value: value.into(), // Convert forest_shim::TokenAmount to TokenAmount3
        method_num: METHOD_SEND,
        gas_limit: 0,
        ..Default::default()
    };

    let json_message = MessageJson(message.into());
    mpool_push_message((json_message, None), auth_token).await
}

type SleepParams = (u64,);
type SleepResult = ();

async fn sleep(
    params: SleepParams,
    _auth_token: &Option<String>,
) -> Result<SleepResult, jsonrpc_v2::Error> {
    let secs = params.0;
    time::sleep(time::Duration::from_secs(secs)).await;
    Ok(())
}

type SleepTipsetsParams = (ChainEpoch,);
type SleepTipsetsResult = ();

async fn sleep_tipsets(
    params: SleepTipsetsParams,
    auth_token: &Option<String>,
) -> Result<SleepTipsetsResult, jsonrpc_v2::Error> {
    let mut epoch = None;
    loop {
        let state = sync_status((), auth_token).await?;
        if state.active_syncs[0].stage() == SyncStage::Complete {
            if let Some(prev) = epoch {
                let curr = state.active_syncs[0].epoch();
                if (curr - prev) >= params.0 {
                    return Ok(());
                }
            } else {
                epoch = Some(state.active_syncs[0].epoch());
            }
        }
        time::sleep(time::Duration::from_secs(1)).await;
    }
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
        set_module(context);

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

        // Common API
        bind_func!(context, token, version);
        bind_func!(context, token, shutdown);

        // Bind send_message, sleep, sleep_tipsets
        bind_func!(context, token, send_message);
        bind_func!(context, token, sleep);
        bind_func!(context, token, sleep_tipsets);
    }

    fn import_prelude(&self, context: &mut Context) -> anyhow::Result<()> {
        const INIT: &str = r"
            const Prelude = require('prelude.js')
            if (Prelude.showPeers) { showPeers = Prelude.showPeers; }
            if (Prelude.getPeer) { getPeer = Prelude.getPeer; }
            if (Prelude.disconnectPeers) { disconnectPeers = Prelude.disconnectPeers; }
            if (Prelude.isPeerConnected) { isPeerConnected = Prelude.isPeerConnected; }
            if (Prelude.showWallet) { showWallet = Prelude.showWallet; }
            if (Prelude.showSyncStatus) { showSyncStatus = Prelude.showSyncStatus; }
            if (Prelude.sendFIL) { sendFIL = Prelude.sendFIL; }
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

        self.import_prelude(&mut context)?;

        // If only a short execution was requested, evaluate and return
        if let Some(code) = &self.exec {
            eval(code.trim_end(), &mut context);
            return Ok(());
        }

        eval("Prelude.greet()", &mut context);

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
                        editor.add_history_entry(&buffer);
                        eval(buffer.trim_end(), &mut context);
                        break;
                    }
                    Err(err) => {
                        match err {
                            ParseError::Lex { err: _ } | ParseError::AbruptEnd => {
                                // Continue reading input and append it to buffer
                                buffer.push('\n');
                                prompt = ">> ";
                            }
                            _ => {
                                eprintln!("Uncaught ParseError: {err}");
                                break;
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
