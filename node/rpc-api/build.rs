// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::cmp;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};

use serde::Deserialize;
use syn::{
    AngleBracketedGenericArguments, Expr, ExprLit, GenericArgument, Item, ItemConst, ItemMod,
    ItemType, Lit, Path, PathArguments, PathSegment, Type, TypePath, TypeTuple,
};

#[derive(Debug)]
struct RPCMethod {
    name: String,
    params: Vec<String>,
    result: String,
}

#[derive(Deserialize)]
struct OpenRPCFile {
    methods: Vec<OpenRPCMethod>,
}

#[derive(Deserialize)]
struct OpenRPCMethod {
    name: String,
    params: Vec<OpenRPCParams>,
    result: OpenRPCResult,
}

#[derive(Deserialize)]
struct OpenRPCParams {
    description: String,
}

#[derive(Deserialize)]
struct OpenRPCResult {
    description: String,
}

type MethodMap = BTreeMap<String, RPCMethod>;
type StringLengths = (usize, usize, usize);

fn parse_generic(generic_type: String, arguments: PathArguments) -> String {
    if let PathArguments::AngleBracketed(AngleBracketedGenericArguments { args, .. }) = arguments {
        let mut generic_args = vec![];

        for arg in args {
            if let GenericArgument::Type(Type::Path(TypePath {
                path: Path { segments, .. },
                ..
            })) = arg
            {
                for ps in segments {
                    let PathSegment { ident, .. } = ps;
                    let option_arg = ident.to_string();
                    generic_args.push(option_arg);
                }
            }
        }

        format!("{}<{}>", generic_type, generic_args.join(", "))
    } else {
        generic_type
    }
}

fn map_lotus_type(lotus_param: &str) -> String {
    let mut param = lotus_param.to_owned();

    if param.starts_with("[]") {
        param = param.replace("[]", "");
        param = format!("Vec<{}>", param);
    }

    if param.starts_with("map[string]") {
        param = param.replace("map[string]", "HashMap<String, ");
        param.push('>');
    }

    param = param.replace("*", "");
    param = param.replace("TipSet", "Tipset");

    // Strip namespaces
    param = param.replace("abi.", "");
    param = param.replace("address.", "");
    param = param.replace("bitfield.", "");
    param = param.replace("cid.", "");
    param = param.replace("miner.", "");
    param = param.replace("types.", "");

    // Replace primitive types
    param = param.replace("byte", "u8");
    param = param.replace("float64", "f64");
    param = param.replace("uint64", "u64");
    param = param.replace("int64", "i64");

    match param.as_ref() {
        "TipsetKey" => "TipsetKeys".to_owned(), // Maybe?
        "Message" => "UnsignedMessageJson".to_owned(),
        "Actor" => "ActorState".to_owned(),
        "dline.Info" => "DeadlineInfo".to_owned(),
        "apiNetworkVersion" => "NetworkVersion".to_owned(),
        "MsgLookup" => "MessageLookup".to_owned(),
        "SyncState" => "RPCSyncState".to_owned(),
        "crypto.Signature" => "SignatureJson".to_owned(),
        _ => param,
    }
}

fn compare_types(lotus: &str, forest: &str) -> bool {
    let lotus = lotus.replace("Json", "");
    let lotus = lotus.replace("Option<", "");
    let lotus = lotus.replace(">", "");

    let forest = forest.replace("Json", "");
    let forest = forest.replace("Option<", "");
    let forest = forest.replace(">", "");

    lotus.trim_end_matches("Json") != forest.trim_end_matches("Json")
}

