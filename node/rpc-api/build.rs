// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};

use serde::Deserialize;
use syn::{Expr, ExprLit, Item, ItemConst, ItemMod, Lit};

struct RPCMethod {
    name: String,
    // params: P,
    // result: R,
}

#[derive(Deserialize)]
struct OpenRPCFile {
    methods: Vec<OpenRPCMethod>,
}

#[derive(Deserialize)]
struct OpenRPCMethod {
    name: String,
}

fn run() -> Result<(usize, usize), Box<dyn Error>> {
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

    let mut forest_rpc: HashMap<String, RPCMethod> = HashMap::new();

    for item in api_modules.iter() {
        if let Item::Mod(ItemMod {
            content: Some((_, items)),
            ..
        }) = item
        {
            items.iter().for_each(|item| {
                if let Item::Const(ItemConst { expr, .. }) = item {
                    if let Expr::Lit(ExprLit {
                        lit: Lit::Str(token),
                        ..
                    }) = *expr.clone()
                    {
                        let name = token.value();
                        forest_rpc.insert(name.clone(), RPCMethod { name });
                    }
                }
            });
        }
    }

    let lotus_rpc: OpenRPCFile = serde_json::from_str(&lotus_rpc_content)?;

    for lotus_method in lotus_rpc.methods.iter() {
        let forest_method = forest_rpc.get(&lotus_method.name);

        let status = match forest_method {
            Some(_method) => "✔️ ",
            None => "❌",
        };

        println!("cargo:warning= {} {}", status, lotus_method.name);
    }

    Ok((forest_rpc.len(), lotus_rpc.methods.len()))
}

fn main() {
    match run() {
        Ok((forest_count, lotus_count)) => {
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
