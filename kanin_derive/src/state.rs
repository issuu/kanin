use proc_macro::TokenStream;
use quote::quote;
use syn::{punctuated::Punctuated, token::Comma, Field, Ident};

pub(crate) fn derive_named(state_type: Ident, fields: Punctuated<Field, Comma>) -> TokenStream {
    let from_impls = fields.into_iter().map(|field| {
        let field_type = field.ty;
        let field_ident = field.ident;

        quote! {
            impl From<&#state_type> for #field_type {
                fn from(value: &#state_type) -> Self {
                    value.#field_ident.clone()
                }
            }

        }
    });

    quote! {
        #(#from_impls)*
    }
    .into()
}

pub(crate) fn derive_unnamed(state_type: Ident, fields: Punctuated<Field, Comma>) -> TokenStream {
    let from_impls = fields.into_iter().enumerate().map(|(field_idx, field)| {
        let field_type = field.ty;
        let field_idx = syn::Index::from(field_idx);

        quote! {
            impl From<&#state_type> for #field_type {
                fn from(value: &#state_type) -> Self {
                    value.#field_idx.clone()
                }
            }

        }
    });

    quote! {
        #(#from_impls)*
    }
    .into()
}
