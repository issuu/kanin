use proc_macro::TokenStream;
use quote::quote;
use syn::{
    punctuated::Punctuated, token::Comma, DataEnum, DeriveInput, Field, FieldsNamed, FieldsUnnamed,
    Ident, Variant,
};

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
            syn::Fields::Unnamed(FieldsUnnamed { unnamed, .. }) => derive_unnamed(name, unnamed),
            syn::Fields::Named(FieldsNamed { named, .. }) => derive_named(name, named),
        },
        syn::Data::Enum(DataEnum { variants, .. }) => derive_enum(name, variants),
        _ => panic!("only structs and enums are supported"),
    }
}

/// Derives the FromError trait for a struct with named fields.
///
/// If the struct is called `InvalidRequest` or `InternalError`, they will be handled
/// specially by implementing FromError for the appropriate kanin error types.
fn derive_named(name: Ident, fields: Punctuated<Field, Comma>) -> TokenStream {
    let name_s = name.to_string();

    if name_s.contains("InvalidRequest") {
        return derive_invalid_request(name);
    } else if name_s.contains("InternalError") {
        return derive_internal_error(name);
    }

    let num_fields = fields.len();

    if num_fields != 1 {
        panic!("structs with named field must have exactly 1 field");
    }

    let field_name = fields
        .first()
        .expect("we just checked that there is exactly 1 field")
        .ident
        .as_ref()
        .expect("field must be named since we matched on named struct");

    derive_named_newtype(name, field_name)
}

/// Derives the FromError for the InvalidRequest struct. It will use RequestError in kanin for this instead of the more general error type.
fn derive_invalid_request(name: Ident) -> TokenStream {
    quote! {
        impl ::kanin::error::FromError<::kanin::error::RequestError> for #name {
            fn from_error(error: ::kanin::error::RequestError) -> Self {
                #name {
                    error: format!("{:#}", error)
                }
            }
        }
    }
    .into()
}

fn derive_internal_error(name: Ident) -> TokenStream {
    quote! {
        impl ::kanin::error::FromError<::kanin::error::ServerError> for #name {
            fn from_error(error: ::kanin::error::ServerError) -> Self {
                #name {
                    error: format!("{:#}", error),
                    source: env!("CARGO_PKG_NAME").to_string(),
                }
            }
        }
    }
    .into()
}

/// Derives the FromError trait for a newtype struct, i.e. a tuple struct with a single unnamed field.
///
/// The field must implement FromError on its own. The implementation uses the implementation of the singular inner field.
fn derive_unnamed(name: Ident, fields: Punctuated<Field, Comma>) -> TokenStream {
    if fields.len() != 1 {
        panic!("only tuple structs with a single field are supported",);
    }

    quote! {
        impl ::kanin::error::FromError<::kanin::HandlerError> for #name {
            fn from_error(error: ::kanin::HandlerError) -> Self {
                Self(::kanin::error::FromError::from_error(error))
            }
        }
    }
    .into()
}

/// Derives the FromError trait for a struct with a single named field.
///
/// The field must implement FromError on its own. The implementation uses the implementation of the singular inner field.
fn derive_named_newtype(name: Ident, field_name: &Ident) -> TokenStream {
    quote! {
        impl ::kanin::error::FromError<::kanin::HandlerError> for #name {
            fn from_error(error: ::kanin::HandlerError) -> Self {
                Self {
                    #field_name: ::kanin::error::FromError::from_error(error)
                }
            }
        }
    }
    .into()
}

/// Derives the FromError trait for an enum with InvalidRequest and InternalError variants.
fn derive_enum(name: Ident, variants: Punctuated<Variant, Comma>) -> TokenStream {
    let invalid_request_name = &variants
        .iter()
        .find(|v| v.ident.to_string().contains("InvalidRequest"))
        .expect("enum missing a variant containing \"InvalidRequest\"")
        .ident;

    let internal_error_name = &variants
        .iter()
        .find(|v| v.ident.to_string().contains("InternalError"))
        .expect("enum missing a variant containing \"InternalError\"")
        .ident;

    quote! {
        impl ::kanin::error::FromError<::kanin::HandlerError> for #name {
            fn from_error(error: ::kanin::HandlerError) -> Self {
                match error {
                    ::kanin::HandlerError::InvalidRequest(e) => {
                        ::log::warn!("{e}");
                        Self::#invalid_request_name(::kanin::error::FromError::from_error(e))
                    },
                    ::kanin::HandlerError::Internal(e) => {
                        ::log::error!("{e}");
                        Self::#internal_error_name(::kanin::error::FromError::from_error(e))
                    },
                }
            }
        }
    }
    .into()
}
