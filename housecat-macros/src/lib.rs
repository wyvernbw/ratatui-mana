extern crate proc_macro;
use unsynn::*;

// #[proc_macro_attribute]
// pub fn component(
//     _: proc_macro::TokenStream,
//     item: proc_macro::TokenStream,
// ) -> proc_macro::TokenStream {
//     // Define grammar using unsynn! macro
//     unsynn! {
//         keyword FnWord = "fn";
//         keyword Static = "&'static";
//         keyword Pub = "pub";
//         keyword Impl = "impl";

//         operator Arrow = "->";

//         // Visibility modifier (optional)
//         struct Visibility(Option<Pub>);

//         // Parameter: name: type
//         struct Parameter {
//             name: Ident,
//             _colon: Colon,
//             _static: Static,
//             param_type: Ident,
//         }

//         // Return type: -> Type
//         struct ReturnType {
//             _arrow: Arrow,
//             impl_word: Impl,
//             trait_name: Ident,
//         }

//         // Complete function definition
//         pub struct FunctionDef {
//             visibility: Visibility,
//             _fn_keyword: FnWord,
//             name: Ident,
//             params: ParenthesisGroupContaining<CommaDelimitedVec<Parameter>>,
//             return_type: Option<ReturnType>,
//             body: BraceGroup,
//         }
//     };
//     let attr = TokenStream::from(item);
//     let def: FunctionDef = attr.to_token_iter().parse().unwrap();
//     let struct_name = {
//         let name = def.name.to_string();
//         let first_letter = name.chars().next().expect("Function name cannot be empty");
//         first_letter.to_ascii_uppercase().to_string() + &name[1..]
//     };
//     let vis = def.visibility;
//     let name = def.name;
//     let body = def.body;
//     let params = def.params.content;

//     quote! {
//         #[derive(Debug)]
//         #vis struct #struct_name(Component);

//         impl ComponentMarker for #struct_name {
//             fn render(&self, area: Rect, buf: &mut Buffer) {
//                 self.0.render(area, buf)
//             }
//             fn sizing(&self) -> Option<Constraint> {
//                 self.0.sizing()
//             }
//             fn layout(&self, sizes: &[Constraint]) -> Option<Layout> {
//                 self.0.layout(sizes)
//             }
//         }

//         #vis fn #name(#params) -> #struct_name {
//             #struct_name({#body}.into_component())
//         }
//     }
//     .to_token_stream()
//     .into()
// }

use proc_macro::TokenStream;
use quote::quote;
use syn::{Ident, ItemFn, parse_macro_input};

#[proc_macro_attribute]
pub fn component(_: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);

    let struct_name = {
        let name = func.sig.ident.to_string();
        let first_letter = name.chars().next().expect("Function name cannot be empty");
        Ident::new(
            &(first_letter.to_ascii_uppercase().to_string() + &name[1..]),
            func.sig.ident.span(),
        )
    };

    let vis = &func.vis;
    let name = &func.sig.ident;
    let inputs = &func.sig.inputs;
    let body = &func.block;

    quote! {


        #[derive(Debug)]
        #vis struct #struct_name(Component);

        impl ComponentMarker for #struct_name {
            fn render(&self, area: Rect, buf: &mut Buffer) {
                self.0.render(area, buf)
            }
            fn sizing(&self) -> Option<Constraint> {
                self.0.sizing()
            }
            fn layout(&self, sizes: &[Constraint]) -> Option<Layout> {
                self.0.layout(sizes)
            }
        }

        #vis fn #name(#inputs) -> #struct_name {
            #struct_name({#body}.into_component())
        }


    }
    .into()
}
