/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Attribute definitions for a `ComponentInterface`.
//!
//! This module provides some conveniences for working with attribute definitions
//! from WebIDL. When encountering a weedle `ExtendedAttribute` node, use `TryFrom`
//! to convert it into an [`Attribute`] representing one of the attributes that we
//! support. You can also use the [`parse_attributes`] function to parse an
//! `ExtendedAttributeList` into a vec of same.
//!
//! We only support a small number of attributes, so it's manageable to have them
//! all handled by a single abstraction. This might need to be refactored in future
//! if we grow significantly more complicated attribute handling.

use std::convert::{TryFrom, TryInto};

use anyhow::Result;

/// Represents an attribute parsed from UDL, like `[ByRef]` or `[Throws]`.
///
/// This is a convenience enum for parsing UDL attributes and erroring out if we encounter
/// any unsupported ones. These don't convert directly into parts of a `ComponentInterface`, but
/// may influence the properties of things like functions and arguments.
#[derive(Debug, Clone, Hash)]
pub(super) enum Attribute {
    ByRef,
    Enum,
    Error,
    Name(String),
    SelfType(SelfType),
    Threadsafe, // N.B. the `[Threadsafe]` attribute is deprecated and will be removed
    Throws(String),
    // `[External="crate_name"]` - We can `use crate_name::...` for the type.
    External(String),
    // Custom type on the scaffolding side
    Custom,
}

impl Attribute {
    pub fn is_error(&self) -> bool {
        matches!(self, Attribute::Error)
    }
    pub fn is_enum(&self) -> bool {
        matches!(self, Attribute::Enum)
    }
}

/// Attributes that can be attached to an `enum` definition in the UDL.
/// There's only one case here: using `[Error]` to mark an enum as an error class.
#[derive(Debug, Clone, Hash, Default)]
pub(super) struct EnumAttributes(Vec<Attribute>);

impl EnumAttributes {
    pub fn contains_error_attr(&self) -> bool {
        self.0.iter().any(|attr| attr.is_error())
    }
}

impl<T: TryInto<EnumAttributes, Error = anyhow::Error>> TryFrom<Option<T>> for EnumAttributes {
    type Error = anyhow::Error;
    fn try_from(value: Option<T>) -> Result<Self, Self::Error> {
        match value {
            None => Ok(Default::default()),
            Some(v) => v.try_into(),
        }
    }
}

/// Represents UDL attributes that might appear on a function.
///
/// This supports the `[Throws=ErrorName]` attribute for functions that
/// can produce an error.
#[derive(Debug, Clone, Hash, Default)]
pub(super) struct FunctionAttributes(pub(super) Vec<Attribute>);

impl FunctionAttributes {
    pub(super) fn get_throws_err(&self) -> Option<&str> {
        self.0.iter().find_map(|attr| match attr {
            // This will hopefully return a helpful compilation error
            // if the error is not defined.
            Attribute::Throws(inner) => Some(inner.as_ref()),
            _ => None,
        })
    }
}

impl<T: TryInto<FunctionAttributes, Error = anyhow::Error>> TryFrom<Option<T>>
    for FunctionAttributes
{
    type Error = anyhow::Error;
    fn try_from(value: Option<T>) -> Result<Self, Self::Error> {
        match value {
            None => Ok(Default::default()),
            Some(v) => v.try_into(),
        }
    }
}

/// Represents UDL attributes that might appear on a function argument.
///
/// This supports the `[ByRef]` attribute for arguments that should be passed
/// by reference in the generated Rust scaffolding.
#[derive(Debug, Clone, Hash, Default)]
pub(super) struct ArgumentAttributes(Vec<Attribute>);

impl ArgumentAttributes {
    pub fn by_ref(&self) -> bool {
        self.0.iter().any(|attr| matches!(attr, Attribute::ByRef))
    }
}

impl<T: TryInto<ArgumentAttributes, Error = anyhow::Error>> TryFrom<Option<T>>
    for ArgumentAttributes
{
    type Error = anyhow::Error;
    fn try_from(value: Option<T>) -> Result<Self, Self::Error> {
        match value {
            None => Ok(Default::default()),
            Some(v) => v.try_into(),
        }
    }
}

/// Represents UDL attributes that might appear on an `interface` definition.
#[derive(Debug, Clone, Hash, Default)]
pub(super) struct InterfaceAttributes(Vec<Attribute>);

