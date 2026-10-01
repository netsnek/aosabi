// SPDX-License-Identifier: MIT
/*
 * aosabi-infer: the checker view of a descriptor graph, which says for each
 * node kind what the TypeScript checker says about the twin of that node.
 *
 * No file of the reference is translated here.
 */

//! The checker view of each node kind.
//!
//! | Descriptor | Flags | Printed | Further answers |
//! | --- | --- | --- | --- |
//! | a string, a number | `STRING`, `NUMBER` | `string`, `number` | no symbol |
//! | a boolean | `BOOLEAN` and `UNION` | `boolean` | the members are the literals `false` and `true` |
//! | void, any, never | `VOID`, `ANY`, `NEVER` | `void`, `any`, `never` | no symbol |
//! | `Nullable(T)` | `UNION` | `T \| undefined` | the members are `undefined`, then the members of `T` when it is a union, else `T` |
//! | `List(T)` | `OBJECT` | `T[]` | a reference, the symbol `Array`, the type argument and the number index type `T`, the property `length` |
//! | `Promise(T)` | `OBJECT` | `Promise<T>` | a reference, the symbol `Promise`, the type argument `T` |
//! | `Repeater(T)` | `OBJECT` | `Repeater<T>` | a reference, a symbol declared in the repeater package, the type argument `T` |
//! | a named object | `OBJECT` | the name, with its type arguments | a class symbol, the fields then the methods as properties, a `__typename` property for a typename, the `extends` heritage as base types |
//! | a named object that only `implements` entries name | `OBJECT` | the name, with its type arguments | an interface symbol, the properties of a named object, every heritage entry as a base type |
//! | a method | `OBJECT` | `(a: A) => R` | the method symbol, one call signature |
//! | a method of a root | `OBJECT` | `(a: A) => R` | the symbol `__function` of an arrow function, one call signature |
//! | a callable | `OBJECT` | `(a: A) => R` | the symbol `__function` of a function type, one call signature over its own parameters |
//! | a named enum | `UNION` | the name | the members are string literals of the values, the alias symbol carries the name |
//! | a named enum declaration | `UNION` | the name | the members of a named enum, and the symbol, an enum symbol, is the alias symbol too, so it carries the documentation |
//! | a named union | `UNION` | the name | the members are the payload types, the alias symbol carries the name |
//! | a named scalar | `OBJECT` | the scalar name | a symbol named as the scalar and one opaque property |
//! | an intersection | `INTERSECTION` | the members joined with `&` | no symbol, the members in source order, the properties of every member, each name once in the order of its first member |
//!
//! Two answers exist for a reason the table of the reference does not show.
//! The type mapping calls an object with no property and no call signature
//! `JSONObject` before it looks at a name or at a list. An array and a scalar
//! such as `Date` have properties on the TypeScript side, so a list presents
//! `length` and a scalar presents one opaque property whose name starts with
//! `__`, which the forbidden name filter removes again.
//!
//! A named object is the twin of a class, unless it is the base of an
//! `implements` entry, is the base of no `extends` entry and carries no
//! `extends` entry itself. Such an object is the twin of an interface, the
//! one an implementing class names and the one an interface that extends
//! another is written as, which is an `implements` entry. The difference
//! reaches the schema through the base types. The checker resolves the base
//! types of a class from its `extends` clause alone and those of an interface
//! from every type of its `extends` clause, and the interface the reference
//! makes of a base implements the interface of each base type of that base. A
//! class is walked for its base classes and an interface is not.
//!
//! A field whose type is a callable, a `Lazy` member among them, is the twin
//! of a property whose value is a function, which the walk follows into a
//! field with the parameters of the signature as its arguments and the
//! awaited return as its type, as it follows a method.
//!
//! A struct that carries a field named `__typename` presents that field as
//! its one `__typename` property and no literal of a typename beside it, so
//! a `pub __typename: String` field is the twin of a `__typename` widened to
//! `string`, which names no type, and a spread whose type carries a typename
//! loses it to such a field that stands after it.
//!
//! A type of a pulled registry under `#[aosabi(remote)]` describes its
//! identifier as its typename and leaves its slot `__typename: TypeName` out
//! of its fields, so it presents the `__typename` property with the string
//! literal of its `TYPENAME`, the twin of `readonly __typename: 'User'` of
//! the reference's registry. A spread of such a record brings that literal
//! into the struct of the patch, whose `get_property("__typename")` answers
//! it, and the anonymous struct of the patch takes its name from it, as the
//! reference names the patched type after the remote type.
//!
//! An anonymous object presents a symbol named `__type` without the class
//! flag and no declaration name, which is what an object literal type has,
//! so the naming falls to the property it was reached through. An anonymous
//! enum or union presents no alias symbol, so its name is joined from its
//! members.
//!
//! A root is the twin of an object literal of the `graphql` export, whose
//! function members are arrow functions, and the type of an arrow function
//! carries the symbol `__function` and not the symbol of the property it
//! stands in. A class method carries the method symbol as the symbol of its
//! type. The naming reads that symbol name and returns a scalar whose name it
//! capitalizes to, so a class method named `int` gives the scalar `Int` where
//! a root member of that name gives the scalar of its return type. The root
//! half is the case `ir-characterization-28` of the builder corpus, whose
//! members `int` and `float` print `Number`.

use aosabi_json::{
    map::IndexSet,
    value::KeyHasherBuilder, //
};

use super::{
    DescriptorSignature,
    DescriptorSource,
    DescriptorSymbol,
    DescriptorType,
    graph::{
        CallableNode,
        DocsNode,
        Expr,
        FieldNode,
        MethodNode,
        NamedKind,
        NamedNode,
        Node,
        ParamNode,
        PrimitiveKind,
        TagNode, //
    }, //
};
use crate::type_source::{
    JsDocTag,
    ObjectFlags,
    SymbolFlags,
    TypeFlags, //
};

/// The file a symbol of the default library is declared in.
const LIBRARY_FILE: &str = "typescript/lib/lib.es5.d.ts";

