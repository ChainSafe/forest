// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::cmp;
use std::collections::HashMap;
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
}

fn run() -> Result<
    (
        HashMap<String, RPCMethod>,
        OpenRPCFile,
        (usize, usize, usize),
    ),
    Box<dyn Error>,
> {
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

    let mut forest_rpc = HashMap::new();
    let mut longest_name = 0;
    let mut longest_params = 0;
    let mut longest_result = 0;

    let mut name = "".to_owned();
    let mut params = vec![];
    let mut result = "".to_owned();

    for item in api_modules.iter() {
        if let Item::Mod(ItemMod {
            content: Some((_, items)),
            ..
        }) = item
        {
            for item in items.iter() {
                if let Item::Const(ItemConst { expr, .. }) = item {
                    if let Expr::Lit(ExprLit {
                        lit: Lit::Str(token),
                        ..
                    }) = *expr.clone()
                    {
                        name = token.value();
                        println!("cargo:warning=TODO: Name - {}", name);
                        longest_name = cmp::max(longest_name, name.len());
                    }
                }

                if let Item::Type(ItemType { ty, ident, .. }) = item {
                    let token = ident.to_string();

                    if token.ends_with("Params") {
                        if let Type::Tuple(TypeTuple { elems, .. }) = *ty.clone() {
                            for t in elems.iter() {
                                if let Type::Path(TypePath {
                                    path: Path { segments, .. },
                                    ..
                                }) = t
                                {
                                    for ps in segments.iter() {
                                        let PathSegment { ident, .. } = ps;
                                        let param = ident.to_string();
                                        println!("cargo:warning=TODO: Params - {}", param);
                                        params.push(param);
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
                            for ps in segments.iter() {
                                let PathSegment {
                                    ident, arguments, ..
                                } = ps;
                                result = ident.to_string();

                                if result == "Option" {
                                    if let PathArguments::AngleBracketed(
                                        AngleBracketedGenericArguments { args, .. },
                                    ) = arguments
                                    {
                                        for arg in args.iter() {
                                            if let GenericArgument::Type(Type::Path(TypePath {
                                                path: Path { segments, .. },
                                                ..
                                            })) = arg
                                            {
                                                let mut option_args = vec![];
                                                for ps in segments.iter() {
                                                    let PathSegment { ident, .. } = ps;
                                                    let option_arg = ident.to_string();
                                                    println!(
                                                        "cargo:warning=TODO: Option Arg - {}",
                                                        option_arg
                                                    );
                                                    option_args.push(option_arg);
                                                }
                                                result =
                                                    format!("Option<{}>", option_args.join(", "));
                                            }
                                        }
                                    }
                                }
                                println!("cargo:warning=TODO: Result - {}", result);
                            }
                            longest_result = cmp::max(longest_result, result.len());

                            println!("cargo:warning=TODO: HELLO 1: {}", &name);

                            forest_rpc.insert(
                                name.clone(),
                                RPCMethod {
                                    name: name.clone(),
                                    params: params.clone(),
                                    result: result.clone(),
                                },
                            );

                            println!("cargo:warning=TODO: HELLO 2");
                        }

                        name = "".to_owned();
                        params = vec![];
                        result = "".to_owned();
                    }
                }
            }
        }
    }

    let lotus_rpc: OpenRPCFile = serde_json::from_str(&lotus_rpc_content)?;

    for lotus_method in &lotus_rpc.methods {
        longest_name = cmp::max(longest_name, lotus_method.name.len());
    }

    println!("cargo:warning=TODO: FOREST RPC LEN {}", forest_rpc.len());

    for forest_method in forest_rpc.keys() {
        if lotus_rpc
            .methods
            .iter()
            .find(|m| &m.name == forest_method)
            .is_none()
        {
            println!(
                "cargo:warning=Forest implements an RPC method that Lotus does not: {}",
                forest_method
            );
        } else {
            println!("cargo:warning=TODO: {}", forest_method);
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

            for lotus_method in lotus_rpc.methods.iter() {
                let forest_method = forest_rpc.get(&lotus_method.name);

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
            let lotus_count = lotus_rpc.methods.len();

            println!(
                "cargo:warning=Forest: {}, Lotus: {}, {:.2}%",
                forest_count,
                lotus_count,
                (forest_count as f32 / lotus_count as f32) * 100.0
            );
        }
        Err(_) => {
            println!("cargo:warning=Error parsing Lotus OpenRPC file, skipping...");
        }
    }
}
