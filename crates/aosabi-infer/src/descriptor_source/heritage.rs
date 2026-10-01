// SPDX-License-Identifier: MIT
/*
 * aosabi-infer: the heritage of a descriptor graph, the list the heritage
 * sweep of the inference reads.
 *
 * No file of the reference is translated here.
 */

//! The heritage declarations of a descriptor graph.
//!
//! The reference scans every source file of the program for a class or an
//! interface with a heritage clause. A Rust crate is a module graph and no
//! directory scan, and the order of the declaration list is the order of its
//! scan and no source file order, so the list here is every declaration of
//! the application crate that has heritage, in `SourceKey` order, with its
//! bases in attribute order. The graph keeps its declarations in that order
//! already.
//!
//! # The type of an `extends` base
//!
//! The checker types the expression of a heritage clause of a class as the
//! named type instantiated with the `this` type of the declaring class,
//! whenever the named type is a generic reference, which every class is and
//! which an interface that has no type parameter and never says `this` is
//! not.
//!
//! The source therefore presents the base of an `extends` entry as a type of
//! its own, with an identity apart from the declared class, with the symbol
//! of that class and with no base types. The reason is the inheritance map of
//! the reference, which files a base under the first key that has its symbol
//! and under its own identity where no key has, so whether a base arrives as
//! the declared class or as a type beside it decides which key holds the
//! derived classes and what an interface made from that key implements.
//!
//! The descriptor records the two relations of a twin and not whether the
//! base of the TypeScript twin is a class or an interface. An `extends` entry
//! is therefore presented as the twin of a class that extends a class, with
//! the instantiated base, and an `implements` entry as the twin of a class
//! that implements an interface, with the declared base, which are the two
//! forms the reference tests write. An interface that extends an interface
//! with no type parameter, as at `:91` of that file, is written as an
//! `implements` entry as well, because the base of its clause is the declared
//! interface by the rule of the section above, and the base types that
//! present.rs answers tell the class from the interface.

use super::{
    DescriptorSource,
    DescriptorType,
    present::TypeData, //
};
use crate::type_source::HeritageDeclaration;

impl DescriptorSource {
    /// Returns one entry per declaration that carries a link and heritage.
    ///
    /// A generic declaration carries no link, because a generic type is no
    /// type before it is instantiated, and it contributes nothing.
    pub(super) fn heritage_declarations(&self) -> Vec<HeritageDeclaration<DescriptorType>> {
        self.graph
            .declarations
            .iter()
            .filter(|declaration| !declaration.heritage.is_empty())
            .filter_map(|declaration| {
                let derived = self.type_of(TypeData::Node(declaration.link?));
                let bases = declaration
                    .heritage
                    .iter()
                    .map(|entry| {
                        self.type_of(if entry.is_extends {
                            TypeData::ExtendsReference(entry.base)
                        } else {
                            TypeData::Node(entry.base)
                        })
                    })
                    .collect();

                Some(HeritageDeclaration { derived, bases })
            })
            .collect()
    }
}