/// The file the repeater symbol is declared in, which carries the needle the
/// reference looks for.
const REPEATER_FILE: &str = "node_modules/@repeaterjs/repeater/lib/repeater.d.ts";

/// The nesting depth after which a printed type reads `...`, which ends the
/// printing of an anonymous object that refers to itself.
const PRINT_DEPTH: u32 = 8;

/// What a type handle stands for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum TypeData {
    /// A node of the graph.
    Node(u32),
    /// The type the checker gives the base of a class `extends` clause, which
    /// is the class of the node instantiated with the `this` type of the
    /// derived class.
    ///
    /// It is a type of its own with the symbol, the name and the properties
    /// of the class, and it answers no base types, because only the declared
    /// class does.
    ExtendsReference(u32),
    /// The `undefined` type.
    Undefined,
    /// The `never` type, which is also the union of no type.
    Never,
    /// The `number` type of the `length` of a list.
    Number,
    /// The literal `false` or `true`.
    BooleanLiteral(bool),
    /// A string literal, which is an enum value, a typename or the opaque
    /// property of a scalar.
    StringLiteral(String),
    /// The function type of the method `index` of the node `owner`.
    Method {
        /// The node that declares the method.
        owner: u32,
        /// The position of the method.
        index: u32,
    },
    /// A union the inference asked the source to build, flattened.
    Union(Vec<DescriptorType>),
}

/// What a symbol handle stands for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum SymbolData {
    /// The symbol of a named node.
    Named(u32),
    /// The `Array` symbol of a list.
    Array,
    /// The `Promise` symbol.
    Promise,
    /// The `Repeater` symbol.
    Repeater,
    /// The `length` property of a list.
    Length,
    /// The field `index` of the node `owner`.
    Field {
        /// The node that declares the field.
        owner: u32,
        /// The position of the field.
        index: u32,
    },
    /// The method `index` of the node `owner`.
    Method {
        /// The node that declares the method.
        owner: u32,
        /// The position of the method.
        index: u32,
    },
    /// The symbol `__function` of the type of the method `index` of the root
    /// node `owner`, the twin of an arrow function in an object literal.
    Arrow {
        /// The root node that declares the method.
        owner: u32,
        /// The position of the method.
        index: u32,
    },
    /// The synthetic `__typename` property of the node `owner`.
    Typename {
        /// The node that carries the typename.
        owner: u32,
    },
    /// The opaque property of the scalar node `owner`.
    Brand {
        /// The scalar node.
        owner: u32,
    },
    /// The parameter `index` of the method `method` of the node `owner`.
    Parameter {
        /// The node that declares the method.
        owner: u32,
        /// The position of the method.
        method: u32,
        /// The position of the parameter.
        index: u32,
    },
    /// The symbol `__function` of the callable node `node`, the twin of the
    /// type of a function valued property.
    Callable {
        /// The callable node.
        node: u32,
    },
    /// The parameter `index` of the callable node `node`.
    CallableParameter {
        /// The callable node.
        node: u32,
        /// The position of the parameter.
        index: u32,
    },
}

/// What a signature handle stands for, the one call signature of a method or
/// of a callable.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum SignatureData {
    /// The signature of the method `index` of the node `owner`.
    Method {
        /// The node that declares the method.
        owner: u32,
        /// The position of the method.
        index: u32,
    },
    /// The signature of the callable node at the position.
    Callable(u32),
}

/// Returns the element of `list` at the position `index`.
fn at<T>(list: &[T], index: u32) -> Option<&T> {
    list.get(usize::try_from(index).ok()?)
}

/// Returns the positions of `list` as node positions.
fn positions<T>(list: &[T]) -> impl Iterator<Item = u32> {
    (0..list.len()).filter_map(|index| u32::try_from(index).ok())
}

impl DescriptorSource {
    /// Returns the node at `index`.
    pub(super) fn node(&self, index: u32) -> Option<&Node> {
        at(&self.graph.types, index)
    }

    /// Returns the named node at `index`.
    pub(super) fn named(&self, index: u32) -> Option<&NamedNode> {
        match &self.node(index)?.expr {
            Expr::Named(named) => Some(named),
            _ => None,
        }
    }

    /// Returns what the node behind `type` is, for a type that is a node.
    pub(super) fn expr_of_type(&self, r#type: DescriptorType) -> Option<&Expr> {
        match self.type_data(r#type) {
            TypeData::Node(index) => self.node(index).map(|node| &node.expr),
            _ => None,
        }
    }

    /// Returns the method `index` of the node `owner`.
    pub(super) fn method(&self, owner: u32, index: u32) -> Option<&MethodNode> {
        at(&self.named(owner)?.methods, index)
    }

    /// Returns the field `index` of the node `owner`.
    pub(super) fn field(&self, owner: u32, index: u32) -> Option<&FieldNode> {
        at(&self.named(owner)?.fields, index)
    }

    /// Returns the callable node at `index`.
    pub(super) fn callable(&self, index: u32) -> Option<&CallableNode> {
        match &self.node(index)?.expr {
            Expr::Callable(callable) => Some(callable),
            _ => None,
        }
    }

    /// Returns the parameter `index` of the callable node `node`.
    fn callable_param(&self, node: u32, index: u32) -> Option<&ParamNode> {
        at(&self.callable(node)?.params, index)
    }

    /// Returns the handle of the node at `index`.
    pub(super) fn node_type(&self, index: u32) -> DescriptorType {
        self.type_of(TypeData::Node(index))
    }

    /// Returns the declared class behind the type of an `extends` clause, and
    /// every other type as it is, which is `target` of a type reference.
    pub(super) fn declared_of(&self, r#type: DescriptorType) -> DescriptorType {
        match self.type_data(r#type) {
            TypeData::ExtendsReference(index) => self.node_type(index),
            _ => r#type,
        }
    }

