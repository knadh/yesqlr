extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Lit, Meta};

#[proc_macro_derive(ScanQueries, attributes(key))]
pub fn scan_queries_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let expanded = match generate_try_from(&input) {
        Ok(tokens) => tokens,
        Err(e) => return e.to_compile_error().into(),
    };

    TokenStream::from(expanded)
}

fn generate_try_from(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;

    let fields = if let syn::Data::Struct(ref data_struct) = input.data {
        &data_struct.fields
    } else {
        return Err(syn::Error::new_spanned(
            input,
            "ScanQueries can only be derived for structs",
        ));
    };

    let mut map = Vec::new();

    for field in fields.iter() {
        let field_name = field.ident.as_ref().unwrap();
        let mut key = field_name.to_string();

        for attr in &field.attrs {
            if attr.path.is_ident("key") {
                if let Ok(Meta::NameValue(meta)) = attr.parse_meta() {
                    if let Lit::Str(lit_str) = meta.lit {
                        key = lit_str.value();
                    }
                }
            }
        }

        map.push((field_name, key, &field.ty));
    }

    let extract_fields = map.iter().map(|(field, key, field_type)| {
        quote! {
            #field: {
                if let Some(value) = queries.remove(#key) {
                    convert_query::<#field_type>(&value.query)
                        .map_err(|e| format!("Failed to convert key '{}': {}", #key, e))?
                } else {
                    Default::default()
                }
            }
        }
    });

    let expanded = quote! {
        impl std::convert::TryFrom<crate::Queries> for #name {
            type Error = String;

            fn try_from(mut queries: crate::Queries) -> Result<Self, Self::Error> {
                fn convert_query<T: std::str::FromStr>(value: &str) -> Result<T, String> {
                    value.parse::<T>().map_err(|_| format!("Failed to parse value: {}", value))
                }

                Ok(Self {
                    #(#extract_fields,)*
                })
            }
        }
    };

    Ok(expanded)
}
