// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fs::read_to_string;
use std::io;

use boa_engine::object::{FunctionBuilder, JsArray};
use boa_engine::{prelude::JsObject, property::Attribute, syntax::parser::ParseError};
use boa_engine::{Context, JsResult, JsValue};
use convert_case::{Case, Casing};
use rustyline::{config::Config as RustyLineConfig, EditMode, Editor};
use serde::Serialize;
use serde_json::Value as JsonValue;
use structopt::StructOpt;

use super::Config;
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
    }

    function getPeer(peerID) {
        return netPeers().find((x) => x.ID == peerID);
    }

    function isPeerConnected(peerID) {
        return netPeers().some((x) => x.ID == peerID);
    }
"#;

fn require(_: &JsValue, params: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let param = params.get(0).unwrap();

    let path = param
        .to_string(context)
        .expect("Failed to convert to string")
        .to_string();

    println!("Loading: {}", path);
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
            $token,
        )
        .name(js_func_name.clone())
        .build();
        let attr = Attribute::WRITABLE | Attribute::NON_ENUMERABLE | Attribute::CONFIGURABLE;
        $context.register_global_property(js_func_name, js_func, attr);
    };
}

fn init_context(token: &Option<String>) -> Context {
    let mut context = Context::default();

    // Disable tracing
    context.set_trace(false);

    context.register_global_property("_BOA_VERSION", "0.16.0", Attribute::default());

    // Add custom implementation that mimics `require`
    context.register_global_function("require", 0, require);

    // Add custom object that mimics `module.exports`
    let moduleobj = JsObject::default();
    moduleobj
        .set("exports", JsValue::from(" "), false, &mut context)
        .unwrap();
    context.register_global_property("module", JsValue::from(moduleobj), Attribute::default());

    context.eval(ON_INIT_SCRIPT).expect("ON_INIT_SCRIPT script should work");

    // Bind net ops
    // TODO: remove token clones
    bind_func!(&mut context, token.clone(), net_addrs_listen);
    bind_func!(&mut context, token.clone(), net_peers);
    bind_func!(&mut context, token.clone(), net_disconnect);
    bind_func!(&mut context, token.clone(), net_connect);

    // Bind sync ops
    bind_func!(&mut context, token.clone(), sync_check_bad);
    bind_func!(&mut context, token.clone(), sync_mark_bad);
    bind_func!(&mut context, token.clone(), sync_status);

    context
}

impl AttachCommand {
    pub async fn run(&self, config: Config) -> anyhow::Result<()> {
        let mut context = init_context(&config.client.rpc_token);

        let config = RustyLineConfig::builder()
            .keyseq_timeout(1)
            .edit_mode(EditMode::Emacs)
            .build();

        let mut editor: rustyline::Editor<()> =
            Editor::with_config(config).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        'main: loop {
            let mut prompt = "> ";
            let mut buffer = String::new();
            loop {
                match editor.readline(prompt) {
                    Ok(input) => {
                        if input == ":quit" {
                            break 'main;
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
                            Ok(v) => println!("{}", v.display()),
                            Err(v) => eprintln!("Uncaught: {:?}", v),
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
                                eprintln!("Expecting token {:?} but got {}", expected, found);
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
        Ok(())
    }
}
