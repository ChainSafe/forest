use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};

use syn::{Expr, ExprLit, Item, ItemConst, ItemMod, Lit};

struct Method {
    method: String,
    // params: P,
    // result: R,
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut file = File::open("src/lib.rs")?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    let ast = syn::parse_file(&content)?;
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

    let methods = api_modules.iter().fold(vec![], |mut acc, item| match item {
        Item::Mod(ItemMod { content, .. }) => {
            if let Some((_, items)) = content {
                items.iter().for_each(|item| match item {
                    Item::Const(ItemConst { expr, .. }) => {
                        match *expr.clone() {
                            Expr::Lit(ExprLit { lit, .. }) => match lit {
                                Lit::Str(token) => {
                                    let method = token.value();
                                    acc.push(Method { method });
                                }
                                _ => {}
                            },
                            _ => {}
                        };
                    }
                    _ => {}
                })
            }
            acc
        }
        _ => acc,
    });

    for method in methods {
        println!("cargo:warning={}", method.method);
    }

    Ok(())
}

fn main() {
    run().expect("parses");
}
