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
use proc_macro2::Span;
use quote::quote;
use syn::{parse_quote, punctuated::Punctuated, token, Ident};

use util::{expr, SetPos as _};

fn main() -> anyhow::Result<()> {
    let (methods, errors) = [
        (include_str!("../api/api_common.go"), "Common"),
        (include_str!("../api/api_net.go"), "Net"),
        (include_str!("../api/api_full.go"), "FullNode"),
    ]
    .into_iter()
    .map(|(text, interface)| gosyn::parse_source(text).and_then(|it| extract(it, interface)))
    .collect::<Result<Vec<_>, _>>()?
    .into_iter()
    .flatten()
    .sorted()
    .map(|(name, (params, returns))| {
        do_resolve(name, params, returns, resolve).map(Method::into_syn)
    })
    .partition_result::<Vec<_>, Vec<_>, _, _>();

    let gen = parse_quote! {
        pub trait Api {
            #(#methods)*
        }
    };
    println!("{}", prettyplease::unparse(&gen));

    for it in &errors {
        eprintln!("{}", it)
    }

    eprintln!(
        "processed {} methods. Additionally, there were {} methods that failed to process.",
        methods.len(),
        errors.len()
    );

    Ok(())
}

/// Get the methods defined in a given interface.
///
/// Special handling for `context.Context` and `(_, error)`.
///
/// Returns a mapping from `method_name` -> `(param_types, return_type)`.
pub fn extract(
    file: File,
    interface: &str,
) -> anyhow::Result<HashMap<Ident, (FieldList, Option<Field>)>> {
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
        item.set_pos(0);

        if let Expression::TypeFunction(FuncType {
            pos: _,
            typ_params,
            mut params,
            mut result,
        }) = item.typ
        {
            ensure!(
                item.name.len() == 1,
                "method `{}` must have a single name",
                item.name.iter().map(|it| &it.name).join(", ")
            );

            let name = &item.name[0].name;
            let name = syn::parse_str(name)
                .with_context(|| format!("couldn't parse method `{}`", name))?;

            ensure!(
                typ_params.list.is_empty(),
                "method `{}` must not be generic",
                name
            );

            if let Some(first) = params.list.first_mut() {
                if first.typ == expr::selector(expr::ident("context"), "Context") {
                    params.list.remove(0);
                }
            }

            match result.list.len() {
                0 | 1 => {}
                2 => {
                    ensure!(
                        result.list.remove(1).typ == expr::ident("error"),
                        "method `{}` has unsupported return type",
                        name
                    )
                }
                _ => bail!("method `{}` has too many return values", name),
            }

            match all_methods.entry(name) {
                Entry::Occupied(it) => bail!("duplicate method definition {}", it.key()),
                Entry::Vacant(it) => {
                    it.insert((params, result.list.pop()));
                }
            }
        }
    }

    Ok(all_methods)
}

/// A processed method definition.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Method<T> {
    name: Ident,
    params: Vec<T>,
    returns: Option<T>,
}

impl Method<syn::Type> {
    fn into_syn(self) -> syn::TraitItemFn {
        let Self {
            name,
            params,
            returns,
        } = self;
        syn::TraitItemFn {
            attrs: vec![],
            sig: syn::Signature {
                constness: None,
                asyncness: None,
                unsafety: None,
                abi: None,
                fn_token: token::Fn::default(),
                ident: name,
                generics: syn::Generics {
                    lt_token: None,
                    params: Punctuated::new(),
                    gt_token: None,
                    where_clause: None,
                },
                paren_token: token::Paren::default(),
                inputs: params
                    .into_iter()
                    .enumerate()
                    .map(|(ix, ty)| -> syn::FnArg {
                        let ident = Ident::new(&format!("arg{ix}"), Span::call_site());
                        parse_quote!(#ident: #ty)
                    })
                    .collect(),
                variadic: None,
                output: match returns {
                    Some(it) => syn::ReturnType::Type(token::RArrow::default(), Box::new(it)),
                    None => syn::ReturnType::Default,
                },
            },
            default: None,
            semi_token: Some(token::Semi::default()),
        }
    }
}

/// Some types for this method could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ResolveError {
    name: Ident,
    required: Vec<Expression>,
}

impl std::error::Error for ResolveError {}

impl fmt::Display for ResolveError {
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

/// Attempt to transform this method using the given `resolver`.
fn do_resolve<T>(
    name: Ident,
    params: FieldList,
    returns: Option<Field>,
    mut resolver: impl FnMut(Expression) -> Option<T>,
) -> Result<Method<T>, ResolveError> {
    let mut required = vec![];
    let mut resolved_params = vec![];
    let mut resolved_returns = None;
    for Field { typ, .. } in params.list {
        match resolver(typ.clone()) {
            Some(it) => resolved_params.push(it),
            None => required.push(typ),
        }
    }
    if let Some(Field { typ, .. }) = returns {
        match resolver(typ.clone()) {
            Some(it) => resolved_returns = Some(it),
            None => required.push(typ),
        }
    }
    match required.is_empty() {
        true => Ok(Method {
            name,
            params: resolved_params,
            returns: resolved_returns,
        }),
        false => Err(ResolveError { name, required }),
    }
}

fn resolve(mut ty: Expression) -> Option<syn::Type> {
    ty.set_pos(0);
    while let Expression::TypePointer(it) = ty {
        ty = *it.typ;
    }
    let map = HashMap::<_, _, RandomState>::from_iter(
        [
            (
                expr::selector(expr::ident("address"), "Address"),
                quote!(crate::shim::address::Address),
            ),
            (
                expr::selector(expr::ident("types"), "TipSetKey"),
                quote!(crate::blocks::TipsetKeys),
            ),
            (
                expr::selector(expr::ident("cid"), "Cid"),
                quote!(::cid::Cid),
            ),
            (
                expr::selector(expr::ident("types"), "BigInt"),
                quote!(::num::BigInt),
            ),
            // TODO(aatifsyed): should these go via HasLotusJson
            (expr::ident("bool"), quote!(::std::primitive::bool)),
            (expr::ident("uint64"), quote!(::std::primitive::u64)),
            (
                expr::slice(expr::ident("byte")),
                quote!(::std::vec::Vec<::std::primitive::u8>),
            ),
        ]
        .into_iter()
        .map(|(expr, ty)| {
            (
                expr,
                parse_quote! {
                    <#ty as crate::lotus_json::HasLotusJson>::LotusJson
                },
            )
        }),
    );
    map.get(&ty).cloned()
}
