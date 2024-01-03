mod from_error;
mod state;

use proc_macro::TokenStream;
use syn::{DataEnum, DeriveInput, FieldsNamed, FieldsUnnamed};

/// Derives `From<&S>` for all the fields in the `S` struct.
#[proc_macro_derive(AppState)]
pub fn derive_state_from(tokens: TokenStream) -> TokenStream {
    // Parse the input type.
    let abstract_syntax_tree: DeriveInput =
        syn::parse(tokens).expect("could not parse derive macro input");

    let name = abstract_syntax_tree.ident;

    match abstract_syntax_tree.data {
        syn::Data::Struct(s) => match s.fields {
            syn::Fields::Unit => panic!("unit structs are not supported"),
            syn::Fields::Unnamed(FieldsUnnamed { unnamed, .. }) => {
                state::derive_unnamed(name, unnamed)
            }
            syn::Fields::Named(FieldsNamed { named, .. }) => state::derive_named(name, named),
        },
        syn::Data::Enum(DataEnum { .. }) => panic!(
            "enums are currently not supported (but could be, please shout if you need this)"
        ),
        _ => panic!("only structs supported"),
    }
}

/// Derives the `kanin::error::FromError` trait for a type. This only works under specific circumstances.
///
/// If the type is a tuple struct, it must have a single member that also implements FromError.
///
/// If the type is a struct with named fields, it must have exactly one field that also implements FromError,
/// _except_ if the struct's name contains InternalError or InvalidRequest, in which case FromError will be implemented specially,
/// by assuming the structure of the type to match the expected structure.
///
/// The expected structure is:
/// ```
/// struct InvalidRequest {
///     error: String,
/// }
///
/// struct InternalError {
///     /// The source is the app ID of the service in which the error originated.
///     source: String,
///     error: String,
/// }
/// ```
#[proc_macro_derive(FromError)]
pub fn from_error_derive(tokens: TokenStream) -> TokenStream {
    // Parse the input type.
    let abstract_syntax_tree: DeriveInput =
        syn::parse(tokens).expect("could not parse derive macro input");

    let name = abstract_syntax_tree.ident;
    match abstract_syntax_tree.data {
        syn::Data::Struct(s) => match s.fields {
            syn::Fields::Unit => panic!("unit structs are not supported"),
            syn::Fields::Unnamed(FieldsUnnamed { unnamed, .. }) => {
                from_error::derive_unnamed(name, unnamed)
            }
            syn::Fields::Named(FieldsNamed { named, .. }) => from_error::derive_named(name, named),
        },
        syn::Data::Enum(DataEnum { variants, .. }) => from_error::derive_enum(name, variants),
        _ => panic!("only structs and enums are supported"),
    }
}
