// SPDX-License-Identifier: MIT
/*
 * aosabi-infer: the TypeSource seam, the trait that names every question the
 * inference asks about a type, a symbol and a call signature.
 *
 * No file of the reference is translated here.
 */

//! The seam between the inference and the thing that knows the types.
//!
//! The TypeScript sources of the inference read types through the TypeScript
//! checker. The translated modules of this crate read them through
//! [`TypeSource`], so a second front end replaces one implementation and no
//! line of the inference. Every method carries the name of the checker call
//! it replaces, in snake case, and where the inference digs through
//! declaration nodes the method carries the name of the helper that does the
//! digging.
//!
//! The paths of the method docs are shortened.
//!
//! # Identity
//!
//! The reference uses a `ts.Type` and a `ts.Symbol` as `Map` keys and compares
//! them with `===`, which is object identity. Here identity is `Eq` and `Hash`
//! of [`TypeSource::Type`] and [`TypeSource::Symbol`], so an implementation
//! hands out the same value for the same type and never the same value for
//! two.
//!
//! [same-value-zero]: https://tc39.es/ecma262/#sec-samevaluezero

use core::{
    fmt::Debug,
    hash::Hash, //
};

use aosabi_ir::RootName;

/// Defines a set of flags of which only membership is ever asked.
///
/// The reference tests a flag with `flags & Flag` and never reads the number,
/// so the bit values are this crate's own and are never the numbers of the
/// TypeScript enums. Every constant is one bit.
macro_rules! flag_set {
    (
        $(#[$meta:meta])*
        $name:ident {
            $(
                $(#[$flag_meta:meta])*
                $flag:ident = $bit:expr,
            )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct $name(u32);

        impl $name {
            $(
                $(#[$flag_meta])*
                pub const $flag: Self = Self(1 << $bit);
            )+

            /// Returns the set that holds no flag.
            #[must_use]
            pub const fn empty() -> Self {
                Self(0)
            }

            /// Returns whether every flag of `other` is set in `self`.
            ///
            /// For a constant of this type, which is one bit, that is the
            /// `flags & Flag` test of the reference.
            #[must_use]
            pub const fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }

            /// Returns the set that holds the flags of `self` and of `other`.
            #[must_use]
            pub const fn union(self, other: Self) -> Self {
                Self(self.0 | other.0)
            }

            /// Returns whether the set holds no flag.
            #[must_use]
            pub const fn is_empty(self) -> bool {
                self.0 == 0
            }
        }
    };
}

flag_set! {
    /// The members of `ts.TypeFlags` the inference tests.
    TypeFlags {
        /// `TypeFlags.Any`.
        ANY = 0,
        /// `TypeFlags.String`.
        STRING = 1,
        /// `TypeFlags.Number`.
        NUMBER = 2,
        /// `TypeFlags.Boolean`.
        BOOLEAN = 3,
        /// `TypeFlags.StringLiteral`.
        STRING_LITERAL = 4,
        /// `TypeFlags.NumberLiteral`.
        NUMBER_LITERAL = 5,
        /// `TypeFlags.BooleanLiteral`.
        BOOLEAN_LITERAL = 6,
        /// `TypeFlags.Void`.
        VOID = 7,
        /// `TypeFlags.Undefined`.
        UNDEFINED = 8,
        /// `TypeFlags.Null`.
        NULL = 9,
        /// `TypeFlags.Never`.
        NEVER = 10,
        /// `TypeFlags.Object`.
        OBJECT = 11,
        /// `TypeFlags.NonPrimitive`, the `object` keyword type.
        NON_PRIMITIVE = 12,
        /// `TypeFlags.Union`.
        UNION = 13,
        /// `TypeFlags.Intersection`.
        INTERSECTION = 14,
    }
}

flag_set! {
    /// The member of `ts.ObjectFlags` the inference tests.
    ObjectFlags {
        /// `ObjectFlags.Reference`, a generic type with its type arguments.
        REFERENCE = 0,
    }
}

flag_set! {
    /// The members of `ts.SymbolFlags` the inference tests.
    SymbolFlags {
        /// `SymbolFlags.Class`.
        CLASS = 0,
        /// `SymbolFlags.Interface`.
        INTERFACE = 1,
        /// `SymbolFlags.TypeAlias`.
        TYPE_ALIAS = 2,
        /// `SymbolFlags.Enum`.
        ENUM = 3,
    }
}

flag_set! {
    /// The member of `ts.ModifierFlags` the inference tests.
    ModifierFlags {
        /// `ModifierFlags.Private`.
        PRIVATE = 0,
    }
}

/// One element of a labeled tuple that types a rest parameter.
///
/// The reference turns each element into one GraphQL argument.
#[derive(Debug, Clone)]
pub struct TupleElement<T, S> {
    /// The label of the element, `element.name.text`.
    pub name: String,
    /// The type of the element, `checker.getTypeAtLocation(element)`.
    pub r#type: T,
    /// The symbol of the element, `checker.getSymbolAtLocation(element)`.
    pub symbol: Option<S>,
    /// Whether the element carries an initializer, `element.initializer`.
    pub has_initializer: bool,
}

/// One documentation tag of a symbol, as `symbol.getJsDocTags` lists it.
#[derive(Debug, Clone)]
pub struct JsDocTag {
    /// The tag name without the `@`.
    pub name: String,
    /// The tag text, `ts.displayPartsToString(tag.text)`.
    pub text: String,
}

/// One class or interface declaration with the types its heritage clauses
/// name.
///
/// The reference treats an `extends` and an `implements` clause alike, so
/// `bases` holds the entries of both in source order.
#[derive(Debug, Clone)]
pub struct HeritageDeclaration<T> {
    /// The declared type, `checker.getTypeAtLocation(node)`.
    pub derived: T,
    /// The types the heritage clauses name, in clause order.
    pub bases: Vec<T>,
}

/// Answers every question the inference asks about the types of an
/// application.
///
/// A value of [`TypeSource::Type`] stands for one `ts.Type`, a value of
/// [`TypeSource::Symbol`] for one `ts.Symbol` and a value of
/// [`TypeSource::Signature`] for one `ts.Signature`. Each method doc names
/// the call of the reference it replaces.
pub trait TypeSource {
    /// The handle of a type, whose `Eq` and `Hash` are the identity of a
    /// `ts.Type`.
    type Type: Copy + Eq + Hash + Debug;
    /// The handle of a symbol, whose `Eq` and `Hash` are the identity of a
    /// `ts.Symbol`.
    type Symbol: Copy + Eq + Hash + Debug;
    /// The handle of a call signature.
    type Signature: Copy + Debug;

    /// Returns the type of the root operation object named `root`, or `None`
    /// when the application declares none.
    ///
    /// Replaces the entry discovery and the three `getProperty` reads of
    /// B:347-394.
    fn type_of_root(&self, root: RootName) -> Option<Self::Type>;

    /// Returns the printed form of `type`, which is `checker.typeToString`.
    fn type_to_string(&self, r#type: Self::Type) -> String;

    /// Returns the flags of `type`, which is `type.flags`.
    fn get_flags(&self, r#type: Self::Type) -> TypeFlags;

    /// Returns the object flags of `type`, which is `type.objectFlags`.
    fn get_object_flags(&self, r#type: Self::Type) -> ObjectFlags;

    /// Returns the symbol of `type`, which is `type.getSymbol()` and
    /// `type.symbol`.
    fn get_symbol(&self, r#type: Self::Type) -> Option<Self::Symbol>;

    /// Returns the alias symbol of `type`, which is `type.aliasSymbol`.
    fn get_alias_symbol(&self, r#type: Self::Type) -> Option<Self::Symbol>;

    /// Returns the name of `symbol`, which is `symbol.getName()` and
    /// `symbol.escapedName`.
    fn get_name(&self, symbol: Self::Symbol) -> String;

    /// Returns the flags of `symbol`, which is `symbol.flags`.
    fn get_symbol_flags(&self, symbol: Self::Symbol) -> SymbolFlags;

    /// Returns the properties of `type` in the order the checker lists them,
    /// which is `type.resolvedProperties` and `type.getProperties()`.
    fn get_properties(&self, r#type: Self::Type) -> Vec<Self::Symbol>;

    /// Returns the property of `type` named `name`, which is
    /// `type.getProperty(name)`.
    fn get_property(&self, r#type: Self::Type, name: &str) -> Option<Self::Symbol>;

    /// Returns the type of `symbol` as seen from the entry file, which is
    /// `checker.getTypeOfSymbolAtLocation(symbol, sfiFile)`.
    fn get_type_of_symbol_at_location(&self, symbol: Self::Symbol) -> Self::Type;

    /// Returns whether `symbol` has a value declaration, which is
    /// `symbol.valueDeclaration`.
    fn has_value_declaration(&self, symbol: Self::Symbol) -> bool;

    /// Returns the modifiers of the value declaration of `symbol`, which is
    /// `ts.getCombinedModifierFlags(symbol.valueDeclaration)`.
    fn get_combined_modifier_flags(&self, symbol: Self::Symbol) -> ModifierFlags;

    /// Returns the call signatures of `type`, which is
    /// `type.getCallSignatures()`.
    fn get_call_signatures(&self, r#type: Self::Type) -> Vec<Self::Signature>;

    /// Returns the parameters of `signature`, which is
    /// `signature.getParameters()`.
    fn get_parameters(&self, signature: Self::Signature) -> Vec<Self::Symbol>;

    /// Returns the return type of `signature`, which is
    /// `signature.getReturnType()`.
    fn get_return_type(&self, signature: Self::Signature) -> Self::Type;

    /// Returns whether `parameter` carries an initializer or a question
    /// token, which is the test of SP:1180-1182.
    fn is_optional_parameter(&self, parameter: Self::Symbol) -> bool;

    /// Returns whether `parameter` is a rest parameter, which is
    /// `valueDeclaration.dotDotDotToken` at SP:1160-1162.
    fn is_rest_parameter(&self, parameter: Self::Symbol) -> bool;

    /// Returns the labeled elements of `type` when it is a tuple type, and
    /// `None` when `checker.isTupleType(type)` is false.
    fn get_labeled_tuple_elements(
        &self,
        r#type: Self::Type,
    ) -> Option<Vec<TupleElement<Self::Type, Self::Symbol>>>;

    /// Returns whether `type` is a union type, which is `type.isUnion()`.
    fn is_union(&self, r#type: Self::Type) -> bool;

    /// Returns whether `type` is an intersection type, which is
    /// `type.isIntersection()`.
    fn is_intersection(&self, r#type: Self::Type) -> bool;

    /// Returns the members of a union or an intersection type, which is
    /// `type.types`.
    fn get_types(&self, r#type: Self::Type) -> Vec<Self::Type>;

    /// Returns `type` without `null` and `undefined`, which is
    /// `type.getNonNullableType()`.
    fn get_non_nullable_type(&self, r#type: Self::Type) -> Self::Type;

    /// Returns the union of `types`, which is the private
    /// `checker.getUnionType` of TH:180.
    fn get_union_type(&self, types: &[Self::Type]) -> Self::Type;

    /// Returns whether `source` is assignable to `target`, which is
    /// `checker.isTypeAssignableTo`.
    fn is_type_assignable_to(&self, source: Self::Type, target: Self::Type) -> bool;

    /// Returns the base types of `type`, which is `type.getBaseTypes()`, and
    /// `None` where the checker answers `undefined`.
    fn get_base_types(&self, r#type: Self::Type) -> Option<Vec<Self::Type>>;

    /// Returns the declared type a generic instantiation points at, which is
    /// `(type as TypeReference).target`.
    fn get_target(&self, r#type: Self::Type) -> Option<Self::Type>;

    /// Returns the type arguments of `type`, which is
    /// `checker.getTypeArguments` and `reference.typeArguments`, and an empty
    /// list for a type that is no reference.
    fn get_type_arguments(&self, r#type: Self::Type) -> Vec<Self::Type>;

    /// Returns the type arguments of the alias of `type`, which is
    /// `type.aliasTypeArguments`.
    fn get_alias_type_arguments(&self, r#type: Self::Type) -> Option<Vec<Self::Type>>;

    /// Returns the number index type of `type`, which is
    /// `checker.getIndexTypeOfType(type, ts.IndexKind.Number)`.
    fn get_index_type_of_type(&self, r#type: Self::Type) -> Option<Self::Type>;

    /// Returns whether the type node of `type` is an array type or a tuple
    /// type, which is the test of TH:13-19 over `checker.typeToTypeNode`.
    fn is_array_or_tuple_type_node(&self, r#type: Self::Type) -> bool;

    /// Returns the name the type node of `type` refers to, which is
    /// `typeNode.typeName?.symbol?.escapedName` at TDB:431-455.
    fn get_type_node_name(&self, r#type: Self::Type) -> Option<String>;

    /// Returns the value of a string or a number literal type as the string
    /// `value.toString()` gives, and `None` when `type.isLiteral()` is false.
    fn get_literal_value(&self, r#type: Self::Type) -> Option<String>;

    /// Returns the name the declaration walk of TDB:201-235 finds for
    /// `symbol`, which is the name of the first declaration that has one or
    /// of the first named ancestor of a declaration.
    fn get_declaration_name(&self, symbol: Self::Symbol) -> Option<String>;

    /// Returns whether `type` is declared in a default library file, which is
    /// the `isLibraryType` helper of TDB:256-270.
    fn is_library_type(&self, r#type: Self::Type) -> bool;

    /// Returns whether a declaration of `symbol` sits in a file whose name
    /// contains `needle`, which is the test of TH:229-238.
    fn is_declared_in_file(&self, symbol: Self::Symbol, needle: &str) -> bool;

    /// Returns the documentation comment of `symbol` as one string, which is
    /// `ts.displayPartsToString(symbol.getDocumentationComment(checker))`.
    fn get_documentation_comment(&self, symbol: Self::Symbol) -> String;

    /// Returns the documentation tags of `symbol`, which is
    /// `symbol.getJsDocTags(checker)`.
    fn get_js_doc_tags(&self, symbol: Self::Symbol) -> Vec<JsDocTag>;

    /// Returns every class and interface declaration of the program that has
    /// a heritage clause, in the order the sweep of SP:1270-1284 meets them.
    fn get_heritage_declarations(&self) -> Vec<HeritageDeclaration<Self::Type>>;
}
