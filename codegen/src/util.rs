use std::{fmt, rc::Rc};

use gosyn::ast::{
    ChanMode, ChannelType, Comment, Expression, Field, FieldList, FuncType, Ident, InterfaceType,
    MapType, PointerType, Selector, SliceType, StringLit, StructType,
};

/// Ergonomic constructors for [`Expression`].
pub mod expr {
    use super::*;

    pub fn selector(left: Expression, right: impl Into<String>) -> Expression {
        Expression::Selector(Selector {
            pos: 0,
            x: Box::new(left),
            sel: Ident {
                pos: 0,
                name: right.into(),
            },
        })
    }
    pub fn ident(ident: impl Into<String>) -> Expression {
        Expression::Ident(Ident {
            pos: 0,
            name: ident.into(),
        })
    }
    pub fn slice(of: Expression) -> Expression {
        Expression::TypeSlice(SliceType {
            pos: (0, 0),
            typ: Box::new(of),
        })
    }
    pub fn pointer(to: Expression) -> Expression {
        Expression::TypePointer(PointerType {
            pos: 0,
            typ: Box::new(to),
        })
    }
}

/// Set the `position` field on the given [`gosyn::ast`] struct recursively.
///
/// Useful for clearing that field for comparison (`==`) purposes.
///
/// # API Hazards
/// - This is a fast and loose trait, and may set _any_ [`usize`]s in the tree.
/// - Only partially implemented, and may panic
pub trait SetPos {
    fn set_pos(&mut self, new: usize);
}

impl<T> SetPos for &mut T
where
    T: SetPos,
{
    fn set_pos(&mut self, new: usize) {
        T::set_pos(self, new)
    }
}

impl<T> SetPos for Vec<T>
where
    T: SetPos,
{
    fn set_pos(&mut self, new: usize) {
        for it in self {
            it.set_pos(new)
        }
    }
}

impl<T> SetPos for Option<T>
where
    T: SetPos,
{
    fn set_pos(&mut self, new: usize) {
        if let Some(it) = self {
            it.set_pos(new)
        }
    }
}

impl<T> SetPos for Rc<T>
where
    T: SetPos + Clone,
{
    fn set_pos(&mut self, new: usize) {
        Rc::make_mut(self).set_pos(new)
    }
}

impl<T, U> SetPos for (T, U)
where
    T: SetPos,
    U: SetPos,
{
    fn set_pos(&mut self, new: usize) {
        let (a, b) = self;
        a.set_pos(new);
        b.set_pos(new)
    }
}

impl SetPos for usize {
    fn set_pos(&mut self, new: usize) {
        *self = new; // naughty
    }
}

impl SetPos for String {
    fn set_pos(&mut self, _: usize) {}
}

impl SetPos for ChanMode {
    fn set_pos(&mut self, _: usize) {}
}

macro_rules! set_pos {
    ($(
        $ty:ty {
            $($ident:ident),* $(,)?
        }
    ),* $(,)?) => {
        $(
            impl SetPos for $ty {
                fn set_pos(&mut self, new: usize) {
                    let Self {
                        $($ident,)*
                    } = self;
                    $($ident.set_pos(new);)*
                }
            }
        )*
    };
}

set_pos! {
    Field { name, typ, tag, comments },
    Ident { pos, name },
    StringLit { pos, value },
    Comment { pos, text },
    Selector { pos, x, sel },
    ChannelType { pos, dir, typ },
    SliceType { pos, typ },
    PointerType { pos, typ },
    MapType { pos, key, val },
    InterfaceType { pos, methods },
    FieldList { pos, list },
    FuncType { pos, typ_params, params, result },
    StructType { pos, fields },
}

impl SetPos for Expression {
    fn set_pos(&mut self, new: usize) {
        match self {
            Expression::Call(_) => todo!(),
            Expression::Index(_) => todo!(),
            Expression::IndexList(_) => todo!(),
            Expression::Slice(_) => todo!(),
            Expression::Ident(it) => it.set_pos(new),
            Expression::FuncLit(_) => todo!(),
            Expression::Ellipsis(_) => todo!(),
            Expression::Selector(it) => it.set_pos(new),
            Expression::BasicLit(_) => todo!(),
            Expression::Range(_) => todo!(),
            Expression::Star(_) => todo!(),
            Expression::Paren(_) => todo!(),
            Expression::TypeAssert(_) => todo!(),
            Expression::CompositeLit(_) => todo!(),
            Expression::List(_) => todo!(),
            Expression::Operation(_) => todo!(),
            Expression::TypeMap(it) => it.set_pos(new),
            Expression::TypeArray(_) => todo!(),
            Expression::TypeSlice(it) => it.set_pos(new),
            Expression::TypeFunction(it) => it.set_pos(new),
            Expression::TypeStruct(it) => it.set_pos(new),
            Expression::TypeChannel(it) => it.set_pos(new),
            Expression::TypePointer(it) => it.set_pos(new),
            Expression::TypeInterface(it) => it.set_pos(new),
        }
    }
}

/// Custom formatter.
pub struct Fmt<'a, T>(pub &'a T);

impl fmt::Display for Fmt<'_, Expression> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Expression::Call(_) => todo!(),
            Expression::Index(_) => todo!(),
            Expression::IndexList(_) => todo!(),
            Expression::Slice(_) => todo!(),
            Expression::Ident(it) => f.write_str(&it.name),
            Expression::FuncLit(_) => todo!(),
            Expression::Ellipsis(_) => todo!(),
            Expression::Selector(it) => {
                f.write_fmt(format_args!("{}.{}", Fmt(&*it.x), &it.sel.name))
            }
            Expression::BasicLit(_) => todo!(),
            Expression::Range(_) => todo!(),
            Expression::Star(_) => todo!(),
            Expression::Paren(_) => todo!(),
            Expression::TypeAssert(_) => todo!(),
            Expression::CompositeLit(_) => todo!(),
            Expression::List(_) => todo!(),
            Expression::Operation(_) => todo!(),
            Expression::TypeMap(it) => {
                f.write_fmt(format_args!("map[{}]{}", Fmt(&*it.key), Fmt(&*it.val)))
            }
            Expression::TypeArray(_) => todo!(),
            Expression::TypeSlice(it) => f.write_fmt(format_args!("[]{}", Fmt(&*it.typ))),
            Expression::TypeFunction(_) => todo!(),
            Expression::TypeStruct(it) => match it.fields.is_empty() {
                true => f.write_fmt(format_args!("interface {{}}")),
                false => todo!(),
            },
            Expression::TypeChannel(it) => match &it.dir {
                Some(ChanMode::Recv) => f.write_fmt(format_args!("<-chan {}", Fmt(&*it.typ))),
                Some(ChanMode::Send) => f.write_fmt(format_args!("chan<- {}", Fmt(&*it.typ))),
                None => f.write_fmt(format_args!("chan {}", Fmt(&*it.typ))),
            },
            Expression::TypePointer(it) => f.write_fmt(format_args!("*{}", Fmt(&*it.typ))),
            Expression::TypeInterface(it) => match it.methods.list.is_empty() {
                true => f.write_fmt(format_args!("interface {{}}")),
                false => todo!(),
            },
        }
    }
}
