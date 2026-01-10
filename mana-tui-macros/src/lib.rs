use std::fmt::Display;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    Token,
    parse::{Parse, discouraged::Speculative},
    parse_macro_input,
    spanned::Spanned,
};

macro_rules! impl_parse_enum {
    ($enum_name:ident { $($variant:ident($inner:ty)),* $(,)? }) => {
        impl syn::parse::Parse for $enum_name {
            fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
                $(
                    let f = input.fork();
                    let res = f.parse::<$inner>();
                    if let Ok(inner) = res {
                        input.advance_to(&f);
                        return Ok($enum_name::$variant(inner));
                    }
                )*

                Err(input.error(concat!("expected one of: ", $(stringify!($variant), ", "),*)))
            }
        }
    };
}

macro_rules! impl_quote_enum {
    ($enum_name:ident { $($variant:ident),* $(,)? }) => {
        impl quote::ToTokens for $enum_name {
            fn to_tokens(&self, tokens: &mut TokenStream) {
                let tok = match self {
                    $(
                        $enum_name::$variant(inner) => quote! { #inner },
                    )*
                };
                tokens.extend(tok);
            }
        }
    };
}

/// # Example
///
///```
/// use mana_tui_macros::ui;
/// use mana_tui_elemental::prelude::*;
///
/// let root = ui! {
///    <block .title_top="sidebar" Width(Size::Fixed(10)) Padding::uniform(1)>
///        <block .title_top="2" />
///    </block>
/// };
///```
#[proc_macro]
pub fn ui(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // let input = preprocess_tokens(input.into());
    // let input = input.into();
    let tree = parse_macro_input!(input as ManaElement);
    let tokens = quote! { #tree };

    tokens.into()
}

#[derive(Debug, Clone)]
struct OpenTag {
    _lt: Token![<],
    data: ManaTagData,
    sl: Option<Token![/]>,
    _gt: Token![>],
}

#[derive(Debug, Clone)]
struct CloseTag {
    _lt: Token![<],
    _sl: Token![/],
    ident: ManaName,
    _gt: Token![>],
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ManaName {
    Ident(syn::Ident),
    Path(syn::ExprPath),
}

impl_quote_enum!(ManaName { Ident, Path });

impl Display for ManaName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManaName::Ident(ident) => write!(f, "{ident}"),
            ManaName::Path(path) => write!(
                f,
                "{}",
                path.path
                    .segments
                    .iter()
                    .map(|seg| seg.ident.to_string() + "::")
                    .collect::<String>()
            ),
        }
    }
}

impl ManaName {
    fn span(&self) -> Span {
        match self {
            ManaName::Ident(ident) => ident.span(),
            ManaName::Path(expr_path) => expr_path.span(),
        }
    }
}

impl_parse_enum!(ManaName {
    Ident(syn::Ident),
    Path(syn::ExprPath),
});

#[derive(Debug, Clone)]
struct ManaTagData {
    ident: ManaName,
    attrs: ManaAttrVec,
    components: ComponentVec,
}

#[derive(Debug, Clone)]
struct ManaAttr {
    _dot: Token![.],
    fn_name: syn::Ident,
    _eq: Token![=],
    value: ManaAttrValue,
}

#[derive(Debug, Clone)]
enum ManaAttrValue {
    Lit(syn::Lit),
    ExprTuple(syn::ExprTuple),
    ExprBlock(syn::ExprBlock),
}

impl_quote_enum!(ManaAttrValue {
    Lit,
    ExprTuple,
    ExprBlock
});

#[derive(Debug, Clone)]
struct ManaAttrVec(Vec<ManaAttr>);

impl_parse_enum!(
    ManaAttrValue {
        Lit(syn::Lit),
        ExprTuple(syn::ExprTuple),
        ExprBlock(syn::ExprBlock),
    }
);

#[derive(Debug, Clone)]
struct Component(ComponentExpr);

#[derive(Debug, Clone)]
enum ComponentExpr {
    Block(syn::ExprBlock),
    Call(syn::ExprCall),
    Cast(syn::ExprCast),
    Const(syn::ExprConst),
    If(syn::ExprIf),
    Index(syn::ExprIndex),
    Lit(syn::ExprLit),
    Macro(syn::ExprMacro),
    Match(syn::ExprMatch),
    MethodCall(syn::ExprMethodCall),
    Paren(syn::ExprParen),
    Path(syn::ExprPath),
    Range(syn::ExprRange),
    Repeat(syn::ExprRepeat),
    Struct(syn::ExprStruct),
    Try(syn::ExprTry),
    TryBlock(syn::ExprTryBlock),
    Tuple(syn::ExprTuple),
    Unary(syn::ExprUnary),
    Unsafe(syn::ExprUnsafe),
}

impl_parse_enum!(ComponentExpr {
    Block(syn::ExprBlock),
    Call(syn::ExprCall),
    Cast(syn::ExprCast),
    Const(syn::ExprConst),
    If(syn::ExprIf),
    Index(syn::ExprIndex),
    Lit(syn::ExprLit),
    Macro(syn::ExprMacro),
    Match(syn::ExprMatch),
    MethodCall(syn::ExprMethodCall),
    Paren(syn::ExprParen),
    Path(syn::ExprPath),
    Range(syn::ExprRange),
    Repeat(syn::ExprRepeat),
    Struct(syn::ExprStruct),
    Try(syn::ExprTry),
    TryBlock(syn::ExprTryBlock),
    Tuple(syn::ExprTuple),
    Unary(syn::ExprUnary),
    Unsafe(syn::ExprUnsafe),
});

