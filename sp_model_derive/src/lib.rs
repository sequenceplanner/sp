use proc_macro::{self, TokenStream};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    parse_macro_input, punctuated::Punctuated, Data, DataStruct, DeriveInput, Fields, Ident,
    MetaNameValue, Token,
};

#[proc_macro_derive(Resource, attributes(Variable, Resource))]
pub fn derive_resource(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let fields = match &input.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields),
            ..
        }) => &fields.named,
        _ => panic!("expected a struct with named fields"),
    };

    let field_vars: Vec<(syn::Ident, TokenStream2)> = fields
        .iter()
        .flat_map(|field| {
            let field_ident: Ident = field.ident.clone()?;
            let attr = single_value(field.attrs.iter())?;
            if !attr.path.is_ident("Variable") {
                return None;
            }

            let name_values: Result<Punctuated<MetaNameValue, Token![,]>, _> =
                attr.parse_args_with(Punctuated::parse_terminated);

            let mut var_type: Option<TokenStream2> = None;
            let mut initial: Option<TokenStream2> = None;
            let mut domain: Option<String> = None;
            let mut is_string = false;

            // TODO: clean up, remove panics.
            if let Ok(name_value) = name_values {
                for nv in name_value {
                    if nv.path.get_ident().map(|i| i.to_string()) == Some("type".into()) {
                        let value = match &nv.lit {
                            syn::Lit::Str(v) => v.value(),
                            _ => "expeced a string value".into(), // handle this err and don't panic
                        };

                        var_type = match value.to_ascii_lowercase().as_str() {
                            "string" => {
                                is_string = true;
                                Some(quote!(SPValueType::String))
                            }
                            "bool" => Some(quote!(SPValueType::Bool)),
                            "int" => Some(quote!(SPValueType::Int32)),
                            "float" => Some(quote!(SPValueType::Float32)),
                            _ => panic!("must have a type"),
                        };
                    }

                    if nv.path.get_ident().map(|i| i.to_string()) == Some("initial".into()) {
                        let value = match &nv.lit {
                            syn::Lit::Str(_) => &nv.lit,
                            syn::Lit::Bool(_) => &nv.lit,
                            syn::Lit::Int(_) => &nv.lit,
                            syn::Lit::Float(_) => &nv.lit,
                            _ => panic!("expeced a string value"),
                        };
                        initial = Some(quote!(#value . to_spvalue()));
                    }

                    if nv.path.get_ident().map(|i| i.to_string()) == Some("domain".into()) {
                        let value = match &nv.lit {
                            syn::Lit::Str(v) => v.value(),
                            _ => "expeced a string value".into(),
                        };
                        domain = Some(value);
                    }
                }
            }

            let var_type = var_type.unwrap();

            let domain = if let Some(domain) = &domain {
                let vec_str = domain
                    .split(' ')
                    .map(|s| {
                        if is_string {
                            format!("\"{s}\".to_spvalue()")
                        } else {
                            format!("{s}.to_spvalue()")
                        }
                    })
                    .collect::<Vec<String>>()
                    .join(",");
                let v = format!("vec! [ {vec_str} ]");
                use std::str::FromStr;
                TokenStream2::from_str(&v).unwrap()
            } else {
                quote!(vec![])
            };

            let name = quote!(format!("{}/{}", name, stringify!(#field_ident)));
            let var = if let Some(val) = initial {
                quote!({
                    let mut v = Variable::new(& #name, #var_type, #domain);
                    v.initial_state = #val;
                    v
                })
            } else {
                quote!(Variable::new(& #name, #var_type, #domain))
            };
            Some((field_ident, var))
        })
        .collect();

    let nested: Vec<(Ident, TokenStream2)> = fields
        .iter()
        .flat_map(|field| {
            let field_ident: Ident = field.ident.clone()?;
            let attr = single_value(field.attrs.iter())?;
            if !attr.path.is_ident("Resource") {
                return None;
            }

            let ty = &field.ty;
            let name = quote!(&format!("{}/{}", name, stringify!(#field_ident)));
            Some((field_ident, quote!(#ty :: new(#name))))
        })
        .collect();

    let variables: Vec<TokenStream2> = field_vars
        .iter()
        .map(|(f, _)| quote!(self . #f . clone()))
        .collect();

    let make_fields: Vec<TokenStream2> = field_vars
        .into_iter()
        .map(|(f, v)| quote!(#f : #v))
        .collect();

    let nested_variables: Vec<TokenStream2> = nested
        .iter()
        .map(|(f, _)| quote!(self . #f . get_variables()))
        .collect();

    let make_nested: Vec<TokenStream2> = nested.into_iter().map(|(f, v)| quote!(#f : #v)).collect();

    let struct_name = &input.ident;
    quote! {
        impl #struct_name {
            fn new(name: &str) -> Self {
                Self {
                    #( #make_fields , )*
                    #( #make_nested , )*
                }
            }

            fn get_variables(&self) -> Vec<Variable> {
                let mut vars: Vec<Variable> = vec![];
                #( vars.push(#variables); )*
                #( vars.extend(#nested_variables); )*
                return vars;
            }
        }
    }
    .into()
}

fn single_value<T>(mut it: impl Iterator<Item = T>) -> Option<T> {
    if let Some(result) = it.next() {
        if it.next().is_none() {
            return Some(result);
        }
    }
    None
}
