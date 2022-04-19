use std::env;
use std::path::{Path, PathBuf};

use fs_err as fs;
use once_cell::sync::Lazy;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{FnArg, Item, ItemFn, Pat, ReturnType};
use uniffi_meta::{EnumMetadata, FnMetadata, StructMetadata};

// TODO(jplatte): Ensure no generics, no async, …
// TODO(jplatte): Aggregate errors instead of short-circuiting, whereever possible

static METADATA_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set");
    let metadata_dir = Path::new(&manifest_dir).join(".uniffi").join("metadata");

    fs::create_dir_all(&metadata_dir).unwrap();
    metadata_dir
});

pub fn write_metadata(item: &Item, mod_path: &str) -> syn::Result<()> {
    let dir: &Path = &METADATA_DIR;

    let res = match item {
        Item::Enum(e) => EnumMetadata::new(e)?.write_to(dir),
        Item::Fn(f) => FnMetadata::new(f, mod_path)?.write_to(dir),
        //Item::Impl(i) => ImplMetadata::new(i)?.write_to(dir),
        Item::Impl(_) => {
            return Err(syn::Error::new(
                Span::call_site(),
                "support for impl blocks coming soon",
            ))
        }
        Item::Struct(s) => StructMetadata::new(s)?.write_to(dir),
        // FIXME: Support const / static?
        _ => {
            return Err(syn::Error::new(
                Span::call_site(),
                "unsupported item: only functions, structs, enums and impl \
                 blocks may be annotated with this attribute",
            ));
        }
    };

    if let Err(io_error) = res {
        return Err(syn::Error::new(
            Span::call_site(),
            format!("failed to write file: {}", io_error),
        ));
    }

    Ok(())
}

pub fn gen_scaffolding(item: &Item, mod_path: &str) -> syn::Result<TokenStream> {
    match item {
        Item::Enum(e) => {
            todo!()
        }
        Item::Fn(f) => gen_fn_scaffolding(f, mod_path),
        //Item::Impl(i) => ImplMetadata::new(i)?.write_to(dir),
        Item::Impl(_) => Err(syn::Error::new(
            Span::call_site(),
            "support for impl blocks coming soon",
        )),
        Item::Struct(s) => {
            todo!()
        }
        // FIXME: Support const / static?
        _ => Err(syn::Error::new(
            Span::call_site(),
            "unsupported item: only functions, structs, enums and impl \
             blocks may be annotated with this attribute",
        )),
    }
}

fn gen_fn_scaffolding(item: &ItemFn, mod_path: &str) -> syn::Result<TokenStream> {
    let name = &item.sig.ident;
    let name_s = name.to_string();
    let ffi_name = format_ident!("__uniffi_{}_{}", mod_path, name);

    let (params, args): (Vec<_>, Vec<_>) = item
        .sig
        .inputs
        .iter()
        .enumerate()
        .map(|(i, arg)| match arg {
            FnArg::Receiver(_) => unimplemented!("TODO(jplatte)"),
            FnArg::Typed(pat_ty) => {
                let ty = &pat_ty.ty;
                let name = format_ident!("arg{}", i);

                let param = quote! { #name: <#ty as ::uniffi::FfiConverter>::FfiType };

                let panic_fmt = match &*pat_ty.pat {
                    Pat::Ident(i) => {
                        format!("Failed to convert arg '{}': {{}}", i.ident)
                    }
                    _ => {
                        format!("Failed to convert arg #{}: {{}}", i)
                    }
                };
                let arg = quote! {
                    <#ty as ::uniffi::FfiConverter>::try_lift(#name).unwrap_or_else(|err| {
                        ::std::panic!(#panic_fmt, err)
                    })
                };

                (param, arg)
            }
        })
        .unzip();
    let fn_call = quote! {
        #name(#(#args),*)
    };

    // FIXME(jplatte): Use an extra trait implemented for `T: FfiConverter` as
    // well as `()` so no different codegen is needed?
    let (output, return_expr);
    match &item.sig.output {
        ReturnType::Default => {
            output = None;
            return_expr = fn_call;
        }
        ReturnType::Type(_, ty) => {
            output = Some(quote! {
                -> <#ty as ::uniffi::FfiConverter>::FfiType
            });
            return_expr = quote! {
                <#ty as ::uniffi::FfiConverter>::lower(#fn_call)
            };
        }
    }

    Ok(quote! {
        #[doc(hidden)]
        #[no_mangle]
        pub extern "C" fn #ffi_name(
            #(#params,)*
            call_status: &mut ::uniffi::RustCallStatus,
        ) #output {
            ::uniffi::deps::log::debug!(#name_s);
            ::uniffi::call_with_output(call_status, || {
                #return_expr
            })
        }
    })
}
