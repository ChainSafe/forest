#![allow(clippy::disallowed_types)] // I _hate_ this

mod util;

use std::{
    collections::{
        hash_map::{Entry, RandomState},
        HashMap,
    },
    fmt,
};

use anyhow::{anyhow, bail, ensure, Context as _};
use gosyn::ast::{Declaration, Expression, Field, FieldList, File, FuncType};
use itertools::Itertools as _;

use util::{expr, SetPos as _};

fn main() -> anyhow::Result<()> {
    let (mapped, could_not_map) = [
        (include_str!("../api/api_common.go"), "Common"),
        (include_str!("../api/api_net.go"), "Net"),
        (include_str!("../api/api_full.go"), "FullNode"),
    ]
    .into_iter()
    .map(|(text, interface)| gosyn::parse_source(text).and_then(|it| extract(it, interface)))
    .collect::<Result<Vec<_>, _>>()?
    .into_iter()
    .flatten()
    .map(|(name, (params, returns))| do_map(name, params, returns, map))
    .sorted()
    .partition_result::<Vec<_>, Vec<_>, _, _>();

    for Method {
        name,
        params,
        returns,
    } in &mapped
    {
        println!(
            "{}({}) -> ({})",
            name,
            params.join(", "),
            returns.join(", ")
        )
    }

    println!();

    for it in &could_not_map {
        println!("{}", it)
    }

    println!(
        "processed {} methods. Additionally, there were {} methods that failed to process.",
        mapped.len(),
        could_not_map.len()
    );

    Ok(())
}

/// Get the methods defined in a given interface.
///
/// Returns a mapping from `method_name` -> `(param_types, return_types)`.
pub fn extract(
    file: File,
    interface: &str,
) -> anyhow::Result<HashMap<String, (FieldList, FieldList)>> {
    let found = file
        .decl
        .into_iter()
        .flat_map(|it| match it {
            Declaration::Type(ty) => Some(ty.specs),
            _ => None,
        })
        .flatten()
        .filter(|it| it.name.name == interface)
        .exactly_one()
        .map_err(|too_many| anyhow!("{}", too_many))
        .with_context(|| format!("couldn't get `type {interface} interface {{ .. }}`"))?
        .typ;

    let Expression::TypeInterface(interface) = found else {
        bail!("expected Expression::TypeInterface, not {:?}", found)
    };

    let mut all_methods = HashMap::new();

    for mut item in interface.methods.list {
        if let Expression::TypeFunction(FuncType {
            pos: _,
            typ_params,
            params,
            result,
        }) = item.typ
        {
            ensure!(
                item.name.len() == 1,
                "method `{}` must have a single name",
                item.name.iter().map(|it| &it.name).join(", ")
            );
            ensure!(
                typ_params.list.is_empty(),
                "generic functions are not supported"
            );
            match all_methods.entry(item.name.remove(0).name) {
                Entry::Occupied(it) => bail!("duplicate method definition {}", it.key()),
                Entry::Vacant(it) => {
                    it.insert((params, result));
                }
            }
        }
    }

    Ok(all_methods)
}

/// A processed method definition.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Method<T> {
    name: String,
    params: Vec<T>,
    returns: Vec<T>,
}

/// Some types for this method could not be mapped.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct MapError {
    name: String,
    required: Vec<Expression>,
}

impl std::error::Error for MapError {}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            "{} depends on the following unmapped types:\n",
            self.name
        ))?;
        for req in &self.required {
            f.write_fmt(format_args!("- {}\n", util::Fmt(req)))?
        }

        Ok(())
    }
}

/// Attempt to transform this method using the given `mapper`.
fn do_map<T>(
    name: String,
    params: FieldList,
    returns: FieldList,
    mut mapper: impl FnMut(Expression) -> Option<T>,
) -> Result<Method<T>, MapError> {
    let mut required = vec![];
    let mut mapped_params = vec![];
    let mut mapped_returns = vec![];
    for Field { typ, .. } in params.list {
        match mapper(typ.clone()) {
            Some(it) => mapped_params.push(it),
            None => required.push(typ),
        }
    }
    for Field { typ, .. } in returns.list {
        match mapper(typ.clone()) {
            Some(it) => mapped_returns.push(it),
            None => required.push(typ),
        }
    }
    match required.is_empty() {
        true => Ok(Method {
            name,
            params: mapped_params,
            returns: mapped_returns,
        }),
        false => Err(MapError { name, required }),
    }
}

fn map(mut ty: Expression) -> Option<&'static str> {
    ty.set_pos(0);
    while let Expression::TypePointer(it) = ty {
        ty = *it.typ;
    }
    let map = HashMap::<_, _, RandomState>::from_iter([
        (expr::selector(expr::ident("context"), "Context"), "Context"),
        (expr::ident("error"), "Error"),
        (expr::selector(expr::ident("address"), "Address"), "Address"),
        (
            expr::selector(expr::ident("types"), "TipSetKey"),
            "TipsetKey",
        ),
        (expr::selector(expr::ident("cid"), "Cid"), "Cid"),
        (expr::selector(expr::ident("types"), "BigInt"), "BigInt"),
        (expr::ident("bool"), "bool"),
        (expr::ident("uint64"), "u64"),
        (
            expr::selector(expr::ident("abi"), "ChainEpoch"),
            "ChainEpoch",
        ),
        (expr::slice(expr::ident("byte")), "Vec<u8>"),
        (expr::pointer(expr::ident("MessagePrototype")), "Message"),
    ]);
    map.get(&ty).copied()
}