    /// Returns the flags of a node.
    fn node_flags(expr: &Expr) -> TypeFlags {
        match expr {
            Expr::Primitive(PrimitiveKind::String) => TypeFlags::STRING,
            Expr::Primitive(PrimitiveKind::Number) => TypeFlags::NUMBER,
            Expr::Primitive(PrimitiveKind::Boolean) => TypeFlags::BOOLEAN.union(TypeFlags::UNION),
            Expr::Void => TypeFlags::VOID,
            Expr::Any => TypeFlags::ANY,
            Expr::Never => TypeFlags::NEVER,
            Expr::Nullable(_) => TypeFlags::UNION,
            Expr::List(_) | Expr::Promise(_) | Expr::Repeater(_) | Expr::Callable(_) => {
                TypeFlags::OBJECT
            }
            Expr::Named(named) => match named.kind {
                NamedKind::Object | NamedKind::Scalar => TypeFlags::OBJECT,
                NamedKind::Enum | NamedKind::EnumDeclaration | NamedKind::Union => TypeFlags::UNION,
            },
            Expr::Intersection(_) => TypeFlags::INTERSECTION,
        }
    }

    /// Returns the members of an intersection in source order, and `None`
    /// for every other type.
    pub(super) fn intersection_members_of(
        &self,
        r#type: DescriptorType,
    ) -> Option<Vec<DescriptorType>> {
        match self.expr_of_type(r#type)? {
            Expr::Intersection(members) => Some(
                members
                    .iter()
                    .map(|member| self.node_type(*member))
                    .collect(),
            ),
            _ => None,
        }
    }

    /// Returns the flags of `type`.
    ///
    /// A built union of exactly `false` and `true` is the boolean type and
    /// carries its flag.
    pub(super) fn flags_of(&self, r#type: DescriptorType) -> TypeFlags {
        match self.type_data(r#type) {
            TypeData::Node(index) => self
                .node(index)
                .map_or(TypeFlags::NEVER, |node| Self::node_flags(&node.expr)),
            TypeData::ExtendsReference(_) | TypeData::Method { .. } => TypeFlags::OBJECT,
            TypeData::Undefined => TypeFlags::UNDEFINED,
            TypeData::Never => TypeFlags::NEVER,
            TypeData::Number => TypeFlags::NUMBER,
            TypeData::BooleanLiteral(_) => TypeFlags::BOOLEAN_LITERAL,
            TypeData::StringLiteral(_) => TypeFlags::STRING_LITERAL,
            TypeData::Union(members) => {
                let is_boolean = members.len() == 2
                    && members.iter().all(|member| {
                        matches!(self.type_data(*member), TypeData::BooleanLiteral(_))
                    });

                if is_boolean {
                    TypeFlags::BOOLEAN.union(TypeFlags::UNION)
                } else {
                    TypeFlags::UNION
                }
            }
        }
    }

