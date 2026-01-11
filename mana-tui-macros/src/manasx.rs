use std::fmt::Display;

use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::{
    Token, parenthesized,
    parse::{Parse, discouraged::Speculative},
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

#[derive(Debug, Clone)]
struct OpenTag {
    lt: Token![<],
    data: ManaTagData,
    sl: Option<Token![/]>,
    gt: Token![>],
}

impl OpenTag {
    fn span(&self) -> Span {
        self.lt
            .span()
            .join(self.gt.span())
            .unwrap_or(self.data.ident.span())
    }
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
    assign: Option<ManaAttrAssign>,
}

#[derive(Debug, Clone)]
struct ManaAttrAssign {
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
impl_parse_enum!(
    ManaAttrValue {
        Lit(syn::Lit),
        ExprTuple(syn::ExprTuple),
        ExprBlock(syn::ExprBlock),
    }
);

#[derive(Debug, Clone)]
struct ManaAttrVec(Vec<ManaAttr>);

#[derive(Debug, Clone)]
struct Component(ComponentExpr);

#[derive(Debug, Clone)]
enum ComponentExpr {
    Call(syn::ExprCall),
    PathCall(PathCall),
    Block(syn::ExprBlock),
    Cast(syn::ExprCast),
    Const(syn::ExprConst),
    If(syn::ExprIf),
    Index(syn::ExprIndex),
    Lit(syn::ExprLit),
    Macro(syn::ExprMacro),
    Match(syn::ExprMatch),
    MethodCall(syn::ExprMethodCall),
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
    Call(syn::ExprCall),
    PathCall(PathCall),
    Block(syn::ExprBlock),
    Cast(syn::ExprCast),
    Const(syn::ExprConst),
    If(syn::ExprIf),
    Index(syn::ExprIndex),
    Lit(syn::ExprLit),
    Macro(syn::ExprMacro),
    Match(syn::ExprMatch),
    MethodCall(syn::ExprMethodCall),
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
    Call,
    PathCall,
    Block,
    Cast,
    Const,
    If,
    Index,
    Lit,
    Macro,
    Match,
    MethodCall,
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
struct PathCall {
    path: syn::Path, // Padding::uniform  or  just  uniform
    _paren: syn::token::Paren,
    args: syn::punctuated::Punctuated<syn::Expr, Token![,]>,
}

impl Parse for PathCall {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let path: syn::Path = input.call(syn::Path::parse_mod_style)?; // or Path::parse if you want turbofish

        let content;
        let paren = parenthesized!(content in input);

        let args = syn::punctuated::Punctuated::parse_terminated(&content)?;

        Ok(PathCall {
            path,
            _paren: paren,
            args,
        })
    }
}

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
enum Children {
    Block(ChildrenBlock),
    List(ChildrenList),
}

#[derive(Debug, Clone)]
struct ChildrenBlock(BraceBlock);

#[derive(Debug, Clone)]
struct ChildrenList(Vec<ManaElement>);

impl_parse_enum!(Children {
    Block(ChildrenBlock),
    List(ChildrenList),
});
impl_quote_enum!(Children { Block, List });

#[derive(Debug, Clone)]
struct BraceBlock {
    _brace: syn::token::Brace,
    block: Vec<syn::Stmt>,
}

#[derive(Debug, Clone)]
pub enum ManaElement {
    Plaintext(syn::LitStr),
    ExprBlock(BraceBlock),
    Element(Box<Element>),
    SelfClosing(OpenTag),
}

impl Parse for ManaElement {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let f = input.fork();
        let exprblock = f.parse::<BraceBlock>();
        if let Ok(exprblock) = exprblock {
            input.advance_to(&f);
            return Ok(Self::ExprBlock(exprblock));
        }

        let f = input.fork();
        let litstr = f.parse::<syn::LitStr>();
        if let Ok(litstr) = litstr {
            input.advance_to(&f);
            return Ok(Self::Plaintext(litstr));
        }

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
        Ok(Self::Element(Box::new(Element {
            open,
            children,
            close,
        })))
    }
}

impl Parse for ChildrenList {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut ret = Vec::new();
        while let Ok(child) = input.parse() {
            ret.push(child);
        }

        Ok(ChildrenList(ret))
    }
}

impl Parse for BraceBlock {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let content;
        let brace = syn::braced!(content in input);
        let stmts = content.call(|tokens| syn::Block::parse_within(tokens))?;
        Ok(Self {
            _brace: brace,
            block: stmts,
        })
    }
}

impl Parse for ChildrenBlock {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self(input.parse()?))
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
            assign: parse_any(input).next(),
        })
    }
}

impl Parse for ManaAttrAssign {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
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
            lt: input.parse()?,
            data: input.parse()?,
            sl: input.parse()?,
            gt: input.parse()?,
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
            Self::Plaintext(text) => tokens.extend(quote! {
                {
                    use ::ratatui::text::Text;
                    __ui_internal(Text::raw(format!(#text)).into_view()).done()
                }
            }),
            ManaElement::Element(element) => {
                tokens.extend(quote! { #element.done() });
            }
            ManaElement::SelfClosing(open_tag) => {
                let span = open_tag.span();
                tokens.extend(quote_spanned! { span => { #open_tag.done() } });
            }
            ManaElement::ExprBlock(expr_block) => {
                tokens.extend(quote! {
                    __ui_internal(#expr_block .into_view()).done()
                });
            }
        }
    }
}

impl quote::ToTokens for OpenTag {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let OpenTag {
            lt: _lt,
            data,
            sl: _sl,
            gt: _gt,
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
            __ui_internal(#ident::default() #attrs .into_view())#components
        };
        tokens.extend(out);
    }
}

impl quote::ToTokens for PathCall {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let PathCall { path, _paren, args } = self;
        let args = args.iter();
        let tok = quote! {
            #path(#(#args),*)
        };
        tokens.extend(tok);
    }
}

impl quote::ToTokens for ComponentVec {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        if self.0.is_empty() {
            return;
        }
        let result = self
            .0
            .iter()
            .map(|component| {
                let Component(c_expr) = component;
                match c_expr {
                    ComponentExpr::Tuple(c_expr) => {
                        quote! {
                            .with(#c_expr)
                        }
                    }
                    _ => {
                        quote! {
                            .with((#c_expr,))
                        }
                    }
                }
            })
            .reduce(|acc, el| quote! {#acc #el});
        tokens.extend(result);
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
            assign,
        } = self;
        let tok = quote! {
            .#fn_name(
                #assign
            )
        };
        tokens.extend(tok);
    }
}

impl quote::ToTokens for ManaAttrAssign {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ManaAttrAssign { _eq, value } = self;
        let tok = quote! {
            #[allow(unused_braces)]
            {
                #value
            }
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

impl quote::ToTokens for ChildrenList {
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

impl quote::ToTokens for BraceBlock {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let BraceBlock { _brace, block } = self;
        let expr = quote! {#(#block)*};
        let fncall = quote! {
            #[allow(unused_braces)]
            {
                #expr
            }
        };
        tokens.extend(fncall);
    }
}

impl quote::ToTokens for ChildrenBlock {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ChildrenBlock(block) = self;
        let fncall = quote! {
            .children(
                #block
            )
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
