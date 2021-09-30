// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::cmp;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};

use serde::Deserialize;
use syn::{
    AngleBracketedGenericArguments, Expr, ExprLit, GenericArgument, Item, ItemConst, ItemMod,
    ItemType, Lit, Path, PathArguments, PathSegment, Type, TypePath, TypeTuple,
};

const API_IMPLEMENTATION_MD_PATH: &str = "../../API_IMPLEMENTATION.md";
const LOTUS_OPENRPC_JSON_PATH: &str = "static/full.json";
const FOREST_RPC_API_LIB_PATH: &str = "src/lib.rs";
const FOREST_RPC_API_AST_PATH: &str = "static/ast.ron";

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
        "Actor" => "ActorState".to_owned(),
        "apiNetworkVersion" => "NetworkVersion".to_owned(),
        "crypto.Signature" => "SignatureJson".to_owned(),
        "dline.Info" => "DeadlineInfo".to_owned(),
        "Message" => "UnsignedMessageJson".to_owned(),
        "MsgLookup" => "MessageLookup".to_owned(),
        "string" => "String".to_owned(),
        "SyncState" => "RPCSyncState".to_owned(),
        "TipsetKey" => "TipsetKeys".to_owned(), // Maybe?
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

type MethodMap = BTreeMap<String, RPCMethod>;
type LongestMethodNameLen = usize;
type ParamsMismatches = Vec<(String, usize, String, String)>;
type ResultMismatches = Vec<(String, String, String)>;
type ForestOnlyMethods = Vec<String>;
type Metrics = (
    MethodMap,
    MethodMap,
    LongestMethodNameLen,
    ParamsMismatches,
    ResultMismatches,
    ForestOnlyMethods,
);

fn run() -> Result<Metrics, Box<dyn Error>> {
    let mut lotus_rpc_file = File::open(LOTUS_OPENRPC_JSON_PATH)?;
    let mut lotus_rpc_content = String::new();
    lotus_rpc_file.read_to_string(&mut lotus_rpc_content)?;

    let mut api_lib = File::open(FOREST_RPC_API_LIB_PATH)?;
    let mut api_lib_content = String::new();
    api_lib.read_to_string(&mut api_lib_content)?;

    let ast = syn::parse_file(&api_lib_content)?;
    let out = format!("{:#?}", ast);

    let mut ast_file = File::create(FOREST_RPC_API_AST_PATH).expect("Create static/ast.ron failed");
    ast_file
        .write_all(out.as_bytes())
        .expect("Write static/ast.ron failed");

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

    let mut longest_method_name_len = 0;

    let mut params_mismatches = vec![];
    let mut result_mismatches = vec![];
    let mut forest_only_methods = vec![];

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
                        longest_method_name_len = cmp::max(longest_method_name_len, name.len());
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
        longest_method_name_len = cmp::max(longest_method_name_len, lotus_method.name.len());
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

        if let Some(forest_method) = forest_rpc.get(&lotus_method.name) {
            // Check params
            for (param_index, forest_param) in forest_method.params.iter().enumerate() {
                let lotus_param = match lotus_method.params.get(param_index) {
                    Some(lotus_param) => map_lotus_type(lotus_param.description.as_ref()),
                    None => "()".to_owned(),
                };

                if compare_types(&lotus_param, forest_param) {
                    params_mismatches.push((
                        lotus_method.name.clone(),
                        param_index,
                        forest_param.to_owned(),
                        lotus_param,
                    ));
                }
            }

            // Check result
            let lotus_result = map_lotus_type(lotus_method.result.description.as_ref());

            if compare_types(&lotus_result, &forest_method.result) {
                result_mismatches.push((
                    lotus_method.name,
                    forest_method.result.clone(),
                    lotus_result,
                ));
            }
        }
    }

    // Check forest methods against lotus methods
    for forest_method in forest_rpc.keys() {
        if !lotus_rpc.contains_key(forest_method) {
            forest_only_methods.push(forest_method.to_owned());
        }
    }

    Ok((
        forest_rpc,
        lotus_rpc,
        longest_method_name_len,
        params_mismatches,
        result_mismatches,
        forest_only_methods,
    ))
}