fn run() -> Result<(MethodMap, MethodMap, StringLengths), Box<dyn Error>> {
    let mut lotus_rpc_file = File::open("static/full.json")?;
    let mut lotus_rpc_content = String::new();
    lotus_rpc_file.read_to_string(&mut lotus_rpc_content)?;

    let mut api_lib = File::open("src/lib.rs")?;
    let mut api_lib_content = String::new();
    api_lib.read_to_string(&mut api_lib_content)?;

    let ast = syn::parse_file(&api_lib_content)?;
    let out = format!("{:#?}", ast);

    let mut ast_file = std::fs::File::create("static/ast.ron").expect("create failed");
    ast_file.write_all(out.as_bytes()).expect("write failed");

    println!(
        "cargo:warning=wrote {} syntax tree items to static/ast.ron",
        ast.items.len()
    );

    let api_modules: Vec<&Item> = ast
        .items
        .iter()
        .filter(|item| match item {
            Item::Mod(ItemMod { ident, .. }) => ident.to_string().ends_with("_api"),
            _ => false,
        })
        .collect();

    let mut forest_rpc = BTreeMap::new();
    let mut lotus_rpc = BTreeMap::new();

    let mut longest_name = 0;
    let mut longest_params = 0;
    let mut longest_result = 0;

    let mut name = "".to_owned();
    let mut params = vec![];
    let mut result = "".to_owned();

    for item in api_modules {
        if let Item::Mod(ItemMod {
            content: Some((_, items)),
            ..
        }) = item
        {
            for item in items {
                if let Item::Const(ItemConst { expr, .. }) = item {
                    if let Expr::Lit(ExprLit {
                        lit: Lit::Str(token),
                        ..
                    }) = *expr.clone()
                    {
                        name = token.value();
                        longest_name = cmp::max(longest_name, name.len());
                    }
                }

                if let Item::Type(ItemType { ty, ident, .. }) = item {
                    let token = ident.to_string();

                    if token.ends_with("Params") {
                        if let Type::Tuple(TypeTuple { elems, .. }) = *ty.clone() {
                            for t in elems {
                                if let Type::Path(TypePath {
                                    path: Path { segments, .. },
                                    ..
                                }) = t
                                {
                                    for ps in segments {
                                        let PathSegment { ident, .. } = ps;
                                        let param = ident.to_string();

                                        params.push(parse_generic(param, ps.arguments));
                                    }

                                    longest_params =
                                        cmp::max(longest_params, params.join(", ").len() + 2);
                                }
                            }
                        }
                    } else if token.ends_with("Result") {
                        if let Type::Path(TypePath {
                            path: Path { segments, .. },
                            ..
                        }) = *ty.clone()
                        {
                            for ps in segments {
                                let PathSegment {
                                    ident, arguments, ..
                                } = ps;

                                result = ident.to_string();
                                result = parse_generic(result, arguments);
                            }

                            longest_result = cmp::max(longest_result, result.len());

                            forest_rpc.insert(
                                name.clone(),
                                RPCMethod {
                                    name: name.clone(),
                                    params: params.clone(),
                                    result: result.clone(),
                                },
                            );
                        }

                        name = "".to_owned();
                        params = vec![];
                        result = "".to_owned();
                    }
                }
            }
        }
    }

    let lotus_rpc_file: OpenRPCFile = serde_json::from_str(&lotus_rpc_content)?;

    for lotus_method in lotus_rpc_file.methods {
        // Check lotus methods against forest methods
        longest_name = cmp::max(longest_name, lotus_method.name.len());
        lotus_rpc.insert(
            lotus_method.name.clone(),
            RPCMethod {
                name: lotus_method.name.clone(),
                params: lotus_method
                    .params
                    .iter()
                    .map(|schema| schema.description.clone())
                    .collect(),
                result: lotus_method.result.description.clone(),
            },
        );

        match forest_rpc.get(&lotus_method.name) {
            Some(forest_method) => {
                // Check params
                for (param_index, forest_param) in forest_method.params.iter().enumerate() {
                    let lotus_param = match lotus_method.params.get(param_index) {
                        Some(lotus_param) => map_lotus_type(lotus_param.description.as_ref()),
                        None => "()".to_owned(),
                    };

                    let method_pad = " ".repeat(longest_name - lotus_method.name.len());

                    if compare_types(&lotus_param, forest_param) {
                        println!(
                            "cargo:warning=Forest params type mismatch for method in param index {}: {}{} Forest: {}\t\t\tLotus: {}",
                            param_index,
                            lotus_method.name,
                            method_pad,
                            forest_param,
                            lotus_param,
                        )
                    }
                }

                // Check result
                let lotus_result = map_lotus_type(lotus_method.result.description.as_ref());
                let method_pad = " ".repeat(longest_name - lotus_method.name.len());

                if compare_types(&lotus_result, &forest_method.result) {
                    println!(
                        "cargo:warning=Forest result type mismatch for method: {}{}\t\t\t Forest: {}\t\t\tLotus: {}",
                        lotus_method.name,
                        method_pad,
                        forest_method.result,
                        lotus_result,
                    )
                }
            }
            None => {}
        }
    }

    // Check forest methods against lotus methods
    for forest_method in forest_rpc.keys() {
        if !lotus_rpc.contains_key(forest_method) {
            println!(
                "cargo:warning=Forest implements an RPC method that Lotus does not: {}",
                forest_method
            );
        }
    }

    Ok((
        forest_rpc,
        lotus_rpc,
        (longest_name, longest_params, longest_result),
    ))
}

fn main() {
    match run() {
        Ok((forest_rpc, lotus_rpc, longest_strs)) => {
            let (longest_method, longest_params, longest_result) = longest_strs;

            let method_header = "Method";
            let params_header = "Params";
            let result_header = "Result";
            let method_pad = " ".repeat(longest_method - method_header.len());
            let params_pad = " ".repeat(longest_params - params_header.len() + 2);
            let result_pad = " ".repeat(longest_result - result_header.len());

            println!(
                "cargo:warning=    | {}{} | {}{} | {}{} |",
                method_header, method_pad, params_header, params_pad, result_header, result_pad
            );

            for (lotus_name, lotus_method) in lotus_rpc.iter() {
                let forest_method = forest_rpc.get(lotus_name);

                let status = match forest_method {
                    Some(_method) => "✔️ ",
                    None => "❌",
                };

                let (forest_params, forest_result) = match forest_method {
                    Some(method) => (method.params.join(", "), method.result.clone()),
                    None => ("".to_owned(), "()".to_owned()),
                };

                // Pad strings for display
                let method_pad = " ".repeat(longest_method - lotus_method.name.len());
                let params_pad = " ".repeat(longest_params - forest_params.len());
                let result_pad = " ".repeat(longest_result - forest_result.len());

                println!(
                    "cargo:warning= {} | {}{} | ({}){} | {}{} |",
                    status,
                    lotus_method.name,
                    method_pad,
                    forest_params,
                    params_pad,
                    forest_result,
                    result_pad
                );
            }

            let forest_count = forest_rpc.len();
            let lotus_count = lotus_rpc.len();

            println!(
                "cargo:warning=Forest: {}, Lotus: {}, {:.2}%",
                forest_count,
                lotus_count,
                (forest_count as f32 / lotus_count as f32) * 100.0
            );
        }
        Err(err) => {
            println!(
                "cargo:warning=Error parsing Lotus OpenRPC file, skipping... Error was: {}",
                err
            );
        }
    }
}
