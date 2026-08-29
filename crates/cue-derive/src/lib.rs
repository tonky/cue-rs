use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Expr, Lit};

#[proc_macro_derive(CueValidate, attributes(cue))]
pub fn derive_cue_validate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let mut schema_str = None;
    let mut file_path = None;

    for attr in &input.attrs {
        if attr.path().is_ident("cue") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("schema") {
                    let value: Expr = meta.value()?.parse()?;
                    if let Expr::Lit(expr_lit) = value
                        && let Lit::Str(s) = expr_lit.lit {
                            schema_str = Some(s.value());
                        }
                } else if meta.path.is_ident("file") {
                    let value: Expr = meta.value()?.parse()?;
                    if let Expr::Lit(expr_lit) = value
                        && let Lit::Str(s) = expr_lit.lit {
                            file_path = Some(s.value());
                        }
                }
                Ok(())
            });
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

    let expanded = quote! {
        impl #name {
            pub fn cue_validate(&self) -> Result<(), String> {
                let json_val = serde_json::to_value(self)
                    .map_err(|e| format!("JSON serialization error during CUE validation: {e}"))?;
                #validation_logic
            }
        }
    };

    TokenStream::from(expanded)
}
