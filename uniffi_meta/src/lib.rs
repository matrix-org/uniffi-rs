use std::io;
use std::path::Path;

use fs_err::File;
use serde::{Deserialize, Serialize};
use syn::{
    FnArg, ImplItemMethod, ItemEnum, ItemFn, ItemImpl, ItemStruct, ReturnType, Type, Variant,
};

#[derive(Deserialize, Serialize)]
pub struct EnumMetadata {
    name: String,
    variants: Vec<EnumVariantMetadata>,
}

impl EnumMetadata {
    pub fn new(e: &ItemEnum) -> syn::Result<Self> {
        Ok(Self {
            name: e.ident.to_string(),
            variants: e.variants.iter().map(EnumVariantMetadata::new).collect(),
        })
    }

    pub fn write_to(&self, dir: &Path) -> io::Result<()> {
        let path = dir.join(format!("type.{}.json", self.name));
        let file = File::create(path)?;
        serde_json::to_writer_pretty(file, self)?;

        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
pub struct EnumVariantMetadata {
    name: String,
}

impl EnumVariantMetadata {
    pub fn new(e: &Variant) -> Self {
        Self {
            name: e.ident.to_string(),
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct FnMetadata {
    pub module: String,
    pub name: String,
    pub inputs: Vec<FnParamMetadata>,
    pub output: Option<String>,
}

impl FnMetadata {
    pub fn new(f: &ItemFn, module: &str) -> syn::Result<Self> {
        let output = match &f.sig.output {
            ReturnType::Default => None,
            ReturnType::Type(_, ty) => Some(type_name(ty)?),
        };

        Ok(Self {
            module: module.to_owned(),
            name: f.sig.ident.to_string(),
            inputs: f
                .sig
                .inputs
                .iter()
                .map(|a| FnParamMetadata::new(a, false))
                .collect(),
            output,
        })
    }

    pub fn write_to(&self, dir: &Path) -> io::Result<()> {
        let path = dir.join(format!("mod.{}.fn.{}.json", self.module, self.name));
        let file = File::create(path)?;
        serde_json::to_writer_pretty(file, self)?;

        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
pub struct FnParamMetadata {}

impl FnParamMetadata {
    pub fn new(_a: &FnArg, is_method: bool) -> Self {
        Self {}
    }
}

#[derive(Deserialize, Serialize)]
pub struct ImplMetadata {
    module: String,
    self_name: String,
    fn_metadata: Vec<ImplFnMetadata>,
}

impl ImplMetadata {
    pub fn new(i: &ItemImpl, module: &str) -> syn::Result<Self> {
        let fn_metadata = i
            .items
            .iter()
            .map(|it| match it {
                syn::ImplItem::Method(m) => ImplFnMetadata::new(m),
                _ => Err(syn::Error::new_spanned(
                    it,
                    "item type not supported by uniffi::export",
                )),
            })
            .collect::<syn::Result<_>>()?;

        Ok(Self {
            module: module.to_owned(),
            self_name: type_name(&i.self_ty)?,
            fn_metadata,
        })
    }

    pub fn write_to(&self, dir: &Path) -> io::Result<()> {
        for fn_meta in &self.fn_metadata {
            let path = dir.join(format!(
                "mod.{}.impl.{}.fn.{}.json",
                self.module, self.self_name, fn_meta.name
            ));
            let file = File::create(path)?;
            serde_json::to_writer_pretty(file, fn_meta)?;
        }

        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
pub struct ImplFnMetadata {
    pub name: String,
    pub inputs: Vec<FnParamMetadata>,
    pub output: Option<String>,
}

impl ImplFnMetadata {
    pub fn new(f: &ImplItemMethod) -> syn::Result<Self> {
        let output = match &f.sig.output {
            ReturnType::Default => None,
            ReturnType::Type(_, ty) => Some(type_name(ty)?),
        };

        Ok(Self {
            name: f.sig.ident.to_string(),
            inputs: f
                .sig
                .inputs
                .iter()
                .map(|a| FnParamMetadata::new(a, false))
                .collect(),
            output,
        })
    }
}

#[derive(Deserialize, Serialize)]
pub struct StructMetadata {
    name: String,
}

impl StructMetadata {
    pub fn new(s: &ItemStruct) -> syn::Result<Self> {
        Ok(Self {
            name: s.ident.to_string(),
        })
    }

    pub fn write_to(&self, dir: &Path) -> io::Result<()> {
        let path = dir.join(format!("type.{}.json", self.name));
        let file = File::create(path)?;
        serde_json::to_writer_pretty(file, self)?;

        Ok(())
    }
}

fn type_name(ty: &Type) -> syn::Result<String> {
    match ty {
        Type::Group(g) => type_name(&g.elem),
        Type::Path(p) => {
            if p.qself.is_some() {
                return Err(syn::Error::new_spanned(
                    p,
                    "qualified self types are not currently supported by uniffi::export",
                ));
            }

            let id = p
                .path
                .get_ident()
                .ok_or_else(|| syn::Error::new_spanned(&p.path, "TODO(jplatte)"))?;

            Ok(id.to_string())
        }
        _ => Err(syn::Error::new_spanned(
            ty,
            "unsupported type syntax for uniffi::export",
        )),
    }
}