fn main() {
    match run() {
        Ok((
            forest_rpc,
            lotus_rpc,
            longest_method_name_len,
            params_mismatches,
            result_mismatches,
            forest_only_methods,
        )) => {
            let method_header = "Method";
            let params_header = "Params";
            let result_header = "Result";
            let method_pad_space = " ".repeat(longest_method_name_len - method_header.len() + 2);
            let method_pad_dash = "-".repeat(longest_method_name_len + 2);
            let params_pad_dash = "-".repeat(params_header.len());
            let result_pad_dash = "-".repeat(result_header.len());

            let mut method_table = vec![];

            method_table.push(format!(
                "| Present | {}{} | {} | {} |",
                method_header, method_pad_space, params_header, result_header,
            ));
            method_table.push(format!(
                "| ------- | {} | {} | {}",
                method_pad_dash, params_pad_dash, result_pad_dash
            ));

            for (lotus_name, lotus_method) in lotus_rpc.iter() {
                let forest_method = forest_rpc.get(lotus_name);

                let status = match forest_method {
                    Some(_method) => "  ✔️   ",
                    None => "  ❌   ",
                };

                let (forest_params, forest_result) = match forest_method {
                    Some(method) => (
                        format!("({})", method.params.join(", ")),
                        method.result.clone(),
                    ),
                    None => ("-".to_owned(), "-".to_owned()),
                };

                // Pad strings for display
                let method_pad = " ".repeat(longest_method_name_len - lotus_method.name.len());

                method_table.push(format!(
                    "| {} | `{}`{} | `{}` | `{}` |",
                    status, lotus_method.name, method_pad, forest_params, forest_result,
                ));
            }

            let forest_count = forest_rpc.len();
            let lotus_count = lotus_rpc.len();
            let api_coverage = (forest_count as f32 / lotus_count as f32) * 100.0;

            let params_mismatches_table = params_mismatches
                .iter()
                .map(|(method, param_index, forest_param, lotus_param)| {
                    format!(
                        "| `{method}`{method_space} | `{param_index}` | `{forest_param}` | `{lotus_param}`",
                        method = method,
                        method_space = " ".repeat(longest_method_name_len - method.len()),
                        param_index = param_index,
                        forest_param = forest_param,
                        lotus_param = lotus_param
                    )
                })
                .collect::<Vec<String>>();

            let result_mismatches_list = result_mismatches
                .iter()
                .map(|(method, forest_result, lotus_result)| {
                    format!(
                        "| `{method}`{method_space} | `{forest_result}` | `{lotus_result}`",
                        method = method,
                        method_space = " ".repeat(longest_method_name_len - method.len()),
                        forest_result = forest_result,
                        lotus_result = lotus_result
                    )
                })
                .collect::<Vec<String>>();

            let forest_only_methods_list = forest_only_methods
                .iter()
                .map(|method| format!("- `{method}`", method = method))
                .collect::<Vec<String>>();

            let report = format!(
                r####"# Forest API Implementation Report

## Stats

- Forest method count: {forest_count}
- Lotus method count: {lotus_count}
- API coverage: {api_coverage:.2}%

## Forest-only Methods

These methods exist in Forest only and cannot be compared:

{forest_only_methods_list}

## Type Mismatches

Some methods contain possible inconsistencies between Forest and Lotus.

### Params Mismatches

| Method | Param Index | Forest Param | Lotus Param |
| ------ | ----------- | ------------ | ----------- |
{params_mismatches_table}

### Results Mismatches

| Method | Forest Result | Lotus Result |
| ------ | ------------- | ------------ |
{result_mismatches_list}

## Method Table
{method_table}

## Help & Contributions

If there's a particular API that's needed that we're missing, be sure to let us know.

Feel free to reach out in #fil-forest-help in the [Filecoin Slack](https://docs.filecoin.io/community/chat-and-discussion-forums/), file a GitHub issue, or contribute a pull request.
"####,
                forest_count = forest_rpc.len(),
                lotus_count = lotus_rpc.len(),
                api_coverage = api_coverage,
                params_mismatches_table = params_mismatches_table.join("\n"),
                result_mismatches_list = result_mismatches_list.join("\n"),
                forest_only_methods_list = forest_only_methods_list.join("\n"),
                method_table = method_table.join("\n"),
            );

            // Create if report already exists
            let existing_report = match fs::metadata(API_IMPLEMENTATION_MD_PATH) {
                Ok(metadata) => {
                    if metadata.is_file() {
                        let mut existing_report_file = File::open(API_IMPLEMENTATION_MD_PATH)
                            .expect("Open existing API_IMPLEMENTATION.md file");

                        let mut existing_report_content = String::new();

                        existing_report_file
                            .read_to_string(&mut existing_report_content)
                            .expect("Read existing API_IMPLEMENTATION.md file");

                        existing_report_content
                    } else {
                        println!(
                            "cargo:warning=API_IMPLEMENTATION.md file exists but is not a file"
                        );

                        "Unexpected condition".to_owned()
                    }
                }
                Err(_) => {
                    File::create(API_IMPLEMENTATION_MD_PATH)
                        .expect("Create API_IMPLEMENTATION.md failed");

                    "".to_owned()
                }
            };

            if existing_report != report {
                println!(
                    "cargo:warning=Forest API change detected. Writing new report to API_IMPLEMENTATION.md..."
                );

                let mut report_file = OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(API_IMPLEMENTATION_MD_PATH)
                    .expect("Modify API_IMPLEMENTATION.md");

                report_file
                    .set_len(0)
                    .expect("Truncate existing API_IMPLEMENTATION.md file");

                report_file
                    .write_all(report.as_bytes())
                    .expect("Write API_IMPLEMENTATION.md failed");
            }
        }
        Err(err) => {
            println!(
                "cargo:warning=Error parsing Lotus OpenRPC file, skipping... Error was: {}",
                err
            );
        }
    }
}