impl InterfaceAttributes {
    pub fn contains_enum_attr(&self) -> bool {
        self.0.iter().any(|attr| attr.is_enum())
    }

    pub fn contains_error_attr(&self) -> bool {
        self.0.iter().any(|attr| attr.is_error())
    }

    pub fn threadsafe(&self) -> bool {
        self.0
            .iter()
            .any(|attr| matches!(attr, Attribute::Threadsafe))
    }
}

impl<T: TryInto<InterfaceAttributes, Error = anyhow::Error>> TryFrom<Option<T>>
    for InterfaceAttributes
{
    type Error = anyhow::Error;
    fn try_from(value: Option<T>) -> Result<Self, Self::Error> {
        match value {
            None => Ok(Default::default()),
            Some(v) => v.try_into(),
        }
    }
}

/// Represents UDL attributes that might appear on a constructor.
///
/// This supports the `[Throws=ErrorName]` attribute for constructors that can produce
/// an error, and the `[Name=MethodName]` for non-default constructors.
#[derive(Debug, Clone, Hash, Default)]
pub(super) struct ConstructorAttributes(Vec<Attribute>);

impl ConstructorAttributes {
    pub(super) fn get_throws_err(&self) -> Option<&str> {
        self.0.iter().find_map(|attr| match attr {
            // This will hopefully return a helpful compilation error
            // if the error is not defined.
            Attribute::Throws(inner) => Some(inner.as_ref()),
            _ => None,
        })
    }

    pub(super) fn get_name(&self) -> Option<&str> {
        self.0.iter().find_map(|attr| match attr {
            Attribute::Name(inner) => Some(inner.as_ref()),
            _ => None,
        })
    }
}

/// Represents UDL attributes that might appear on a method.
///
/// This supports the `[Throws=ErrorName]` attribute for methods that can produce
/// an error, and the `[Self=ByArc]` attribute for methods that take `Arc<Self>` as receiver.
#[derive(Debug, Clone, Hash, Default)]
pub(super) struct MethodAttributes(Vec<Attribute>);

impl MethodAttributes {
    pub(super) fn get_throws_err(&self) -> Option<&str> {
        self.0.iter().find_map(|attr| match attr {
            // This will hopefully return a helpful compilation error
            // if the error is not defined.
            Attribute::Throws(inner) => Some(inner.as_ref()),
            _ => None,
        })
    }

    pub(super) fn get_self_by_arc(&self) -> bool {
        self.0
            .iter()
            .any(|attr| matches!(attr, Attribute::SelfType(SelfType::ByArc)))
    }
}

impl<T: TryInto<MethodAttributes, Error = anyhow::Error>> TryFrom<Option<T>> for MethodAttributes {
    type Error = anyhow::Error;
    fn try_from(value: Option<T>) -> Result<Self, Self::Error> {
        match value {
            None => Ok(Default::default()),
            Some(v) => v.try_into(),
        }
    }
}

/// Represents the different possible types of method call receiver.
///
/// Actually we only support one of these right now, `[Self=ByArc]`.
/// We might add more in future, e.g. a `[Self=ByRef]` if there are cases
/// where we need to force the receiver to be taken by reference.
#[derive(Debug, Clone, Hash)]
pub(super) enum SelfType {
    ByArc, // Method receiver is `Arc<Self>`.
}

/// Represents UDL attributes that might appear on a typedef
///
/// This supports the `[External="crate_name"]` and `[Custom]` attributes for types.
#[derive(Debug, Clone, Hash, Default)]
pub(super) struct TypedefAttributes(Vec<Attribute>);

impl TypedefAttributes {
    pub(super) fn get_crate_name(&self) -> String {
        self.0
            .iter()
            .find_map(|attr| match attr {
                Attribute::External(crate_name) => Some(crate_name.clone()),
                _ => None,
            })
            .expect("must have a crate name")
    }

    pub(super) fn is_custom(&self) -> bool {
        self.0
            .iter()
            .any(|attr| matches!(attr, Attribute::Custom { .. }))
    }
}

impl<T: TryInto<TypedefAttributes, Error = anyhow::Error>> TryFrom<Option<T>>
    for TypedefAttributes
{
    type Error = anyhow::Error;
    fn try_from(value: Option<T>) -> Result<Self, Self::Error> {
        match value {
            None => Ok(Default::default()),
            Some(v) => v.try_into(),
        }
    }
}
