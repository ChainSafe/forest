// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fs::{canonicalize, read_to_string, OpenOptions},
    path::PathBuf,
    str::FromStr,
};

use crate::chain::ChainEpochDelta;
use crate::chain_sync::SyncStage;
use crate::rpc_client::*;
use crate::shim::{address::Address, message::Message};
use crate::{cli::humantoken, message::SignedMessage};
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
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value as JsonValue;
use tokio::time;

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

fn check_result<R>(context: &mut Context, result: anyhow::Result<R>) -> JsResult<JsValue>
where
    R: Serialize,
{
    match result {
        Ok(v) => {
            let value: JsonValue =
                serde_json::to_value(v).map_err(|e| JsError::from_opaque(e.to_string().into()))?;
            JsValue::from_json(&value, context)
        }
        Err(err) => {
            eprintln!("Error: {err}");
            Ok(JsValue::Undefined)
        }
    }
}

macro_rules! bind_func {
    ($context:expr, $api:expr, $func:ident) => {
        let js_func_name = stringify!($func).to_case(Case::Camel);
        let js_func = FunctionObjectBuilder::new($context, unsafe {
            {
                let api = $api.clone();
                NativeFunction::from_closure(move |_this, params, context| {
                    let handle = tokio::runtime::Handle::current();

                    let result = tokio::task::block_in_place(|| {
                        let value = if params.is_empty() {
                            JsValue::Null
                        } else {
                            let arr = JsArray::from_iter(params.to_vec(), context);
                            let obj: JsObject = arr.into();
                            JsValue::from(obj)
                        };
                        let args = serde_json::from_value(
                            value
                                .to_json(context)
                                .map_err(|e| anyhow::anyhow!(e.to_string()))?,
                        )?;
                        handle.block_on($func(args, &api))
                    });
                    check_result(context, result)
                })
            }
        })
        .name(js_func_name.clone())
        .build();

        let attr = Attribute::WRITABLE | Attribute::NON_ENUMERABLE | Attribute::CONFIGURABLE;
        $context
            .register_global_property(js_func_name, js_func, attr)
            .expect("`register_global_property` should not fail");
    };
}

fn bind_request<T: DeserializeOwned, R>(
    context: &mut Context,
    api: &ApiInfo,
    name: &'static str,
    req: impl Fn(T) -> RpcRequest<R> + 'static,
) {
    let js_func_name = name.to_case(Case::Camel);
    let js_func = FunctionObjectBuilder::new(context, unsafe {
        NativeFunction::from_closure({
            let api = api.clone();
            move |_this, params, context| {
                let handle = tokio::runtime::Handle::current();

                let result = tokio::task::block_in_place(|| {
                    let value = if params.is_empty() {
                        JsValue::Null
                    } else {
                        let arr = JsArray::from_iter(params.to_vec(), context);
                        let obj: JsObject = arr.into();
                        JsValue::from(obj)
                    };
                    let args = serde_json::from_value(
                        value
                            .to_json(context)
                            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
                    )?;
                    Ok(handle.block_on(api.call_req_e(req(args).lower()))?)
                });
                check_result(context, result)
            }
        })
    })
    .name(js_func_name.clone())
    .build();

    let attr = Attribute::WRITABLE | Attribute::NON_ENUMERABLE | Attribute::CONFIGURABLE;
    context
        .register_global_property(js_func_name, js_func, attr)
        .expect("`register_global_property` should not fail");
}

macro_rules! bind_request_func {
    ($context:expr, $api:expr, $($name:literal => $req:expr),* $(,)?) => {
    $(
        bind_request($context, &$api, $name, $req);
    )*
    };
}

type SendMessageParams = (String, String, String);

async fn send_message(params: SendMessageParams, api: &ApiInfo) -> anyhow::Result<SignedMessage> {
    let (from, to, value) = params;

    let message = Message::transfer(
        Address::from_str(&from)?,
        Address::from_str(&to)?,
        humantoken::parse(&value)?, // Convert forest_shim::TokenAmount to TokenAmount3
    );

    Ok(api.mpool_push_message(message, None).await?)
}

type SleepParams = (u64,);
type SleepResult = ();

async fn sleep(params: SleepParams, _api: &ApiInfo) -> anyhow::Result<SleepResult> {
    let secs = params.0;
    time::sleep(time::Duration::from_secs(secs)).await;
    Ok(())
}

async fn sleep_tipsets(epochs: ChainEpochDelta, api: &ApiInfo) -> anyhow::Result<()> {
    let mut epoch = None;
    loop {
        let state = api.sync_status().await?;
        if state.active_syncs[0].stage() == SyncStage::Complete {
            if let Some(prev) = epoch {
                let curr = state.active_syncs[0].epoch();
                if (curr - prev) >= epochs {
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
    fn setup_context(&self, context: &mut Context, api: ApiInfo) {
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

        bind_request_func!(context, api,
                // Net API
                "net_addrs_listen" => |()| ApiInfo::net_addrs_listen_req(),
                "net_peers"        => |()| ApiInfo::net_peers_req(),
                "net_disconnect"   => |peer| ApiInfo::net_disconnect_req(peer),
                "net_connect"      => |addr| ApiInfo::net_connect_req(addr),

                // Node API
                "node_status" => |()| ApiInfo::node_status_req(),

                // Sync API
                "sync_check_bad" => |cid| ApiInfo::sync_check_bad_req(cid),
                "sync_mark_bad"  => |cid| ApiInfo::sync_mark_bad_req(cid),
                "sync_status"    => |()| ApiInfo::sync_status_req(),

                // Wallet API
                // TODO(elmattic): https://github.com/ChainSafe/forest/issues/3575
                //                 bind wallet_sign, wallet_verify
                "wallet_new"         => |sig_type| ApiInfo::wallet_new_req(sig_type),
                "wallet_default"     => |()| ApiInfo::wallet_default_address_req(),
                "wallet_balance"     => |addr| ApiInfo::wallet_balance_req(addr),
                "wallet_export"      => |addr| ApiInfo::wallet_export_req(addr),
                "wallet_import"      => |keys| ApiInfo::wallet_import_req(keys),
                "wallet_list"        => |()| ApiInfo::wallet_list_req(),
                "wallet_has"         => |addr| ApiInfo::wallet_has_req(addr),
                "wallet_set_default" => |addr| ApiInfo::wallet_set_default_req(addr),

                // Message Pool API
                "mpool_push_message" => |(message, specs)| ApiInfo::mpool_push_message_req(message, specs),

                // Common API
                "version" => |()| ApiInfo::version_req(),
                "shutdown" => |()| ApiInfo::shutdown_req(),
        );

        // Bind send_message, sleep, sleep_tipsets
        bind_func!(context, api, send_message);
        bind_func!(context, api, sleep);
        bind_func!(context, api, sleep_tipsets);
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

    pub fn run(self, api: ApiInfo) -> anyhow::Result<()> {
        let mut context = Context::default();
        self.setup_context(&mut context, api);

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
