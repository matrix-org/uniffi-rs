#[cfg(not(feature = "nightly"))]
pub fn mod_path() -> String {
    // Full mod path since that would require awful hacks w/o TokenStream::expand_expr
    compile_error!("TODO(jplatte): Get the crate name from Cargo.toml")
}

#[cfg(feature = "nightly")]
pub fn mod_path() -> String {
    use proc_macro::TokenStream;
    use quote::quote;

    let module_path_invoc = TokenStream::from(quote! { ::core::module_path!() });
    let expanded_module_path = TokenStream::expand_expr(&module_path_invoc).unwrap();
    syn::parse::<syn::LitStr>(expanded_module_path)
        .unwrap()
        .value()
}
