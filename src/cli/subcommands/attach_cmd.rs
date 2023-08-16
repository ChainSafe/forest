// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fs::{canonicalize, read_to_string, OpenOptions},
    path::PathBuf,
    str::FromStr,
};

use boa_engine::{
    object::{builtins::JsArray, FunctionObjectBuilder},
    prelude::JsObject,
    property::Attribute,
    Context, JsError, JsResult, JsValue, NativeFunction, Source,
};
use boa_interner::Interner;
use boa_parser::Parser;
use boa_runtime::Console;
use convert_case::{Case, Casing};
use directories::BaseDirs;
use rustyline::{config::Config as RustyLineConfig, history::FileHistory, EditMode, Editor};
use serde::Serialize;
use serde_json::Value as JsonValue;
use tokio::time;

use super::Config;
use crate::chain_sync::SyncStage;
use crate::cli::humantoken;
use crate::json::message::json::MessageJson;
use crate::rpc_api::mpool_api::MpoolPushMessageResult;
use crate::rpc_client::node_ops::node_status;
use crate::rpc_client::*;
use crate::shim::{address::Address, clock::ChainEpoch, message::Message};

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
    context
        .register_global_property("module", JsValue::from(module), Attribute::default())
        .expect("`register_global_property` should not fail");
}

fn to_position(err: boa_parser::Error) -> Option<(u32, u32)> {
    use boa_parser::Error::*;

    match err {
        Expected {
            expected: _,
            found: _,
            span,
            context: _,
        } => Some((span.start().line_number(), span.start().column_number())),
        Unexpected {
            found: _,
            span,
            message: _,
        } => Some((span.start().line_number(), span.start().column_number())),
        General {
            message: _,
            position,
        } => Some((position.line_number(), position.column_number())),
        Lex { err: _ } | AbruptEnd => None,
    }
}

fn eval(code: &str, context: &mut Context) {
    match context.eval(Source::from_bytes(code)) {
        Ok(v) => match v {
            JsValue::Undefined => (),
            _ => println!("{}", v.display()),
        },
        Err(err) => {
            eprintln!("Uncaught {err}");
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
        return Err(JsError::from_opaque("expecting string argument".into()));
    };

    // Resolve module path
    let module_name = param.to_string(context)?.to_std_string_escaped();
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
        return Err(JsError::from_opaque("expecting valid module path".into()));
    };
    match result {
        Ok(buffer) => {
            let mut parser = Parser::new(Source::from_bytes(&buffer));
            let mut interner = Interner::new();
            if let Err(err) = parser.parse_eval(true, &mut interner) {
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
            context.eval(Source::from_bytes(&buffer))?;

            // Access module.exports and return as ResultValue
            let global_obj = context.global_object().to_owned();
            let module = global_obj.get("module", context).expect("get must succeed");
            let exports = module
                .as_object()
                .expect("as_object must succeed")
                .get("exports", context);

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
        let js_func = FunctionObjectBuilder::new($context, unsafe {
            NativeFunction::from_closure_with_captures(
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
        })
        .name(js_func_name.clone())
        .build();

        let attr = Attribute::WRITABLE | Attribute::NON_ENUMERABLE | Attribute::CONFIGURABLE;
        $context
            .register_global_property(js_func_name, js_func, attr)
            .expect("`register_global_property` should not fail");
    };
}

type SendMessageParams = (String, String, String);

async fn send_message(
    params: SendMessageParams,
    auth_token: &Option<String>,
) -> Result<MpoolPushMessageResult, jsonrpc_v2::Error> {
    let (from, to, value) = params;

    let message = Message::transfer(
        Address::from_str(&from)?,
        Address::from_str(&to)?,
        humantoken::parse(&value)?, // Convert forest_shim::TokenAmount to TokenAmount3
    );

    let json_message = MessageJson(message);
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
        let console = Console::init(context);
        context
            .register_global_property(Console::NAME, console, Attribute::all())
            .expect("the console object shouldn't exist yet");
        context
            .register_global_property("_BOA_VERSION", "0.17.0", Attribute::default())
            .expect("`register_global_property` should not fail");

        // Add custom implementation that mimics `require`
        let require_func = unsafe {
            NativeFunction::from_closure_with_captures(
                |_this, params, jspath, context| require(_this, params, context, jspath),
                self.jspath.clone(),
            )
        };

        context
            .register_global_builtin_callable("require", 1, require_func)
            .expect("Registering the global`require` should succeed");

        // Add custom object that mimics `module.exports`
        set_module(context);

        // Chain API
        bind_func!(context, token, chain_get_name);

        // Net API
        bind_func!(context, token, net_addrs_listen);
        bind_func!(context, token, net_peers);
        bind_func!(context, token, net_disconnect);
        bind_func!(context, token, net_connect);

        // Node API
        bind_func!(context, token, node_status);

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

        if let Err(err) = context.eval(Source::from_bytes(INIT)) {
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

        let mut editor: Editor<(), FileHistory> = Editor::with_config(config)?;

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
                            editor.clear_history()?;
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

                let mut parser = Parser::new(Source::from_bytes(&buffer));
                let mut interner = Interner::new();
                match parser.parse_eval(true, &mut interner) {
                    Ok(_) => {
                        editor.add_history_entry(&buffer)?;
                        eval(buffer.trim_end(), &mut context);
                        break;
                    }
                    Err(err) => {
                        match err {
                            boa_parser::Error::Lex { err: _ } | boa_parser::Error::AbruptEnd => {
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
