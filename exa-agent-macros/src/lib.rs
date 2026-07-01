use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{quote, quote_spanned};
use syn::{
    parse_macro_input, Data, DeriveInput, Fields, GenericArgument, LitStr, PathArguments, Type,
};

/// Derives an inherent `into_flag_values` helper.
///
/// `#[flag(with = "path::func")]` functions must be `fn(&FieldType) -> Option<String>`.
/// Flag names resolve as `#[flag(rename = "...")]`, then `#[arg(long = "...")]`,
/// then the kebab-case field name. Bare `#[arg(long)]` keeps the kebab default.
/// Inferred field types: `String`, `Option<String>`, `bool`, and `Vec<String>`.
#[proc_macro_derive(IntoFlagValues, attributes(flag, arg))]
pub fn derive_into_flag_values(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_into_flag_values(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_into_flag_values(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    &data.fields,
                    "IntoFlagValues requires a struct with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "IntoFlagValues can only be derived for structs",
            ));
        }
    };

    let mut errors = None;
    let mut pushes = Vec::new();

    for field in fields {
        let Some(ident) = &field.ident else {
            continue;
        };
        let attr = match parse_flag_attr(field) {
            Ok(attr) => attr,
            Err(err) => {
                push_error(&mut errors, err);
                continue;
            }
        };
        if attr.skip {
            continue;
        }

        let flag_name = attr
            .rename
            .map(|(name, _)| name)
            .or(attr.arg_long)
            .unwrap_or_else(|| default_flag_name(ident));
        let flag = LitStr::new(&flag_name, ident.span());
        let value = match attr.with_path {
            Some((path, span)) => quote_spanned! {span=> #path(&self.#ident) },
            None if is_type_named(&field.ty, "String") => quote! { Some(self.#ident.clone()) },
            None if is_option_of(&field.ty, "String") => quote! { self.#ident.clone() },
            None if is_type_named(&field.ty, "bool") => {
                quote! { self.#ident.then(|| "true".to_string()) }
            }
            None if is_vec_of(&field.ty, "String") => {
                quote! { (!self.#ident.is_empty()).then(|| crate::request::encode_str_array(&self.#ident)) }
            }
            None => {
                push_error(
                    &mut errors,
                    syn::Error::new_spanned(
                        &field.ty,
                        format!(
                            "field `{}` requires #[flag(with = \"path::fn\")] or #[flag(skip)]",
                            default_flag_name(ident).replace('-', "_")
                        ),
                    ),
                );
                continue;
            }
        };
        pushes.push(quote! { values.push((#flag, #value)); });
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    let capacity = pushes.len();
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            pub fn into_flag_values(&self) -> Vec<(&'static str, Option<String>)> {
                let mut values = Vec::with_capacity(#capacity);
                #(#pushes)*
                values
            }
        }
    })
}

#[derive(Default)]
struct FlagAttr {
    skip: bool,
    rename: Option<(String, Span)>,
    with_path: Option<(syn::Path, Span)>,
    arg_long: Option<String>,
}

fn parse_flag_attr(field: &syn::Field) -> syn::Result<FlagAttr> {
    let mut out = FlagAttr::default();
    for attr in &field.attrs {
        if attr.path().is_ident("flag") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("skip") {
                    out.skip = true;
                    return Ok(());
                }
                if meta.path.is_ident("rename") {
                    let lit: LitStr = meta.value()?.parse()?;
                    out.rename = Some((lit.value(), lit.span()));
                    return Ok(());
                }
                if meta.path.is_ident("with") {
                    let lit: LitStr = meta.value()?.parse()?;
                    out.with_path = Some((lit.parse()?, lit.span()));
                    return Ok(());
                }
                Err(meta.error("unsupported flag attribute"))
            })?;
            continue;
        }
        if attr.path().is_ident("arg") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("long") {
                    if meta.input.peek(syn::Token![=]) {
                        let lit: LitStr = meta.value()?.parse()?;
                        out.arg_long = Some(lit.value());
                    }
                    return Ok(());
                }
                if meta.input.peek(syn::Token![=]) {
                    let _ = meta.value()?.parse::<syn::Expr>()?;
                }
                Ok(())
            })?;
        }
    }
    if out.skip {
        if let Some((_, span)) = &out.with_path {
            return Err(syn::Error::new(
                *span,
                "#[flag(skip)] cannot be combined with #[flag(with = ...)]",
            ));
        }
        if let Some((_, span)) = &out.rename {
            return Err(syn::Error::new(
                *span,
                "#[flag(skip)] cannot be combined with #[flag(rename = ...)]",
            ));
        }
    }
    Ok(out)
}

fn push_error(errors: &mut Option<syn::Error>, err: syn::Error) {
    if let Some(errors) = errors {
        errors.combine(err);
    } else {
        *errors = Some(err);
    }
}

fn default_flag_name(ident: &syn::Ident) -> String {
    let name = ident.to_string();
    name.strip_prefix("r#").unwrap_or(&name).replace('_', "-")
}

fn is_type_named(ty: &Type, name: &str) -> bool {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == name),
        _ => false,
    }
}

fn is_option_of(ty: &Type, inner_name: &str) -> bool {
    is_generic_of(ty, "Option", inner_name)
}

fn is_vec_of(ty: &Type, inner_name: &str) -> bool {
    is_generic_of(ty, "Vec", inner_name)
}

fn is_generic_of(ty: &Type, outer_name: &str, inner_name: &str) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    let Some(segment) = path.path.segments.last() else {
        return false;
    };
    if segment.ident != outer_name {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return false;
    };
    matches!(
        args.args.first(),
        Some(GenericArgument::Type(inner)) if is_type_named(inner, inner_name)
    )
}
