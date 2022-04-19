/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Object definitions for a `ComponentInterface`.
//!
//! This module converts "interface" definitions from UDL into [`Object`] structures
//! that can be added to a `ComponentInterface`, which are the main way we define stateful
//! objects with behaviour for a UniFFI Rust Component. An [`Object`] is an opaque handle
//! to some state on which methods can be invoked.
//!
//! (The terminology mismatch between "interface" and "object" is a historical artifact of
//! this tool prior to committing to WebIDL syntax).
//!
//! A declaration in the UDL like this:
//!
//! ```
//! # let ci = uniffi_bindgen::interface::ComponentInterface::from_webidl(r##"
//! # namespace example {};
//! interface Example {
//!   constructor(string? name);
//!   string my_name();
//! };
//! # "##)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! Will result in an [`Object`] member with one [`Constructor`] and one [`Method`] being added
//! to the resulting [`ComponentInterface`]:
//!
//! ```
//! # let ci = uniffi_bindgen::interface::ComponentInterface::from_webidl(r##"
//! # namespace example {};
//! # interface Example {
//! #   constructor(string? name);
//! #   string my_name();
//! # };
//! # "##)?;
//! let obj = ci.get_object_definition("Example").unwrap();
//! assert_eq!(obj.name(), "Example");
//! assert_eq!(obj.constructors().len(), 1);
//! assert_eq!(obj.constructors()[0].arguments()[0].name(), "name");
//! assert_eq!(obj.methods().len(),1 );
//! assert_eq!(obj.methods()[0].name(), "my_name");
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! It's not necessary for all interfaces to have constructors.
//! ```
//! # let ci = uniffi_bindgen::interface::ComponentInterface::from_webidl(r##"
//! # namespace example {};
//! # interface Example {};
//! # "##)?;
//! let obj = ci.get_object_definition("Example").unwrap();
//! assert_eq!(obj.name(), "Example");
//! assert_eq!(obj.constructors().len(), 0);
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::hash::{Hash, Hasher};
use std::iter;

use anyhow::Result;

use super::attributes::{ConstructorAttributes, MethodAttributes};
use super::ffi::{FFIArgument, FFIFunction, FFIType};
use super::function::Argument;
use super::types::{Type, TypeIterator};

/// An "object" is an opaque type that can be instantiated and passed around by reference,
/// have methods called on it, and so on - basically your classic Object Oriented Programming
/// type of deal, except without elaborate inheritence hierarchies.
///
/// In UDL these correspond to the `interface` keyword.
///
/// At the FFI layer, objects are represented by an opaque integer handle and a set of functions
/// a common prefix. The object's constuctors are functions that return new objects by handle,
/// and its methods are functions that take a handle as first argument. The foreign language
/// binding code is expected to stitch these functions back together into an appropriate class
/// definition (or that language's equivalent thereof).
///
/// TODO:
///  - maybe "Class" would be a better name than "Object" here?
#[derive(Debug, Clone)]
pub struct Object {
    pub(super) name: String,
    pub(super) constructors: Vec<Constructor>,
    pub(super) methods: Vec<Method>,
    pub(super) ffi_func_free: FFIFunction,
    pub(super) uses_deprecated_threadsafe_attribute: bool,
}

impl Object {
    fn new(name: String) -> Object {
        Object {
            name,
            constructors: Default::default(),
            methods: Default::default(),
            ffi_func_free: Default::default(),
            uses_deprecated_threadsafe_attribute: false,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn type_(&self) -> Type {
        Type::Object(self.name.clone())
    }

    pub fn constructors(&self) -> Vec<&Constructor> {
        self.constructors.iter().collect()
    }

    pub fn primary_constructor(&self) -> Option<&Constructor> {
        self.constructors
            .iter()
            .find(|cons| cons.is_primary_constructor())
    }

    pub fn alternate_constructors(&self) -> Vec<&Constructor> {
        self.constructors
            .iter()
            .filter(|cons| !cons.is_primary_constructor())
            .collect()
    }

    pub fn methods(&self) -> Vec<&Method> {
        self.methods.iter().collect()
    }

    pub fn get_method(&self, name: &str) -> Method {
        let matches: Vec<_> = self.methods.iter().filter(|m| m.name() == name).collect();
        match matches.len() {
            1 => matches[0].clone(),
            n => panic!("{} methods named {}", n, name),
        }
    }

    pub fn ffi_object_free(&self) -> &FFIFunction {
        &self.ffi_func_free
    }

    pub fn uses_deprecated_threadsafe_attribute(&self) -> bool {
        self.uses_deprecated_threadsafe_attribute
    }

    pub fn iter_ffi_function_definitions(&self) -> impl Iterator<Item = &FFIFunction> {
        iter::once(&self.ffi_func_free)
            .chain(self.constructors.iter().map(|f| &f.ffi_func))
            .chain(self.methods.iter().map(|f| &f.ffi_func))
    }

    pub fn derive_ffi_funcs(&mut self, ci_prefix: &str) -> Result<()> {
        self.ffi_func_free.name = format!("ffi_{}_{}_object_free", ci_prefix, self.name);
        self.ffi_func_free.arguments = vec![FFIArgument {
            name: "ptr".to_string(),
            type_: FFIType::RustArcPtr,
        }];
        self.ffi_func_free.return_type = None;
        for cons in self.constructors.iter_mut() {
            cons.derive_ffi_func(ci_prefix, &self.name)
        }
        for meth in self.methods.iter_mut() {
            meth.derive_ffi_func(ci_prefix, &self.name)?
        }
        Ok(())
    }

    pub fn iter_types(&self) -> TypeIterator<'_> {
        Box::new(
            self.methods
                .iter()
                .map(Method::iter_types)
                .chain(self.constructors.iter().map(Constructor::iter_types))
                .flatten(),
        )
    }
}

impl Hash for Object {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // We don't include the FFIFunc in the hash calculation, because:
        //  - it is entirely determined by the other fields,
        //    so excluding it is safe.
        //  - its `name` property includes a checksum derived from  the very
        //    hash value we're trying to calculate here, so excluding it
        //    avoids a weird circular depenendency in the calculation.
        self.name.hash(state);
        self.constructors.hash(state);
        self.methods.hash(state);
    }
}

