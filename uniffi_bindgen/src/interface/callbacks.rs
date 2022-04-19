/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Callback Interface definitions for a `ComponentInterface`.
//!
//! This module converts callback interface definitions from UDL into structures that
//! can be added to a `ComponentInterface`. A declaration in the UDL like this:
//!
//! ```
//! # let ci = uniffi_bindgen::interface::ComponentInterface::from_webidl(r##"
//! # namespace example {};
//! callback interface Example {
//!   string hello();
//! };
//! # "##)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! Will result in a [`CallbackInterface`] member being added to the resulting
//! [`ComponentInterface`]:
//!
//! ```
//! # let ci = uniffi_bindgen::interface::ComponentInterface::from_webidl(r##"
//! # namespace example {};
//! # callback interface Example {
//! #  string hello();
//! # };
//! # "##)?;
//! let callback = ci.get_callback_interface_definition("Example").unwrap();
//! assert_eq!(callback.name(), "Example");
//! assert_eq!(callback.methods()[0].name(), "hello");
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::hash::{Hash, Hasher};

use super::ffi::{FFIArgument, FFIFunction, FFIType};
use super::object::Method;
use super::types::{Type, TypeIterator};

#[derive(Debug, Clone)]
pub struct CallbackInterface {
    pub(super) name: String,
    pub(super) methods: Vec<Method>,
    pub(super) ffi_init_callback: FFIFunction,
}

impl CallbackInterface {
    fn new(name: String) -> CallbackInterface {
        CallbackInterface {
            name,
            methods: Default::default(),
            ffi_init_callback: Default::default(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn type_(&self) -> Type {
        Type::CallbackInterface(self.name.clone())
    }

    pub fn methods(&self) -> Vec<&Method> {
        self.methods.iter().collect()
    }

    pub fn ffi_init_callback(&self) -> &FFIFunction {
        &self.ffi_init_callback
    }

    pub(super) fn derive_ffi_funcs(&mut self, ci_prefix: &str) {
        self.ffi_init_callback.name = format!("ffi_{}_{}_init_callback", ci_prefix, self.name);
        self.ffi_init_callback.arguments = vec![FFIArgument {
            name: "callback_stub".to_string(),
            type_: FFIType::ForeignCallback,
        }];
        self.ffi_init_callback.return_type = None;
    }

    pub fn iter_types(&self) -> TypeIterator<'_> {
        Box::new(self.methods.iter().flat_map(Method::iter_types))
    }
}

impl Hash for CallbackInterface {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // We don't include the FFIFunc in the hash calculation, because:
        //  - it is entirely determined by the other fields,
        //    so excluding it is safe.
        //  - its `name` property includes a checksum derived from  the very
        //    hash value we're trying to calculate here, so excluding it
        //    avoids a weird circular depenendency in the calculation.
        self.name.hash(state);
        self.methods.hash(state);
    }
}
