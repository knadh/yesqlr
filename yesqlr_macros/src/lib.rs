extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Lit, Meta};

#[proc_macro_derive(ScanQueries, attributes(name))]
pub fn scan_queries_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let expanded = match generate_try_from(&input) {
        Ok(tokens) => tokens,
        Err(e) => return e.to_compile_error().into(),
    };

    TokenStream::from(expanded)
}

fn generate_try_from(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let fields = if let syn::Data::Struct(ref data_struct) = input.data {
        &data_struct.fields
    } else {
        return Err(syn::Error::new_spanned(
            input,
            "ScanQueries can only be derived for structs",
        ));
    };

    let mut map = Vec::new();
    let name = &input.ident;

    for field in fields.iter() {
        let field_name = field.ident.as_ref().unwrap();

        // Use the field's name as the key by default.
        let mut query_name = field_name.to_string();

        for attr in &field.attrs {
            // If there's a #[name = "..."] attribute, use that as the name.
            if attr.path.is_ident("name") {
                if let Ok(Meta::NameValue(meta)) = attr.parse_meta() {
                    if let Lit::Str(lit_str) = meta.lit {
                        query_name = lit_str.value();
                    }
                }
            }
        }

        map.push((field_name, query_name));
    }

    let extract_fields = map.iter().map(|(field, key)| {
        quote! {
            #field: {
                if let Some(query) = queries.remove(#key) {
                    query
                } else {
                    Default::default()
                }
            }
        }
    });

    let expanded = quote! {
        impl std::convert::TryFrom<yesqlr::Queries> for #name {
            type Error = String;

            fn try_from(mut queries: yesqlr::Queries) -> Result<Self, Self::Error> {
                Ok(Self {
                    #(#extract_fields,)*
                })
            }
        }
    };

    Ok(expanded)
}