// Represents a constructor for an object type.
//
// In the FFI, this will be a function that returns a pointer to an instance
// of the corresponding object type.
#[derive(Debug, Clone)]
pub struct Constructor {
    pub(super) name: String,
    pub(super) arguments: Vec<Argument>,
    pub(super) ffi_func: FFIFunction,
    pub(super) attributes: ConstructorAttributes,
}

impl Constructor {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn arguments(&self) -> Vec<&Argument> {
        self.arguments.iter().collect()
    }

    pub fn full_arguments(&self) -> Vec<Argument> {
        self.arguments.to_vec()
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

    pub fn is_primary_constructor(&self) -> bool {
        self.name == "new"
    }

    fn derive_ffi_func(&mut self, ci_prefix: &str, obj_prefix: &str) {
        self.ffi_func.name = format!("{}_{}_{}", ci_prefix, obj_prefix, self.name);
        self.ffi_func.arguments = self.arguments.iter().map(Into::into).collect();
        self.ffi_func.return_type = Some(FFIType::RustArcPtr);
    }

    pub fn iter_types(&self) -> TypeIterator<'_> {
        Box::new(self.arguments.iter().flat_map(Argument::iter_types))
    }
}

impl Hash for Constructor {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // We don't include the FFIFunc in the hash calculation, because:
        //  - it is entirely determined by the other fields,
        //    so excluding it is safe.
        //  - its `name` property includes a checksum derived from  the very
        //    hash value we're trying to calculate here, so excluding it
        //    avoids a weird circular depenendency in the calculation.
        self.name.hash(state);
        self.arguments.hash(state);
        self.attributes.hash(state);
    }
}

impl Default for Constructor {
    fn default() -> Self {
        Constructor {
            name: String::from("new"),
            arguments: Vec::new(),
            ffi_func: Default::default(),
            attributes: Default::default(),
        }
    }
}

// Represents an instance method for an object type.
//
// The FFI will represent this as a function whose first/self argument is a
// `FFIType::RustArcPtr` to the instance.
#[derive(Debug, Clone)]
pub struct Method {
    pub(super) name: String,
    pub(super) object_name: String,
    pub(super) return_type: Option<Type>,
    pub(super) arguments: Vec<Argument>,
    pub(super) ffi_func: FFIFunction,
    pub(super) attributes: MethodAttributes,
}

impl Method {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn arguments(&self) -> Vec<&Argument> {
        self.arguments.iter().collect()
    }

    // Methods have a special implicit first argument for the object instance,
    // hence `arguments` and `full_arguments` are different.
    pub fn full_arguments(&self) -> Vec<Argument> {
        vec![Argument {
            name: "ptr".to_string(),
            // TODO: ideally we'd get this via `ci.resolve_type_expression` so that it
            // is contained in the proper `TypeUniverse`, but this works for now.
            type_: Type::Object(self.object_name.clone()),
            by_ref: !self.attributes.get_self_by_arc(),
            optional: false,
            default: None,
        }]
        .into_iter()
        .chain(self.arguments.iter().cloned())
        .collect()
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

    pub fn takes_self_by_arc(&self) -> bool {
        self.attributes.get_self_by_arc()
    }

    pub fn derive_ffi_func(&mut self, ci_prefix: &str, obj_prefix: &str) -> Result<()> {
        self.ffi_func.name = format!("{}_{}_{}", ci_prefix, obj_prefix, self.name);
        self.ffi_func.arguments = self.full_arguments().iter().map(Into::into).collect();
        self.ffi_func.return_type = self.return_type.as_ref().map(Into::into);
        Ok(())
    }

    pub fn iter_types(&self) -> TypeIterator<'_> {
        Box::new(
            self.arguments
                .iter()
                .flat_map(Argument::iter_types)
                .chain(self.return_type.iter().flat_map(Type::iter_types)),
        )
    }
}

impl Hash for Method {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // We don't include the FFIFunc in the hash calculation, because:
        //  - it is entirely determined by the other fields,
        //    so excluding it is safe.
        //  - its `name` property includes a checksum derived from  the very
        //    hash value we're trying to calculate here, so excluding it
        //    avoids a weird circular depenendency in the calculation.
        self.name.hash(state);
        self.object_name.hash(state);
        self.arguments.hash(state);
        self.return_type.hash(state);
        self.attributes.hash(state);
    }
}
