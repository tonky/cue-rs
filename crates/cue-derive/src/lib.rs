use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, Expr, Lit, parse_macro_input};

#[proc_macro_derive(CueValidate, attributes(cue))]
pub fn derive_cue_validate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let mut schema_str = None;
    let mut file_path = None;

    for attr in &input.attrs {
        if attr.path().is_ident("cue") {
            let res = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("schema") {
                    let value: Expr = meta.value()?.parse()?;
                    if let Expr::Lit(expr_lit) = value
                        && let Lit::Str(s) = expr_lit.lit
                    {
                        schema_str = Some(s.value());
                        return Ok(());
                    }
                    Err(meta.error("expected string literal for schema"))
                } else if meta.path.is_ident("file") {
                    let value: Expr = meta.value()?.parse()?;
                    if let Expr::Lit(expr_lit) = value
                        && let Lit::Str(s) = expr_lit.lit
                    {
                        file_path = Some(s.value());
                        return Ok(());
                    }
                    Err(meta.error("expected string literal for file"))
                } else {
                    Err(meta.error("unrecognized cue attribute (expected 'schema' or 'file')"))
                }
            });
            if let Err(err) = res {
                return err.to_compile_error().into();
            }
        }
    }

    let validation_logic = if let Some(schema) = schema_str {
        quote! {
            let schema_content = #schema;
            cue_eval::validate_json(schema_content, &json_val)
                .map_err(|e| format!("CUE validation failed: {e}"))
        }
    } else if let Some(path) = file_path {
        quote! {
            let schema_content = include_str!(#path);
            cue_eval::validate_json(schema_content, &json_val)
                .map_err(|e| format!("CUE validation failed: {e}"))
        }
    } else {
        let default_schema = format!("#{name}");
        quote! {
            let schema_content = #default_schema;
            cue_eval::validate_json(schema_content, &json_val)
                .map_err(|e| format!("CUE validation failed: {e}"))
        }
    };

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics cue_eval::CueValidate for #name #ty_generics #where_clause {
            fn cue_validate(&self) -> Result<(), String> {
                let json_val = serde_json::to_value(self)
                    .map_err(|e| format!("JSON serialization error during CUE validation: {e}"))?;
                #validation_logic
            }
        }

        impl #impl_generics #name #ty_generics #where_clause {
            pub fn cue_validate(&self) -> Result<(), String> {
                <Self as cue_eval::CueValidate>::cue_validate(self)
            }
        }
    };

    TokenStream::from(expanded)
}