    /// Returns the object flags of `type`, which say whether it is a generic
    /// reference.
    pub(super) fn object_flags_of(&self, r#type: DescriptorType) -> ObjectFlags {
        if matches!(self.type_data(r#type), TypeData::ExtendsReference(_)) {
            return ObjectFlags::REFERENCE;
        }

        let is_reference = match self.expr_of_type(r#type) {
            Some(Expr::List(_) | Expr::Promise(_) | Expr::Repeater(_)) => true,
            Some(Expr::Named(named)) => {
                named.kind == NamedKind::Object && !named.type_arguments.is_empty()
            }
            _ => false,
        };

        if is_reference {
            ObjectFlags::REFERENCE
        } else {
            ObjectFlags::empty()
        }
    }

    /// Returns whether `type` prints in parentheses as a list element, as a
    /// member of an intersection or, with `as_union_member`, as a member of a
    /// union, which holds for a function type in all three places, and for a
    /// union with no name and an intersection outside a union.
    fn needs_parentheses(&self, r#type: DescriptorType, as_union_member: bool) -> bool {
        if matches!(self.type_data(r#type), TypeData::Method { .. })
            || matches!(self.expr_of_type(r#type), Some(Expr::Callable(_)))
        {
            return true;
        }

        let flags = self.flags_of(r#type);

        let is_unnamed_union = flags.contains(TypeFlags::UNION)
            && !flags.contains(TypeFlags::BOOLEAN)
            && self.alias_symbol_of_type(r#type).is_none();

        !as_union_member && (is_unnamed_union || flags.contains(TypeFlags::INTERSECTION))
    }

    /// Prints `type` inside a list, an intersection or a union.
    fn print_inner(&self, r#type: DescriptorType, as_union_member: bool, depth: u32) -> String {
        let printed = self.print_type(r#type, depth);

        if self.needs_parentheses(r#type, as_union_member) {
            return format!("({printed})");
        }

        printed
    }

    /// Prints `type` the way `checker.typeToString` prints its twin.
    pub(super) fn print_type(&self, r#type: DescriptorType, depth: u32) -> String {
        if depth > PRINT_DEPTH {
            return String::from("...");
        }

        match self.type_data(r#type) {
            TypeData::Node(index) | TypeData::ExtendsReference(index) => {
                self.print_node(index, depth)
            }
            TypeData::Undefined => String::from("undefined"),
            TypeData::Never => String::from("never"),
            TypeData::Number => String::from("number"),
            TypeData::BooleanLiteral(value) => String::from(if value { "true" } else { "false" }),
            TypeData::StringLiteral(value) => format!("\"{value}\""),
            TypeData::Method { owner, index } => self.print_method(owner, index, depth),
            TypeData::Union(members) => self.print_union(&members, depth),
        }
    }

    /// Prints the members of a union joined with a bar.
    fn print_union(&self, members: &[DescriptorType], depth: u32) -> String {
        members
            .iter()
            .map(|member| self.print_inner(*member, true, depth + 1))
            .collect::<Vec<String>>()
            .join(" | ")
    }

    /// Prints the node at `index`.
    fn print_node(&self, index: u32, depth: u32) -> String {
        let Some(node) = self.node(index) else {
            return String::from("never");
        };

        match &node.expr {
            Expr::Primitive(PrimitiveKind::String) => String::from("string"),
            Expr::Primitive(PrimitiveKind::Number) => String::from("number"),
            Expr::Primitive(PrimitiveKind::Boolean) => String::from("boolean"),
            Expr::Void => String::from("void"),
            Expr::Any => String::from("any"),
            Expr::Never => String::from("never"),
            Expr::Nullable(of) => {
                let inner = self.print_inner(self.node_type(*of), true, depth + 1);
                format!("{inner} | undefined")
            }
            Expr::List(of) => {
                let inner = self.print_inner(self.node_type(*of), false, depth + 1);
                format!("{inner}[]")
            }
            Expr::Promise(of) => {
                format!(
                    "Promise<{}>",
                    self.print_type(self.node_type(*of), depth + 1)
                )
            }
            Expr::Repeater(of) => {
                format!(
                    "Repeater<{}>",
                    self.print_type(self.node_type(*of), depth + 1)
                )
            }
            Expr::Named(named) => self.print_named(index, named, depth),
            Expr::Callable(callable) => {
                let params = self.print_params(&callable.params, depth);
                let returns = self.print_type(self.node_type(callable.returns), depth + 1);
                format!("({params}) => {returns}")
            }
            Expr::Intersection(members) => members
                .iter()
                .map(|member| self.print_inner(self.node_type(*member), false, depth + 1))
                .collect::<Vec<String>>()
                .join(" & "),
        }
    }

    /// Prints a named node, which is its name with its type arguments, and
    /// its shape when it is anonymous.
    fn print_named(&self, index: u32, named: &NamedNode, depth: u32) -> String {
        if named.anonymous {
            return match named.kind {
                NamedKind::Object => self.print_object_literal(index, named, depth),
                NamedKind::Enum | NamedKind::EnumDeclaration | NamedKind::Union => {
                    self.print_union(&self.members_of(self.node_type(index)), depth)
                }
                NamedKind::Scalar => named.name.clone(),
            };
        }

        if named.type_arguments.is_empty() {
            return named.name.clone();
        }

        let arguments = named
            .type_arguments
            .iter()
            .map(|argument| self.print_type(self.node_type(*argument), depth + 1))
            .collect::<Vec<String>>()
            .join(", ");

        format!("{}<{arguments}>", named.name)
    }

    /// Prints an anonymous object as an object literal type.
    fn print_object_literal(&self, index: u32, named: &NamedNode, depth: u32) -> String {
        let mut members = Vec::new();

        for field in &named.fields {
            let printed = self.print_type(self.node_type(field.r#type), depth + 1);
            members.push(format!("{}: {printed};", field.name));
        }

        for (method, position) in named.methods.iter().zip(positions(&named.methods)) {
            let signature = self.print_signature(index, position, "): ", depth);
            members.push(format!("{}{signature};", method.name));
        }

        if members.is_empty() {
            return String::from("{}");
        }

        format!("{{ {} }}", members.join(" "))
    }

    /// Prints the parameters and the return type of a method, with
    /// `separator` between the two.
    fn print_signature(&self, owner: u32, index: u32, separator: &str, depth: u32) -> String {
        let Some(method) = self.method(owner, index) else {
            return format!("({separator}never");
        };

        let params = self.print_params(&method.params, depth);
        let returns = self.print_type(self.node_type(method.returns), depth + 1);

        format!("({params}{separator}{returns}")
    }

    /// Prints a parameter list without its parentheses.
    fn print_params(&self, params: &[ParamNode], depth: u32) -> String {
        params
            .iter()
            .map(|param| {
                let mark = if param.optional { "?" } else { "" };
                let printed = self.print_type(self.node_type(param.r#type), depth + 1);
                format!("{}{mark}: {printed}", param.name)
            })
            .collect::<Vec<String>>()
            .join(", ")
    }

    /// Prints the function type of a method.
    fn print_method(&self, owner: u32, index: u32, depth: u32) -> String {
        self.print_signature(owner, index, ") => ", depth)
    }

    /// Returns the symbol of `type`.
    pub(super) fn symbol_of_type(&self, r#type: DescriptorType) -> Option<DescriptorSymbol> {
        let data = match self.type_data(r#type) {
            TypeData::Node(index) | TypeData::ExtendsReference(index) => {
                match &self.node(index)?.expr {
                    Expr::List(_) => SymbolData::Array,
                    Expr::Promise(_) => SymbolData::Promise,
                    Expr::Repeater(_) => SymbolData::Repeater,
                    Expr::Named(named)
                        if matches!(
                            named.kind,
                            NamedKind::Object | NamedKind::Scalar | NamedKind::EnumDeclaration
                        ) =>
                    {
                        SymbolData::Named(index)
                    }
                    Expr::Callable(_) => SymbolData::Callable { node: index },
                    _ => return None,
                }
            }
            TypeData::Method { owner, index } if self.is_root(owner) => {
                SymbolData::Arrow { owner, index }
            }
            TypeData::Method { owner, index } => SymbolData::Method { owner, index },
            _ => return None,
        };

        Some(self.symbol_of(data))
    }

    /// Returns whether the node `index` is a root of the application.
    fn is_root(&self, index: u32) -> bool {
        self.graph.roots.contains(&Some(index))
    }

    /// Returns the alias symbol of `type`, which a named enum and a named
    /// union carry unless they are anonymous.
    pub(super) fn alias_symbol_of_type(&self, r#type: DescriptorType) -> Option<DescriptorSymbol> {
        let TypeData::Node(index) = self.type_data(r#type) else {
            return None;
        };
        let named = self.named(index)?;

        (matches!(
            named.kind,
            NamedKind::Enum | NamedKind::EnumDeclaration | NamedKind::Union
        ) && !named.anonymous)
            .then(|| self.symbol_of(SymbolData::Named(index)))
    }

    /// Returns the name of `symbol`.
    pub(super) fn name_of(&self, symbol: DescriptorSymbol) -> String {
        let name = match self.symbol_data(symbol) {
            Some(SymbolData::Named(index)) => self.named(index).map(|named| {
                if named.kind == NamedKind::Object && named.anonymous {
                    "__type"
                } else {
                    named.name.as_str()
                }
            }),
            Some(SymbolData::Array) => Some("Array"),
            Some(SymbolData::Promise) => Some("Promise"),
            Some(SymbolData::Repeater) => Some("Repeater"),
            Some(SymbolData::Length) => Some("length"),
            Some(SymbolData::Field { owner, index }) => {
                self.field(owner, index).map(|field| field.name.as_str())
            }
            Some(SymbolData::Method { owner, index }) => {
                self.method(owner, index).map(|method| method.name.as_str())
            }
            Some(SymbolData::Arrow { .. } | SymbolData::Callable { .. }) => Some("__function"),
            Some(SymbolData::Typename { .. }) => Some("__typename"),
            Some(SymbolData::Brand { .. }) => Some("__scalar"),
            Some(SymbolData::Parameter {
                owner,
                method,
                index,
            }) => self
                .method(owner, method)
                .and_then(|method| at(&method.params, index))
                .map(|param| param.name.as_str()),
            Some(SymbolData::CallableParameter { node, index }) => self
                .callable_param(node, index)
                .map(|param| param.name.as_str()),
            None => None,
        };

        String::from(name.unwrap_or_default())
    }

    /// Returns the flags of `symbol`.
    pub(super) fn symbol_flags_of(&self, symbol: DescriptorSymbol) -> SymbolFlags {
        match self.symbol_data(symbol) {
            Some(SymbolData::Named(index)) => match self.named(index) {
                Some(named) if named.kind == NamedKind::Object && !named.anonymous => {
                    if self.is_interface(index, named) {
                        SymbolFlags::INTERFACE
                    } else {
                        SymbolFlags::CLASS
                    }
                }
                Some(named) if matches!(named.kind, NamedKind::Enum | NamedKind::Union) => {
                    SymbolFlags::TYPE_ALIAS
                }
                Some(named) if named.kind == NamedKind::EnumDeclaration => SymbolFlags::ENUM,
                Some(named) if named.kind == NamedKind::Scalar => SymbolFlags::INTERFACE,
                _ => SymbolFlags::empty(),
            },
            Some(SymbolData::Array | SymbolData::Promise) => SymbolFlags::INTERFACE,
            Some(SymbolData::Repeater) => SymbolFlags::CLASS,
            _ => SymbolFlags::empty(),
        }
    }

    /// Returns the properties of `type`, which are the fields, then the
    /// methods, then the typename of a named object that carries no field
    /// named `__typename`, and those of every member of an intersection.
    pub(super) fn properties_of(&self, r#type: DescriptorType) -> Vec<DescriptorSymbol> {
        let (TypeData::Node(owner) | TypeData::ExtendsReference(owner)) = self.type_data(r#type)
        else {
            return Vec::new();
        };

        if self.intersection_members_of(r#type).is_some() {
            let mut symbols = Vec::new();
            self.intersection_properties(r#type, &mut Vec::new(), &mut symbols);
            return symbols;
        }

        let data: Vec<SymbolData> = match self.node(owner).map(|node| &node.expr) {
            Some(Expr::List(_)) => vec![SymbolData::Length],
            Some(Expr::Named(named)) if named.kind == NamedKind::Scalar => {
                vec![SymbolData::Brand { owner }]
            }
            Some(Expr::Named(named)) if named.kind == NamedKind::Object => positions(&named.fields)
                .map(|index| SymbolData::Field { owner, index })
                .chain(positions(&named.methods).map(|index| SymbolData::Method { owner, index }))
                .chain(
                    named
                        .typename
                        .iter()
                        .filter(|_| !named.fields.iter().any(|field| field.name == "__typename"))
                        .map(|_| SymbolData::Typename { owner }),
                )
                .collect(),
            _ => Vec::new(),
        };

        let mut symbols: Vec<DescriptorSymbol> =
            data.into_iter().map(|data| self.symbol_of(data)).collect();
        self.inherit_embedded(owner, &mut symbols);
        symbols
    }

    /// Appends to `symbols` the properties of every member of the
    /// intersection `type` in source order, each whose name `symbols` lacks,
    /// so every name stands once in the order of the first member that has
    /// it, as the checker lists the properties of an intersection.
    ///
    /// A name two members hold keeps the property of the first member, where
    /// the checker makes one property of the intersection of their types,
    /// which a payload meets only through a field `userErrors` of the struct
    /// it answers. A member that is an intersection lends its own members in
    /// its place, and `visiting` holds the intersections on the way, which
    /// ends one that holds itself.
    fn intersection_properties(
        &self,
        r#type: DescriptorType,
        visiting: &mut Vec<DescriptorType>,
        symbols: &mut Vec<DescriptorSymbol>,
    ) {
        let Some(members) = self.intersection_members_of(r#type) else {
            return;
        };
        if visiting.contains(&r#type) {
            return;
        }
        visiting.push(r#type);
        for member in members {
            if self.intersection_members_of(member).is_some() {
                self.intersection_properties(member, visiting, symbols);
                continue;
            }
            for symbol in self.properties_of(member) {
                let name = self.name_of(symbol);
                if !symbols.iter().any(|known| self.name_of(*known) == name) {
                    symbols.push(symbol);
                }
            }
        }
        visiting.pop();
    }

    /// Appends to `symbols` the members of every embedded base of the node
    /// `owner`, nearest base first, each whose name `symbols` lacks.
    ///
    /// The checker lists the members of a derived class first and then the
    /// members it inherits that it does not redeclare, and an inherited
    /// member is the symbol of the base. A base embedded in a base follows
    /// through that base's own list.
    fn inherit_embedded(&self, owner: u32, symbols: &mut Vec<DescriptorSymbol>) {
        let Some(owner_node) = self.named(owner) else {
            return;
        };
        let bases: Vec<u32> = owner_node
            .heritage
            .iter()
            .filter(|entry| entry.embedded.is_some())
            .map(|entry| entry.base)
            .collect();
        if bases.is_empty() {
            return;
        }
        let mut seen: Vec<String> = symbols.iter().map(|symbol| self.name_of(*symbol)).collect();
        for base in bases {
            for symbol in self.properties_of(self.node_type(base)) {
                let name = self.name_of(symbol);
                if !seen.contains(&name) {
                    seen.push(name);
                    symbols.push(symbol);
                }
            }
        }
    }

    /// Returns the type of `symbol`.
    pub(super) fn type_of_symbol(&self, symbol: DescriptorSymbol) -> DescriptorType {
        let data = match self.symbol_data(symbol) {
            Some(SymbolData::Named(index)) => Some(TypeData::Node(index)),
            Some(SymbolData::Length) => Some(TypeData::Number),
            Some(SymbolData::Field { owner, index }) => self
                .field(owner, index)
                .map(|field| TypeData::Node(field.r#type)),
            Some(SymbolData::Method { owner, index } | SymbolData::Arrow { owner, index }) => {
                Some(TypeData::Method { owner, index })
            }
            Some(SymbolData::Callable { node }) => Some(TypeData::Node(node)),
            Some(SymbolData::CallableParameter { node, index }) => self
                .callable_param(node, index)
                .map(|param| TypeData::Node(param.r#type)),
            Some(SymbolData::Typename { owner }) => self
                .named(owner)
                .and_then(|named| named.typename.clone())
                .map(TypeData::StringLiteral),
            Some(SymbolData::Brand { owner }) => self
                .named(owner)
                .map(|named| TypeData::StringLiteral(named.name.clone())),
            Some(SymbolData::Parameter {
                owner,
                method,
                index,
            }) => self
                .method(owner, method)
                .and_then(|method| at(&method.params, index))
                .map(|param| TypeData::Node(param.r#type)),
            _ => None,
        };

        self.type_of(data.unwrap_or(TypeData::Never))
    }

    /// Returns whether `symbol` is a private field or a private method.
    pub(super) fn is_private(&self, symbol: DescriptorSymbol) -> bool {
        match self.symbol_data(symbol) {
            Some(SymbolData::Field { owner, index }) => self
                .field(owner, index)
                .is_some_and(|field| field.is_private),
            Some(SymbolData::Method { owner, index }) => self
                .method(owner, index)
                .is_some_and(|method| method.is_private),
            _ => false,
        }
    }

    /// Returns the parameters of `signature`.
    pub(super) fn parameters_of(&self, signature: DescriptorSignature) -> Vec<DescriptorSymbol> {
        match self.signatures.get(signature.0) {
            Some(SignatureData::Method { owner, index }) => {
                self.method(owner, index).map_or_else(Vec::new, |method| {
                    positions(&method.params)
                        .map(|param| {
                            self.symbol_of(SymbolData::Parameter {
                                owner,
                                method: index,
                                index: param,
                            })
                        })
                        .collect()
                })
            }
            Some(SignatureData::Callable(node)) => {
                self.callable(node).map_or_else(Vec::new, |callable| {
                    positions(&callable.params)
                        .map(|index| self.symbol_of(SymbolData::CallableParameter { node, index }))
                        .collect()
                })
            }
            None => Vec::new(),
        }
    }

    /// Returns the return type of `signature`.
    pub(super) fn return_type_of(&self, signature: DescriptorSignature) -> DescriptorType {
        let returns = match self.signatures.get(signature.0) {
            Some(SignatureData::Method { owner, index }) => {
                self.method(owner, index).map(|method| method.returns)
            }
            Some(SignatureData::Callable(node)) => {
                self.callable(node).map(|callable| callable.returns)
            }
            None => None,
        };

        self.type_of(returns.map_or(TypeData::Never, TypeData::Node))
    }

    /// Returns the call signatures of `type`, the one of a method and the
    /// one of a callable node.
    pub(super) fn call_signatures_of(&self, r#type: DescriptorType) -> Vec<DescriptorSignature> {
        let data = match self.type_data(r#type) {
            TypeData::Method { owner, index } => SignatureData::Method { owner, index },
            TypeData::Node(index) if self.callable(index).is_some() => {
                SignatureData::Callable(index)
            }
            _ => return Vec::new(),
        };

        vec![DescriptorSignature(self.signatures.intern(data))]
    }

    /// Returns whether `symbol` is a parameter with a default.
    pub(super) fn is_optional(&self, symbol: DescriptorSymbol) -> bool {
        match self.symbol_data(symbol) {
            Some(SymbolData::Parameter {
                owner,
                method,
                index,
            }) => self
                .method(owner, method)
                .and_then(|method| at(&method.params, index))
                .is_some_and(|param| param.optional),
            Some(SymbolData::CallableParameter { node, index }) => self
                .callable_param(node, index)
                .is_some_and(|param| param.optional),
            _ => false,
        }
    }

    /// Returns the members a union names itself, before flattening.
    fn direct_members_of(&self, r#type: DescriptorType) -> Vec<DescriptorType> {
        match self.type_data(r#type) {
            TypeData::Union(members) => members,
            TypeData::Node(index) => match self.node(index).map(|node| &node.expr) {
                Some(Expr::Primitive(PrimitiveKind::Boolean)) => vec![
                    self.type_of(TypeData::BooleanLiteral(false)),
                    self.type_of(TypeData::BooleanLiteral(true)),
                ],
                Some(Expr::Nullable(of)) => {
                    vec![self.type_of(TypeData::Undefined), self.node_type(*of)]
                }
                Some(Expr::Named(named))
                    if matches!(named.kind, NamedKind::Enum | NamedKind::EnumDeclaration) =>
                {
                    named
                        .variants
                        .iter()
                        .map(|variant| self.type_of(TypeData::StringLiteral(variant.value.clone())))
                        .collect()
                }
                Some(Expr::Named(named)) if named.kind == NamedKind::Union => named
                    .members
                    .iter()
                    .map(|member| self.node_type(*member))
                    .collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// Adds `type` to `out`, and the members of `type` in its place when it
    /// is a union, which is the flattening the checker performs.
    ///
    /// `visiting` ends the recursion for a union that names itself through a
    /// smart pointer.
    fn flatten_into(
        &self,
        r#type: DescriptorType,
        out: &mut IndexSet<DescriptorType, KeyHasherBuilder>,
        visiting: &mut Vec<DescriptorType>,
    ) {
        if !self.flags_of(r#type).contains(TypeFlags::UNION) {
            out.insert(r#type);
            return;
        }

        if visiting.contains(&r#type) {
            return;
        }

        visiting.push(r#type);
        for member in self.direct_members_of(r#type) {
            self.flatten_into(member, out, visiting);
        }
        visiting.pop();
    }

    /// Returns the flattened members of `types` with `undefined` first, the
    /// place the checker gives it.
    fn flatten(&self, types: &[DescriptorType]) -> Vec<DescriptorType> {
        let mut out = IndexSet::default();
        let mut visiting = Vec::new();

        for r#type in types {
            self.flatten_into(*r#type, &mut out, &mut visiting);
        }

        let undefined = self.type_of(TypeData::Undefined);
        let mut members: Vec<DescriptorType> = Vec::new();

        if out.contains(&undefined) {
            members.push(undefined);
        }
        members.extend(out.into_iter().filter(|member| *member != undefined));

        members
    }

    /// Returns the members of a union, and an empty list for every other
    /// type.
    pub(super) fn members_of(&self, r#type: DescriptorType) -> Vec<DescriptorType> {
        if !self.flags_of(r#type).contains(TypeFlags::UNION) {
            return Vec::new();
        }

        self.flatten(&[r#type])
    }

    /// Returns `type` without `undefined`.
    ///
    /// An `Option` gives the node of its inner type, so the name of an inner
    /// enum or union survives. A type with no `undefined` member is returned
    /// as it is.
    pub(super) fn non_nullable_of(&self, r#type: DescriptorType) -> DescriptorType {
        if let Some(Expr::Nullable(of)) = self.expr_of_type(r#type) {
            return self.non_nullable_of(self.node_type(*of));
        }

        let members = self.members_of(r#type);
        let undefined = self.type_of(TypeData::Undefined);

        if !members.contains(&undefined) {
            return r#type;
        }

        let kept: Vec<DescriptorType> = members
            .into_iter()
            .filter(|member| *member != undefined)
            .collect();

        self.union_of(&kept)
    }

    /// Returns the union of `types`, which is the never type for no member
    /// and the member itself for one.
    pub(super) fn union_of(&self, types: &[DescriptorType]) -> DescriptorType {
        let mut members = self.flatten(types);

        if members.len() > 1 {
            return self.type_of(TypeData::Union(members));
        }

        members
            .pop()
            .unwrap_or_else(|| self.type_of(TypeData::Never))
    }

    /// Returns the base types of a named object, and `None` for every other
    /// type.
    ///
    /// The base types of the twin of a class are the bases of its `extends`
    /// entries, and those of the twin of an interface are the bases of all
    /// its entries, which are `implements` entries.
    pub(super) fn base_types_of(&self, r#type: DescriptorType) -> Option<Vec<DescriptorType>> {
        let TypeData::Node(index) = self.type_data(r#type) else {
            return None;
        };
        let named = self.named(index)?;
        if named.kind != NamedKind::Object {
            return None;
        }

        let is_interface = self.is_interface(index, named);
        Some(
            named
                .heritage
                .iter()
                .filter(|entry| is_interface || entry.is_extends)
                .map(|entry| self.node_type(entry.base))
                .collect(),
        )
    }

    /// Returns whether the named object `named` at `index` is the twin of an
    /// interface, which is the base of an `implements` entry, the base of no
    /// `extends` entry and carries no `extends` entry itself.
    fn is_interface(&self, index: u32, named: &NamedNode) -> bool {
        if named.heritage.iter().any(|entry| entry.is_extends) {
            return false;
        }

        let mut is_implemented = false;
        let entries = self
            .graph
            .types
            .iter()
            .filter_map(|node| match &node.expr {
                Expr::Named(other) => Some(other),
                _ => None,
            })
            .flat_map(|other| &other.heritage)
            .filter(|entry| entry.base == index);
        for entry in entries {
            if entry.is_extends {
                return false;
            }
            is_implemented = true;
        }

        is_implemented
    }

    /// Returns the type arguments of a generic reference.
    pub(super) fn type_arguments_of(&self, r#type: DescriptorType) -> Vec<DescriptorType> {
        match self.expr_of_type(r#type) {
            Some(Expr::List(of) | Expr::Promise(of) | Expr::Repeater(of)) => {
                vec![self.node_type(*of)]
            }
            Some(Expr::Named(named))
                if matches!(named.kind, NamedKind::Object | NamedKind::Scalar) =>
            {
                named
                    .type_arguments
                    .iter()
                    .map(|argument| self.node_type(*argument))
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Returns the type arguments of the alias of a named enum or union.
    pub(super) fn alias_type_arguments_of(
        &self,
        r#type: DescriptorType,
    ) -> Option<Vec<DescriptorType>> {
        self.alias_symbol_of_type(r#type)?;

        let Some(Expr::Named(named)) = self.expr_of_type(r#type) else {
            return None;
        };

        (!named.type_arguments.is_empty()).then(|| {
            named
                .type_arguments
                .iter()
                .map(|argument| self.node_type(*argument))
                .collect()
        })
    }

    /// Returns the element type of a list.
    pub(super) fn index_type_of(&self, r#type: DescriptorType) -> Option<DescriptorType> {
        match self.expr_of_type(r#type) {
            Some(Expr::List(of)) => Some(self.node_type(*of)),
            _ => None,
        }
    }

    /// Returns the name the declaration walk finds for `symbol`, which an
    /// anonymous object does not have.
    pub(super) fn declaration_name_of(&self, symbol: DescriptorSymbol) -> Option<String> {
        match self.symbol_data(symbol) {
            Some(SymbolData::Named(index))
                if self.named(index).is_some_and(|named| named.anonymous) =>
            {
                return None;
            }
            // An arrow function has no name, and the walk up its parents
            // finds the property assignment it stands in.
            Some(SymbolData::Arrow { owner, index }) => {
                return self.method(owner, index).map(|method| method.name.clone());
            }
            _ => {}
        }

        Some(self.name_of(symbol)).filter(|name| !name.is_empty())
    }

    /// Returns the file `symbol` is declared in.
    ///
    /// A callable and its parameters have no declaring node, so they name
    /// no file.
    pub(super) fn file_of(&self, symbol: DescriptorSymbol) -> String {
        let owner = match self.symbol_data(symbol) {
            Some(SymbolData::Repeater) => return String::from(REPEATER_FILE),
            Some(SymbolData::Callable { .. } | SymbolData::CallableParameter { .. }) => {
                return String::new();
            }
            Some(
                SymbolData::Named(owner)
                | SymbolData::Field { owner, .. }
                | SymbolData::Method { owner, .. }
                | SymbolData::Arrow { owner, .. }
                | SymbolData::Typename { owner }
                | SymbolData::Brand { owner }
                | SymbolData::Parameter { owner, .. },
            ) => owner,
            _ => return String::from(LIBRARY_FILE),
        };

        self.named(owner)
            .map_or_else(String::new, |named| named.source.file.clone())
    }

    /// Returns the documentation record the macros wrote for `symbol`.
    fn own_docs(&self, symbol: DescriptorSymbol) -> Option<&DocsNode> {
        match self.symbol_data(symbol)? {
            SymbolData::Named(index) => self.named(index).map(|named| &named.docs),
            SymbolData::Field { owner, index } => self.field(owner, index).map(|field| &field.docs),
            SymbolData::Method { owner, index } => {
                self.method(owner, index).map(|method| &method.docs)
            }
            SymbolData::Parameter {
                owner,
                method,
                index,
            } => self
                .method(owner, method)
                .and_then(|method| at(&method.params, index))
                .map(|param| &param.docs),
            SymbolData::CallableParameter { node, index } => {
                self.callable_param(node, index).map(|param| &param.docs)
            }
            _ => None,
        }
    }

    /// Returns the documentation comment of `symbol`,
    /// `symbol.getDocumentationComment` rendered to text.
    ///
    /// A member whose own comment is empty, or whose tags hold `@inheritDoc`,
    /// takes the comment of the member of the same name of its first base
    /// type that has one, before its own comment and a line break.
    pub(super) fn docs_of(&self, symbol: DescriptorSymbol) -> String {
        self.docs_seen(symbol, &mut Vec::new())
    }

    /// Returns the comment of `symbol`, with `seen` holding the symbols
    /// whose comment is on the way, the `seenSymbols` of the reference.
    fn docs_seen(&self, symbol: DescriptorSymbol, seen: &mut Vec<DescriptorSymbol>) -> String {
        let own = self.own_docs(symbol);
        let comment = own.map_or_else(String::new, |docs| docs.comment.clone());
        if !comment.is_empty() && !own.is_some_and(|docs| has_inherit_doc(&docs.tags)) {
            return comment;
        }
        let Some(base) = self.base_member(symbol, seen) else {
            return comment;
        };
        seen.push(base);
        let inherited = self.docs_seen(base, seen);
        if comment.is_empty() {
            return inherited;
        }
        format!("{inherited}\n{comment}")
    }

    /// Returns the tags of `symbol`, `symbol.getJsDocTags`.
    ///
    /// A member with no tag, or with `@inheritDoc`, takes the tags of the
    /// member of the same name of its first base type before its own.
    pub(super) fn tags_of(&self, symbol: DescriptorSymbol) -> Vec<JsDocTag> {
        self.tags_seen(symbol, &mut Vec::new())
    }

    /// Returns the tags of `symbol`, with `seen` as in [`Self::docs_seen`].
    fn tags_seen(
        &self,
        symbol: DescriptorSymbol,
        seen: &mut Vec<DescriptorSymbol>,
    ) -> Vec<JsDocTag> {
        let own = self.own_docs(symbol);
        let mut tags: Vec<JsDocTag> = own
            .map(|docs| {
                docs.tags
                    .iter()
                    .map(|tag| JsDocTag {
                        name: tag.name.clone(),
                        text: tag.text.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        if !tags.is_empty() && !own.is_some_and(|docs| has_inherit_doc(&docs.tags)) {
            return tags;
        }
        if let Some(base) = self.base_member(symbol, seen) {
            seen.push(base);
            let mut inherited = self.tags_seen(base, seen);
            inherited.append(&mut tags);
            tags = inherited;
        }
        tags
    }

    /// Returns the member of the same name as the member `symbol` in the
    /// first base type of its owner that has one not in `seen`,
    /// `findBaseOfDeclaration` with the `seenSymbols` test of its callback.
    ///
    /// The base types are those of the `extends` entries, then those of the
    /// `implements` entries, which is the order of `getAllSuperTypeNodes` for
    /// a class, and an interface twin carries `implements` entries alone.
    fn base_member(
        &self,
        symbol: DescriptorSymbol,
        seen: &[DescriptorSymbol],
    ) -> Option<DescriptorSymbol> {
        let Some(SymbolData::Field { owner, .. } | SymbolData::Method { owner, .. }) =
            self.symbol_data(symbol)
        else {
            return None;
        };
        let name = self.name_of(symbol);
        let heritage = &self.named(owner)?.heritage;
        let bases = heritage
            .iter()
            .filter(|entry| entry.is_extends)
            .chain(heritage.iter().filter(|entry| !entry.is_extends));
        for entry in bases {
            let member = self
                .properties_of(self.node_type(entry.base))
                .into_iter()
                .find(|property| self.name_of(*property) == name);
            if let Some(member) = member
                && !seen.contains(&member)
            {
                return Some(member);
            }
        }
        None
    }
}

/// Returns whether `tags` hold `@inheritDoc`, `hasJSDocInheritDocTag`.
fn has_inherit_doc(tags: &[TagNode]) -> bool {
    tags.iter()
        .any(|tag| tag.name == "inheritDoc" || tag.name == "inheritdoc")
}
