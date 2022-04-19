/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! #  Helpers for resolving UDL type expressions into concrete types.
//!
//! This module provides the [`TypeResolver`] trait, an abstraction for walking
//! the parse tree of a weedle type expression and using a [`TypeUniverse`] to
//! convert it into a concrete type definition (so it assumes that you're already
//! used a [`TypeFinder`](super::TypeFinder) to populate the universe).
//!
//! Perhaps most importantly, it knows how to error out if the UDL tries to reference
//! an undefined or invalid type.

use anyhow::Result;

use super::{Type, TypeUniverse};

/// Trait to help resolving an UDL type node to a [`Type`].
///
/// Ths trait does structural matching against type-related weedle AST nodes from
/// a parsed UDL file, turning them into a corresponding [`Type`] struct. It uses the
/// known type definitions in a [`TypeUniverse`] to resolve names to types.
///
/// As a side-effect, resolving a type expression will grow the type universe with
/// references to the types seem during traversal. For example resolving the type
/// expression "sequence<TestRecord>?" will:
///
///   * add `Optional<Sequence<TestRecord>` and `Sequence<TestRecord>` to the
///     known types in the universe.
///   * error out if the type name `TestRecord` is not already known.
///
pub(crate) trait TypeResolver {
    fn resolve_type_expression(&self, types: &mut TypeUniverse) -> Result<Type>;
}
