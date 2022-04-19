/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Function definitions for a `ComponentInterface`.
//!
//! This module converts function definitions from UDL into structures that
//! can be added to a `ComponentInterface`. A declaration in the UDL like this:
//!
//! ```
//! # let ci = uniffi_bindgen::interface::ComponentInterface::from_webidl(r##"
//! namespace example {
//!     string hello();
//! };
//! # "##)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! Will result in a [`Function`] member being added to the resulting [`ComponentInterface`]:
//!
//! ```
//! # use uniffi_bindgen::interface::Type;
//! # let ci = uniffi_bindgen::interface::ComponentInterface::from_webidl(r##"
//! # namespace example {
//! #     string hello();
//! # };
//! # "##)?;
//! let func = ci.get_function_definition("hello").unwrap();
//! assert_eq!(func.name(), "hello");
//! assert!(matches!(func.return_type(), Some(Type::String)));
//! assert_eq!(func.arguments().len(), 0);
//! # Ok::<(), anyhow::Error>(())
//! ```
use std::hash::{Hash, Hasher};

use anyhow::Result;

use super::attributes::FunctionAttributes;
use super::ffi::{FFIArgument, FFIFunction};
use super::literal::Literal;
use super::types::{Type, TypeIterator};

/// Represents a standalone function.
///
/// Each `Function` corresponds to a standalone function in the rust module,
/// and has a corresponding standalone function in the foreign language bindings.
///
/// In the FFI, this will be a standalone function with appropriately lowered types.
#[derive(Debug, Clone)]
pub struct Function {
    pub(super) name: String,
    pub(super) arguments: Vec<Argument>,
    pub(super) return_type: Option<Type>,
    pub(super) ffi_func: FFIFunction,
    pub(super) attributes: FunctionAttributes,
}

impl Function {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn arguments(&self) -> Vec<&Argument> {
        self.arguments.iter().collect()
    }

    pub fn full_arguments(&self) -> Vec<Argument> {
        self.arguments.to_vec()
    }

    pub fn return_type(&self) -> Option<&Type> {
        self.return_type.as_ref()
    }

    pub fn ffi_func(&self) -> &FFIFunction {
        &self.ffi_func
    }

    pub fn throws(&self) -> Option<&str> {
        self.attributes.get_throws_err()
    }

    pub fn throws_type(&self) -> Option<Type> {
        self.attributes
            .get_throws_err()
            .map(|name| Type::Error(name.to_owned()))
    }

    pub fn derive_ffi_func(&mut self, ci_prefix: &str) -> Result<()> {
        self.ffi_func.name = format!("{}_{}", ci_prefix, self.name);
        self.ffi_func.arguments = self.arguments.iter().map(|arg| arg.into()).collect();
        self.ffi_func.return_type = self.return_type.as_ref().map(|rt| rt.into());
        Ok(())
    }
}

impl From<uniffi_meta::FnMetadata> for Function {
    fn from(meta: uniffi_meta::FnMetadata) -> Self {
        if !meta.inputs.is_empty() {
            unimplemented!("TODO(jplatte)");
        }

        // FIXME(jplatte): add type assertions to ensure these names aren't shadowed!
        // TODO(jplatte): add support for attributes on parameters that customize the type repr
        let return_type = meta.output.map(|out| match out.as_str() {
            "u8" => Type::UInt8,
            "u16" => Type::UInt16,
            "u32" => Type::UInt32,
            "u64" => Type::UInt64,
            "i8" => Type::Int8,
            "i16" => Type::Int16,
            "i32" => Type::Int32,
            "i64" => Type::Int64,
            "f32" => Type::Float32,
            "f64" => Type::Float64,
            "bool" => Type::Boolean,
            "String" => Type::String,
            _ => unimplemented!("TODO(jplatte)"),
            //_ => Type::Object(out),
        });

        Self {
            name: meta.name.clone(),
            arguments: Vec::new(),
            return_type,
            ffi_func: FFIFunction {
                name: format!("__uniffi_{}", meta.name),
                arguments: Vec::new(),
                return_type: None,
            },
            attributes: FunctionAttributes(Vec::new()),
        }
    }
}

impl Hash for Function {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // We don't include the FFIFunc in the hash calculation, because:
        //  - it is entirely determined by the other fields,
        //    so excluding it is safe.
        //  - its `name` property includes a checksum derived from  the very
        //    hash value we're trying to calculate here, so excluding it
        //    avoids a weird circular depenendency in the calculation.
        self.name.hash(state);
        self.arguments.hash(state);
        self.return_type.hash(state);
        self.attributes.hash(state);
    }
}

/// Represents an argument to a function/constructor/method call.
///
/// Each argument has a name and a type, along with some optional metadata.
#[derive(Debug, Clone, Hash)]
pub struct Argument {
    pub(super) name: String,
    pub(super) type_: Type,
    pub(super) by_ref: bool,
    pub(super) optional: bool,
    pub(super) default: Option<Literal>,
}

impl Argument {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn type_(&self) -> Type {
        self.type_.clone()
    }

    pub fn by_ref(&self) -> bool {
        self.by_ref
    }

    pub fn default_value(&self) -> Option<Literal> {
        self.default.clone()
    }

    pub fn iter_types(&self) -> TypeIterator<'_> {
        self.type_.iter_types()
    }
}

impl From<&Argument> for FFIArgument {
    fn from(a: &Argument) -> FFIArgument {
        FFIArgument {
            name: a.name.clone(),
            type_: (&a.type_).into(),
        }
    }
}