impl_quote_enum!(ComponentExpr {
    Block,
    Call,
    Cast,
    Const,
    If,
    Index,
    Lit,
    Macro,
    Match,
    MethodCall,
    Paren,
    Path,
    Range,
    Repeat,
    Struct,
    Try,
    TryBlock,
    Tuple,
    Unary,
    Unsafe,
});

#[derive(Debug, Clone)]
struct ComponentVec(Vec<Component>);

impl Parse for ComponentVec {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let components = parse_any::<Component>(input).collect::<Vec<_>>();
        Ok(Self(components))
    }
}

#[derive(Debug, Clone)]
struct Element {
    open: OpenTag,
    children: Children,
    close: CloseTag,
}

#[derive(Debug, Clone)]
struct Children(Vec<Child>);

#[derive(Debug, Clone)]
enum Child {
    Block(syn::ExprBlock),
    El(Box<ManaElement>),
}

impl_quote_enum!(Child { El, Block });
impl_parse_enum!(Child {
    Block(syn::ExprBlock),
    El(Box<ManaElement>),
});

#[derive(Debug, Clone)]
enum ManaElement {
    Element(Element),
    SelfClosing(OpenTag),
}

impl Parse for ManaElement {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let f = input.fork();
        let open = f.parse::<OpenTag>()?;
        input.advance_to(&f);
        if open.sl.is_some() {
            return Ok(Self::SelfClosing(open));
        }
        let children = input.parse()?;
        let close = input.parse::<CloseTag>()?;
        if close.ident != open.data.ident {
            return Err(syn::Error::new(
                close.ident.span(),
                format!(
                    "closing tag </{}> does not match opening <{}>",
                    close.ident, open.data.ident
                ),
            ));
        }
        Ok(Self::Element(Element {
            open,
            children,
            close,
        }))
    }
}

impl Parse for Children {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut ret = Vec::new();
        while let Ok(child) = input.parse::<Child>() {
            ret.push(child);
        }

        Ok(Children(ret))
    }
}

fn parse_any<T: Parse>(input: syn::parse::ParseStream) -> impl Iterator<Item = T> {
    std::iter::from_fn(move || input.parse::<T>().ok())
}

impl Parse for ManaTagData {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let ident = input.parse()?;
        let attrs = input.parse()?;
        let components = input.parse()?;
        Ok(ManaTagData {
            ident,
            attrs,
            components,
        })
    }
}

impl Parse for ManaAttr {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(ManaAttr {
            _dot: input.parse()?,
            fn_name: input.parse()?,
            _eq: input.parse()?,
            value: input.parse()?,
        })
    }
}

impl Parse for ManaAttrVec {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = parse_any::<ManaAttr>(input).collect::<Vec<_>>();
        Ok(Self(attrs))
    }
}

impl Parse for Component {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        input.parse().map(Self)
    }
}

impl Parse for OpenTag {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            _lt: input.parse()?,
            data: input.parse()?,
            sl: input.parse()?,
            _gt: input.parse()?,
        })
    }
}

impl Parse for CloseTag {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            _lt: input.parse()?,
            _sl: input.parse()?,
            ident: input
                .parse()
                .map_err(|err| syn::Error::new(err.span(), "expected identifier"))?,
            _gt: input.parse()?,
        })
    }
}

impl quote::ToTokens for ManaElement {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            ManaElement::Element(element) => {
                tokens.extend(quote! { #element });
            }
            ManaElement::SelfClosing(open_tag) => {
                tokens.extend(quote! { #open_tag });
            }
        }
    }
}

impl quote::ToTokens for OpenTag {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let OpenTag {
            _lt,
            data,
            sl: _sl,
            _gt,
        } = self;
        data.to_tokens(tokens);
    }
}

impl quote::ToTokens for ManaTagData {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ManaTagData {
            ident,
            attrs,
            components,
        } = self;
        let out = quote! {
            ui(#ident::default()#attrs)#components
        };
        tokens.extend(out);
    }
}

impl quote::ToTokens for ComponentVec {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        if self.0.is_empty() {
            return;
        }
        let tuple_inner = self
            .0
            .iter()
            .map(|component| {
                let Component(c_expr) = component;
                let tuple_element = quote! {
                    #c_expr
                };
                tuple_element
            })
            .reduce(|acc, el| quote! {#acc,#el});
        let fncall = quote! {
            .with((#tuple_inner,))
        };
        tokens.extend(fncall);
    }
}

impl quote::ToTokens for ManaAttrVec {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let tok = self
            .0
            .iter()
            .map(|attr| quote! { #attr })
            .reduce(|acc, el| quote! {#acc #el});
        tokens.extend(tok);
    }
}

impl quote::ToTokens for ManaAttr {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ManaAttr {
            _dot,
            fn_name,
            _eq,
            value,
        } = self;
        let tok = quote! {
            .#fn_name(#value)
        };
        tokens.extend(tok);
    }
}

impl quote::ToTokens for Element {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Element {
            open,
            children,
            close: _close,
        } = self;
        let init = quote! { #open };
        let children = quote! { #children };
        tokens.extend(quote! {
            #init #children
        });
    }
}

impl quote::ToTokens for Children {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        if self.0.is_empty() {
            return;
        }
        let tok = self
            .0
            .iter()
            .map(|el| quote! { #el })
            .reduce(|acc, el| quote! {#acc, #el});
        let fncall = quote! {
            .children((#tok,))
        };
        tokens.extend(fncall);
    }
}

#[test]
fn test() {
    use std::str::FromStr;

    let input = TokenStream::from_str(r#"<block .javascript={"2" + 2 = "22"} Width(Size::Fit) />"#)
        .unwrap();
    let res = syn::parse2::<ManaElement>(input).unwrap();
    println!("{res:#?}");
}
