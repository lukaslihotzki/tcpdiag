use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, parse_quote, DataStruct, DeriveInput, Expr, Fields, PatType};

fn derive_serialize_with_context_int(input: &syn::DeriveInput) -> proc_macro2::TokenStream {
    let name = &input.ident;
    let syn::Data::Struct(DataStruct { fields, .. }) = &input.data else {
        panic!("serde-context-derive can only be used on structs.")
    };
    let Fields::Named(fields) = &fields else {
        panic!("SerializeWithContext only supports structs with named fields")
    };

    let context_arg = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("context"))
        .map(|attr| {
            attr.parse_args::<PatType>()
                .expect("Failed to parse context attribute")
        });

    let (context_name, context_type) = if let Some(pat_type) = context_arg {
        (pat_type.pat, *pat_type.ty)
    } else {
        (parse_quote!(_), parse_quote!(()))
    };

    let mut names = Vec::new();

    let field_serializations: Vec<_> =
        fields.named.iter().map(|field| {
        let field_name = field.ident.clone().unwrap();
        let pass_attr = field.attrs.iter().find(|attr| attr.path().is_ident("pass"));
        names.push(field_name.clone());

        if let Some(attr) = pass_attr {
            let pass = attr.parse_args::<Expr>().expect("pass must be an expression");
            quote! {
                serializer.serialize_field_with_context(stringify!(#field_name), &self.#field_name, &#pass)?;
            }
        } else {
            quote! {
                serializer.serialize_field(stringify!(#field_name), &self.#field_name)?;
            }
        }
    }).collect();

    let len = field_serializations.len();

    let serialize_impl = (context_type == parse_quote!(())).then(|| {
        quote! {
            impl serde::Serialize for #name {
                fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                    serde_context::SerializeWithContext::serialize(self, &(), serializer)
                }
            }
        }
    });

    quote! {
        #serialize_impl

        impl serde_context::SerializeWithContext for #name {
            type Context = #context_type;

            fn serialize<S>(&self, #context_name: &Self::Context, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                use serde::ser::SerializeStruct;
                use serde_context::SerializerExt;
                let mut serializer = serializer.serialize_struct(stringify!(#name), #len)?;
                let Self { #(ref #names),* } = self;
                #(#field_serializations)*
                serializer.end()
            }
        }
    }
}

#[proc_macro_derive(SerializeWithContext, attributes(context, pass))]
pub fn derive_serialize_with_context(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive_serialize_with_context_int(&input).into()
}
