extern crate proc_macro;

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse2, parse_macro_input, parse_quote, Attribute, DeriveInput, PatType};

fn getattr(attrs: &[Attribute], namet: &str) -> Option<TokenStream> {
    let mut unqualified_attr = None;
    let mut qualified_attr = None;

    for attr in attrs {
        if let syn::Meta::List(lst) = &attr.meta {
            // check for unqualified attributes like #[pass(...)]
            if lst
                .path
                .get_ident()
                .map(|ident| *ident == namet)
                .unwrap_or(false)
            {
                unqualified_attr = Some(lst.tokens.to_token_stream());
            }
            // check for qualified attributes like #[csv(pass(...))]
            if lst
                .path
                .get_ident()
                .map(|ident| *ident == "csv")
                .unwrap_or(false)
            {
                let mut a = lst.tokens.to_token_stream().into_iter();
                while let Some(name) = a.next() {
                    let proc_macro2::TokenTree::Ident(ident) = name else {
                        panic!()
                    };
                    if &ident.to_string()[..] == namet {
                        let proc_macro2::TokenTree::Group(g) = a.next().unwrap() else {
                            panic!()
                        };
                        qualified_attr = Some(g.stream());
                    } else {
                        a.next().unwrap();
                    }
                }
            }
        }
    }

    qualified_attr.or(unqualified_attr)
}

fn derive_csv_int(input: &syn::DeriveInput) -> proc_macro2::TokenStream {
    let base = derive_csv_write_int(input);
    let struct_name = &input.ident;
    let generics = &input.generics;
    let syn::Data::Struct(s) = &input.data else {
        panic!("derive_csv can only be used on structs.")
    };
    let names: Vec<_> = s.fields.iter().map(|f| f.ident.clone().unwrap()).collect();
    let types: Vec<_> = s.fields.iter().map(|f| &f.ty).collect();
    let t_types: Vec<_> = s
        .fields
        .iter()
        .map(|f| {
            let ty = &f.ty;
            getattr(&f.attrs, "type")
                .map(|t| syn::parse2(t).unwrap())
                .unwrap_or(quote! { #ty })
        })
        .collect();

    quote! {
        #base
        impl #generics Csv for #struct_name #generics {
            fn read<'_internal_a, I: Iterator<Item = &'_internal_a str>>(__internal_i: &mut I) -> Self {
                #(let #names = <#t_types as ::csv::Csv<#types>>::read(__internal_i);)*
                Self {
                    #(#names,)*
                }
            }
        }
    }
}

#[proc_macro_derive(Csv, attributes(csv))]
pub fn derive_csv(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive_csv_int(&input).into()
}

fn derive_csv_write_int(input: &syn::DeriveInput) -> proc_macro2::TokenStream {
    let struct_name = &input.ident;
    let generics = &input.generics;

    let (context_name, ctx) = getattr(&input.attrs, "context")
        .map(|x| parse2::<PatType>(x).unwrap())
        .map(|x| (x.pat, *x.ty))
        .unwrap_or_else(|| (parse_quote!(_), parse_quote!(())));

    let syn::Data::Struct(s) = &input.data else {
        panic!("derive_csv can only be used on structs.")
    };
    let names: Vec<_> = s.fields.iter().map(|f| f.ident.clone().unwrap()).collect();
    let (first, tail) = names.split_first().unwrap();
    let types: Vec<_> = s.fields.iter().map(|f| &f.ty).collect();
    let r_types: Vec<_> = s
        .fields
        .iter()
        .map(|f| {
            let ty = &f.ty;
            getattr(&f.attrs, "type")
                .map(|t| syn::parse2(t).unwrap())
                .unwrap_or(quote! { #ty })
        })
        .collect();
    let t_types: Vec<_> = s
        .fields
        .iter()
        .map(|f| {
            let ty = &f.ty;
            getattr(&f.attrs, "type")
                .map(|t| syn::parse2(t).unwrap())
                .unwrap_or(quote! { #ty })
        })
        .collect();
    let (rtypef, rtypet) = r_types.split_first().unwrap();
    let pass: Vec<_> = s
        .fields
        .iter()
        .map(|f| getattr(&f.attrs, "pass").unwrap_or_else(|| quote! { () }))
        .collect();
    let (passf, passt) = pass.split_first().unwrap();
    let snames: Vec<_> = s
        .fields
        .iter()
        .map(|f| {
            let name = &f.ident;
            if getattr(&f.attrs, "flatten").is_some() {
                quote! { "" }
            } else {
                quote! { stringify!(#name) }
            }
        })
        .collect();

    quote! {
        impl #generics CsvWrite for #struct_name #generics {
            type Context = #ctx;

            const DESC: csv::Desc = csv::Desc::Struct(&[
                #((#snames, &<#t_types as csv::CsvWrite<#types>>::DESC)),*
            ]);

            fn write<W: std::io::Write>(&Self { #(ref #names),* }: &Self, #context_name: &Self::Context, w: &mut W) {
                <#rtypef>::write(#first, &#passf, w);
                #(write!(w, " ").unwrap(); <#rtypet>::write(#tail, &#passt, w);)*
            }
        }
    }
}

#[proc_macro_derive(CsvWrite, attributes(csv))]
pub fn derive_csv_write(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive_csv_write_int(&input).into()
}
