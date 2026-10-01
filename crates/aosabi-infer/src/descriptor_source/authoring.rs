// SPDX-License-Identifier: MIT
/*
 * aosabi-infer: the authoring map of a build, which ties every node of the
 * intermediate representation to the Rust item that declares it.
 *
 * The walk mirrors the class AuthoringMap of the TypeScript sidecar, which
 * ties the same nodes to the TypeScript declarations of the twin, so the two
 * maps of one pair name the same nodes.
 */

//! The authoring map of a build.
//!
//! [`DescriptorSource::authoring_map`] walks the intermediate representation
//! of a build and the descriptor graph side by side, the way the operation
//! `authoring_map` of the sidecar walks the model and the checker: from each
//! root it follows every operation to the member that declares it, every
//! argument to its parameter and every reference to the node the source gives
//! the same place, then it finds every named type no reference reached by the
//! name of a declaration. Each node of the answer carries the kind of its
//! Rust item, the site the macros recorded and the heritage of a type, which
//! check C2 and check C4 of `tests/notebooks/12_authoring_parity.ipynb`
//! compare with the TypeScript side.
//!
//! | Kind | Item |
//! | --- | --- |
//! | `struct`, `anonymous_struct` | a derived struct, with and without `#[aosabi(anonymous)]` |
//! | `unit_enum`, `ts_enum`, `payload_enum` | a derived enum, a literal union, a TypeScript `enum` and an object union |
//! | `marker`, `custom_scalar` | a scalar of the facade and one of `#[aosabi::scalar]` |
//! | `root_struct`, `class_root`, `fragment_root` | a root, the second the root whose one field is `prototype`, the third a root of `graphql!` or of `compose` |
//! | `base_struct` | the base struct an interface `I<Base>` is made of |
//! | `field`, `method`, `async_method` | a member |
//! | `spread` | a member a field under `#[aosabi(spread)]` brings, at the line of that field |
//! | `lazy` | a field whose type is a callable, a `Lazy` member among them |
//! | `entry_inline`, `entry_reference`, `entry_shorthand` | an entry of `graphql!`, a closure, a path to a static or a function, and a shorthand |
//! | `private_field_method` | a method over the private field of its name, a property of an interleaved class |
//! | `parameter`, `variant` | a parameter and a variant |
//!
//! A member a spread brings stands at the file and the line of the spread
//! field, the construct check C4 pairs with the spread of the TypeScript
//! literal, and carries the comment of the member itself, which is the
//! description the schema prints for it, because a comment on a spread field
//! reaches no API. A parameter of a callable field is a `parameter` like
//! that of a method.
//!
//! An entry carries `mutation` when it stands under `mutation(...)`, and an
//! entry or a method with decorators carries `decorators`, each with its
//! text and its line, the facts check C3 and check C4 read. Every node
//! carries `guards`, the guard facts of the leading run of the body an entry
//! or a method reaches, which check C3 reads beside the decorators, and the
//! list is empty for a node that has no body and for a body that opens with
//! no guard.

use aosabi_ir::{
    Field as IrField,
    RootName,
    SchemaIr,
    TypeRef, //
};
use aosabi_json::{
    JsObject,
    JsValue,
    map::{
        IndexMap,
        IndexSet, //
    },
    value::KeyHasherBuilder, //
};

use super::{
    DescriptorSource,
    DescriptorSymbol,
    DescriptorType,
    graph::{
        Expr,
        NamedKind,
        NamedNode,
        SiteNode, //
    },
    present::{
        SymbolData,
        TypeData, //
    }, //
};
use crate::type_source::TypeSource;

/// The crate whose scalars are the markers of the facade.
const MARKER_CRATE: &str = "aosabi-core";

/// One base type a node names, as `authoring.json` writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringHeritage {
    /// `extends` or `implements`.
    pub kind: &'static str,
    /// The declared name of the base type.
    pub base: String,
}

/// One check inside the closure of an `authorize` guard, as
/// `authoring.json` writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringGuardCall {
    /// `has_role` or `has_permission`.
    pub name: String,
    /// The string literal arguments of the check, in source order.
    pub args: Vec<String>,
}

/// One guard of the leading run of a body, as `authoring.json` writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringGuard {
    /// `require_role` or `authorize`.
    pub kind: &'static str,
    /// The roles of a `require_role` whose argument is a literal slice of
    /// string literals, `None` for any other argument and for an
    /// `authorize`.
    pub args: Option<Vec<String>>,
    /// The `has_role` and `has_permission` calls inside the closure of an
    /// `authorize`, in source order.
    pub calls: Vec<AuthoringGuardCall>,
    /// The count of the other calls and the comparisons of the closure.
    pub others: u32,
    /// The source line of the call.
    pub line: u32,
}

impl AuthoringGuard {
    /// Returns the guard as `authoring.json` and the dump of the descriptor
    /// graph write it, `{"kind","args","calls":[{"name","args"}],"others",
    /// "line"}`, with `args` `null` where it is `None`.
    #[must_use]
    pub fn to_js_value(&self) -> JsValue {
        let strings = |values: &[String]| {
            values
                .iter()
                .map(|value| JsValue::from(value.as_str()))
                .collect::<Vec<JsValue>>()
        };
        let mut object = JsObject::new();
        object.insert("kind", self.kind);
        object.insert(
            "args",
            self.args
                .as_deref()
                .map_or(JsValue::Null, |args| JsValue::from(strings(args))),
        );
        object.insert(
            "calls",
            self.calls
                .iter()
                .map(|call| {
                    let mut value = JsObject::new();
                    value.insert("name", call.name.as_str());
                    value.insert("args", strings(&call.args));
                    JsValue::Object(value)
                })
                .collect::<Vec<JsValue>>(),
        );
        object.insert("others", self.others);
        object.insert("line", self.line);
        JsValue::Object(object)
    }
}

/// One node of the authoring map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringNode {
    /// The path of the node in the intermediate representation, `Name`,
    /// `Name.field` or `Name.field(arg)`.
    pub path: String,
    /// The kind of the Rust item, `None` when the walk finds none.
    pub kind: Option<&'static str>,
    /// The value of `file!()` at the macro that describes the item.
    pub file: Option<String>,
    /// The source line of the name of the item.
    pub line: Option<u32>,
    /// The lines of the last documentation block, `None` when the item
    /// carries no block.
    pub docs: Option<Vec<String>>,
    /// The number of documentation blocks the item carries.
    pub blocks: u32,
    /// The base types of a type, in attribute order.
    pub heritage: Vec<AuthoringHeritage>,
    /// Whether an entry of `graphql!` stands under `mutation(...)`, `None`
    /// for a node that is no entry.
    pub mutation: Option<bool>,
    /// The decorators of an entry or a method, each with its text and its
    /// line, in source order.
    pub decorators: Vec<(String, u32)>,
    /// The guards of the leading run of the body an entry or a method
    /// reaches, in source order.
    pub guards: Vec<AuthoringGuard>,
}

/// The item a node of the representation maps to.
#[derive(Debug, Clone, Copy)]
enum Item {
    /// The named node at the position.
    Named(u32),
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
    /// The parameter `index` of the method `method` of the node `owner`.
    Parameter {
        /// The node that declares the method.
        owner: u32,
        /// The position of the method.
        method: u32,
        /// The position of the parameter.
        index: u32,
    },
    /// The variant `index` of the enum node `owner`.
    Variant {
        /// The enum node.
        owner: u32,
        /// The position of the variant.
        index: u32,
    },
    /// The parameter `index` of the callable node `node`.
    CallableParameter {
        /// The callable node.
        node: u32,
        /// The position of the parameter.
        index: u32,
    },
}

/// The walk of one map.
struct Walk<'a> {
    /// The source the build inferred from.
    source: &'a DescriptorSource,
    /// The representation the build inferred.
    ir: &'a SchemaIr,
    /// The item and the kind of each path found so far, the first one found.
    sites: IndexMap<String, (&'static str, Item), KeyHasherBuilder>,
    /// The named types the walk entered.
    visited: IndexSet<String, KeyHasherBuilder>,
}

impl DescriptorSource {
    /// Returns the authoring map of `ir`, the representation this source
    /// inferred, one node per node of the representation in its order: the
    /// roots with their operations and arguments, then the objects, the
    /// interfaces and the inputs with their fields and arguments, the unions,
    /// and the enums with their values.
    #[must_use]
    pub fn authoring_map(&self, ir: &SchemaIr) -> Vec<AuthoringNode> {
        let mut walk = Walk {
            source: self,
            ir,
            sites: IndexMap::default(),
            visited: IndexSet::default(),
        };
        walk.walk();
        paths(ir)
            .into_iter()
            .map(|path| walk.describe(path))
            .collect()
    }
}

/// Returns every path of `ir` in the order of the map, each once.
fn paths(ir: &SchemaIr) -> Vec<String> {
    let mut paths: IndexSet<String, KeyHasherBuilder> = IndexSet::default();
    for root in RootName::ALL {
        let mut operations = ir
            .operations
            .iter()
            .filter(|operation| operation.root == root)
            .peekable();
        if operations.peek().is_none() {
            continue;
        }
        paths.insert(String::from(root.as_str()));
        for operation in operations {
            let path = format!("{}.{}", root.as_str(), operation.name);
            paths.insert(path.clone());
            paths.extend(
                operation
                    .args
                    .iter()
                    .map(|arg| format!("{path}({})", arg.name)),
            );
        }
    }
    let shapes = ir
        .objects
        .entries()
        .map(|(name, shape)| (name, &shape.fields))
        .chain(
            ir.interfaces
                .entries()
                .map(|(name, shape)| (name, &shape.fields)),
        )
        .chain(
            ir.inputs
                .entries()
                .map(|(name, shape)| (name, &shape.fields)),
        );
    for (name, fields) in shapes {
        paths.insert(String::from(name));
        for field in fields {
            let path = format!("{name}.{}", field.name);
            paths.insert(path.clone());
            for arg in field.args.iter().flatten() {
                paths.insert(format!("{path}({})", arg.name));
            }
        }
    }
    paths.extend(ir.unions.keys().map(String::from));
    for (name, shape) in ir.enums.entries() {
        paths.insert(String::from(name));
        paths.extend(shape.values.iter().map(|value| format!("{name}.{value}")));
    }
    paths.into_iter().collect()
}

impl Walk<'_> {
    /// Records the item of `path`, keeping the first one found.
    fn put(&mut self, path: String, kind: &'static str, item: Item) {
        self.sites.entry(path).or_insert((kind, item));
    }

    /// Returns the named node at `index`.
    fn named(&self, index: u32) -> Option<&NamedNode> {
        self.source.named(index)
    }

    /// Walks every root, then every named type no reference reached.
    fn walk(&mut self) {
        for (slot, root) in RootName::ALL.into_iter().enumerate() {
            if let Some(Some(node)) = self.source.graph.roots.get(slot) {
                self.visit_root(root, *node);
            }
        }
        let names: Vec<String> = self
            .ir
            .objects
            .keys()
            .chain(self.ir.interfaces.keys())
            .chain(self.ir.unions.keys())
            .chain(self.ir.inputs.keys())
            .chain(self.ir.enums.keys())
            .map(String::from)
            .collect();
        for name in names {
            if !self.visited.contains(&name) {
                self.visit_by_name(&name);
            }
        }
    }

    /// Returns whether the root node `node` is a class root, a struct whose
    /// one field is `prototype`.
    fn is_class_root(&self, node: u32) -> bool {
        self.named(node).is_some_and(|named| {
            named.methods.is_empty()
                && named.fields.len() == 1
                && named.fields.iter().all(|field| field.name == "prototype")
        })
    }

    /// Maps one root and each of its operations.
    fn visit_root(&mut self, root: RootName, node: u32) {
        let class_root = self.is_class_root(node);
        let fragment = self
            .named(node)
            .is_some_and(|named| named.fragment.is_some());
        let kind = if class_root {
            "class_root"
        } else if fragment {
            "fragment_root"
        } else {
            "root_struct"
        };
        self.put(String::from(root.as_str()), kind, Item::Named(node));
        let root_type = self.source.node_type(node);
        for operation in self
            .ir
            .operations
            .iter()
            .filter(|operation| operation.root == root)
        {
            let path = format!("{}.{}", root.as_str(), operation.name);
            self.visit_member(
                &path,
                root_type,
                &operation.name,
                &operation.args,
                &operation.returns,
            );
        }
    }

    /// Maps the member `name` of `owner_type` at `path`, its arguments and
    /// the type it returns.
    fn visit_member(
        &mut self,
        path: &str,
        owner_type: DescriptorType,
        name: &str,
        args: &[IrField],
        returns: &TypeRef,
    ) {
        let Some(property) = self.property(owner_type, name) else {
            return;
        };
        let Some((kind, item)) = self.member(property) else {
            return;
        };
        self.put(String::from(path), kind, item);
        if let Item::Method { owner, index } = item {
            self.visit_args(path, args, owner, index);
            let returns_type = self
                .source
                .method(owner, index)
                .map(|method| method.returns);
            if let Some(returns_type) = returns_type {
                self.visit_ref(returns, self.source.node_type(returns_type));
            }
            return;
        }
        let property_type = self.source.type_of_symbol(property);
        if let TypeData::Node(node) = self.source.type_data(property_type)
            && let Some(callable) = self.source.callable(node)
        {
            let returns_type = callable.returns;
            self.visit_callable_args(path, args, node);
            self.visit_ref(returns, self.source.node_type(returns_type));
            return;
        }
        self.visit_ref(returns, property_type);
    }

    /// Maps the arguments at `path` to the parameters of the callable node
    /// `node`.
    fn visit_callable_args(&mut self, path: &str, args: &[IrField], node: u32) {
        let Some(callable) = self.source.callable(node) else {
            return;
        };
        let params: Vec<(u32, String, u32)> = callable
            .params
            .iter()
            .enumerate()
            .filter_map(|(position, param)| {
                Some((
                    u32::try_from(position).ok()?,
                    param.name.clone(),
                    param.r#type,
                ))
            })
            .collect();
        for arg in args {
            let Some((index, _, r#type)) = params.iter().find(|(_, name, _)| *name == arg.name)
            else {
                continue;
            };
            let item = Item::CallableParameter {
                node,
                index: *index,
            };
            self.put(format!("{path}({})", arg.name), "parameter", item);
            self.visit_ref(&arg.r#type, self.source.node_type(*r#type));
        }
    }

    /// Returns the property `name` of `type`.
    fn property(&self, r#type: DescriptorType, name: &str) -> Option<DescriptorSymbol> {
        self.source
            .properties_of(r#type)
            .into_iter()
            .find(|property| self.source.name_of(*property) == name)
    }

    /// Returns the kind and the item of a member symbol.
    ///
    /// A field a spread brings is a `spread`, and a field whose type is a
    /// callable is `lazy`.
    fn member(&self, symbol: DescriptorSymbol) -> Option<(&'static str, Item)> {
        match self.source.symbol_data(symbol)? {
            SymbolData::Field { owner, index } => {
                let field = self.source.field(owner, index)?;
                let kind = if field.spread.is_some() {
                    "spread"
                } else if self.source.callable(field.r#type).is_some() {
                    "lazy"
                } else {
                    "field"
                };
                Some((kind, Item::Field { owner, index }))
            }
            SymbolData::Method { owner, index } => {
                let named = self.named(owner)?;
                let method = named.methods.get(usize::try_from(index).ok()?)?;
                let over_private_field = named
                    .fields
                    .iter()
                    .any(|field| field.is_private && field.name == method.name);
                let entry = method.entry.as_ref().map(|entry| entry.kind);
                let kind = if let Some(entry) = entry {
                    match entry {
                        "reference" => "entry_reference",
                        "shorthand" => "entry_shorthand",
                        _ => "entry_inline",
                    }
                } else if over_private_field {
                    "private_field_method"
                } else if method.is_async {
                    "async_method"
                } else {
                    "method"
                };
                Some((kind, Item::Method { owner, index }))
            }
            _ => None,
        }
    }

    /// Maps the arguments at `path` to the parameters of the method `index`
    /// of the node `owner`.
    fn visit_args(&mut self, path: &str, args: &[IrField], owner: u32, index: u32) {
        let Some(method) = self.source.method(owner, index) else {
            return;
        };
        let params: Vec<(u32, String, u32)> = method
            .params
            .iter()
            .enumerate()
            .filter_map(|(position, param)| {
                Some((
                    u32::try_from(position).ok()?,
                    param.name.clone(),
                    param.r#type,
                ))
            })
            .collect();
        for arg in args {
            let Some((position, _, r#type)) = params.iter().find(|(_, name, _)| *name == arg.name)
            else {
                continue;
            };
            let item = Item::Parameter {
                owner,
                method: index,
                index: *position,
            };
            self.put(format!("{path}({})", arg.name), "parameter", item);
            self.visit_ref(&arg.r#type, self.source.node_type(*r#type));
        }
    }

    /// Returns `type` with its promises awaited, the item of its repeater and without `undefined`.
    ///
    /// The type of a subscription field is the item of its `Repeater`, as the
    /// builder of the reference reads the first type argument of one.
    fn unwrap(&self, r#type: DescriptorType) -> DescriptorType {
        let mut current = self.source.non_nullable_of(r#type);
        while let Some(Expr::Promise(of) | Expr::Repeater(of)) = self.source.expr_of_type(current) {
            current = self.source.non_nullable_of(self.source.node_type(*of));
        }
        current
    }

    /// Follows a reference of the representation into the type the source
    /// gives the same place.
    fn visit_ref(&mut self, type_ref: &TypeRef, r#type: DescriptorType) {
        let mut reference = type_ref;
        let mut current = self.unwrap(r#type);
        while let TypeRef::List { of, .. } = reference {
            let element = self.source.index_type_of(current).unwrap_or(current);
            current = self.unwrap(element);
            reference = of;
        }
        if let TypeRef::Ref { name, .. } = reference {
            self.visit_type(name, current);
        }
    }

    /// Returns the named node behind `type`.
    fn declaration_of_type(&self, r#type: DescriptorType) -> Option<u32> {
        let declared = self.source.declared_of(r#type);
        match self.source.expr_of_type(declared)? {
            Expr::Named(_) => match self.source.type_data(declared) {
                TypeData::Node(index) => Some(index),
                _ => None,
            },
            _ => None,
        }
    }

    /// Returns the kind of the type item at the node `index`.
    fn type_kind(&self, index: u32) -> &'static str {
        let Some(named) = self.named(index) else {
            return "struct";
        };
        match named.kind {
            NamedKind::Object if named.anonymous => "anonymous_struct",
            NamedKind::Object => "struct",
            NamedKind::Enum => "unit_enum",
            NamedKind::EnumDeclaration => "ts_enum",
            NamedKind::Union => "payload_enum",
            NamedKind::Scalar if named.krate == MARKER_CRATE => "marker",
            NamedKind::Scalar => "custom_scalar",
        }
    }

    /// Maps the named type `name` of the representation, which the source
    /// gives as `type`.
    fn visit_type(&mut self, name: &str, r#type: DescriptorType) {
        if !self.visited.insert(String::from(name)) {
            return;
        }
        if self.ir.enums.contains_key(name) {
            self.visit_enum(name, r#type);
        } else if self.ir.unions.contains_key(name) {
            self.visit_union(name, r#type);
        } else if self.ir.interfaces.contains_key(name) {
            self.visit_interface(name, r#type);
        } else {
            let fields = self
                .ir
                .objects
                .get(name)
                .map(|shape| &shape.fields)
                .or_else(|| self.ir.inputs.get(name).map(|shape| &shape.fields));
            let Some(fields) = fields else {
                return;
            };
            if let Some(node) = self.declaration_of_type(r#type) {
                self.put(String::from(name), self.type_kind(node), Item::Named(node));
            }
            self.visit_fields(name, r#type, fields);
        }
    }

    /// Maps an enum and its values to the enum and its variants.
    fn visit_enum(&mut self, name: &str, r#type: DescriptorType) {
        let Some(node) = self.declaration_of_type(r#type) else {
            return;
        };
        self.put(String::from(name), self.type_kind(node), Item::Named(node));
        let Some(shape) = self.ir.enums.get(name) else {
            return;
        };
        for value in &shape.values {
            let position = self.named(node).and_then(|named| {
                named
                    .variants
                    .iter()
                    .position(|variant| variant.value == *value || variant.name == *value)
            });
            if let Some(index) = position.and_then(|position| u32::try_from(position).ok()) {
                let item = Item::Variant { owner: node, index };
                self.put(format!("{name}.{value}"), "variant", item);
            }
        }
    }

    /// Returns the member type of the union `type` whose name, whose
    /// `__typename` literal or whose fields are those of the object `member`
    /// of the representation.
    ///
    /// An anonymous member takes its name from its `__typename` literal, the
    /// member of an inferred union a patch answers.
    fn constituent(&self, r#type: DescriptorType, member: &str) -> Option<DescriptorType> {
        let types = if self.source.is_union(r#type) {
            self.source.members_of(r#type)
        } else {
            vec![r#type]
        };
        let by_name = types.iter().copied().find(|one| {
            self.source
                .alias_symbol_of_type(*one)
                .or_else(|| self.source.symbol_of_type(*one))
                .is_some_and(|symbol| self.source.name_of(symbol) == member)
        });
        if by_name.is_some() {
            return by_name;
        }
        let by_typename = types.iter().copied().find(|one| {
            self.property(*one, "__typename").is_some_and(|property| {
                let literal = self.source.type_of_symbol(property);
                self.source.get_literal_value(literal).as_deref() == Some(member)
            })
        });
        if by_typename.is_some() {
            return by_typename;
        }
        let fields: Vec<&str> = self
            .ir
            .objects
            .get(member)
            .map(|shape| {
                shape
                    .fields
                    .iter()
                    .map(|field| field.name.as_str())
                    .collect()
            })
            .unwrap_or_default();
        types.into_iter().find(|one| {
            let properties: Vec<String> = self
                .source
                .properties_of(*one)
                .into_iter()
                .map(|property| self.source.name_of(property))
                .collect();
            properties.len() == fields.len()
                && fields
                    .iter()
                    .all(|field| properties.iter().any(|name| name == field))
        })
    }

    /// Maps a union and follows it into each of its members.
    fn visit_union(&mut self, name: &str, r#type: DescriptorType) {
        if let Some(node) = self.declaration_of_type(r#type) {
            self.put(String::from(name), "payload_enum", Item::Named(node));
        }
        let members = self
            .ir
            .unions
            .get(name)
            .map(|shape| shape.members.clone())
            .unwrap_or_default();
        for member in members {
            if let Some(found) = self.constituent(r#type, &member) {
                self.visit_type(&member, found);
            }
        }
    }

    /// Maps an interface, promoted from a union or made of a base struct.
    fn visit_interface(&mut self, name: &str, r#type: DescriptorType) {
        let Some(shape) = self.ir.interfaces.get(name) else {
            return;
        };
        if self.source.is_union(r#type) {
            self.visit_promoted(name, r#type, &shape.fields);
            return;
        }
        let node = self.declaration_of_type(r#type);
        let symbol_name = self
            .source
            .alias_symbol_of_type(r#type)
            .or_else(|| self.source.symbol_of_type(r#type))
            .map(|symbol| self.source.name_of(symbol));
        let base = symbol_name
            .filter(|symbol_name| symbol_name != name && format!("I{symbol_name}") == name);
        if let Some(node) = node {
            let kind = if base.is_some() {
                "base_struct"
            } else {
                self.type_kind(node)
            };
            self.put(String::from(name), kind, Item::Named(node));
        }
        self.visit_fields(name, r#type, &shape.fields);
        if let Some(base) = base
            && self.ir.objects.contains_key(&base)
        {
            self.visit_type(&base, r#type);
        }
    }

    /// Maps a union promoted to an interface, its members and each shared
    /// field, which maps to the member of the first implementing type that
    /// declares it.
    fn visit_promoted(&mut self, name: &str, r#type: DescriptorType, fields: &[IrField]) {
        if let Some(node) = self.declaration_of_type(r#type) {
            self.put(String::from(name), "payload_enum", Item::Named(node));
        }
        let implementors: Vec<String> = self
            .ir
            .objects
            .values()
            .filter(|object| object.implements.iter().flatten().any(|one| one == name))
            .map(|object| object.name.clone())
            .collect();
        for object in &implementors {
            if let Some(found) = self.constituent(r#type, object) {
                self.visit_type(object, found);
            }
        }
        for field in fields {
            for object in &implementors {
                let member = self
                    .constituent(r#type, object)
                    .and_then(|found| self.property(found, &field.name))
                    .and_then(|property| self.member(property));
                if let Some((kind, item)) = member {
                    self.put(format!("{name}.{}", field.name), kind, item);
                    break;
                }
            }
        }
    }

    /// Maps the fields of an object, an interface or an input to members.
    fn visit_fields(&mut self, name: &str, r#type: DescriptorType, fields: &[IrField]) {
        for field in fields {
            let path = format!("{name}.{}", field.name);
            let args = field.args.as_deref().unwrap_or_default();
            self.visit_member(&path, r#type, &field.name, args, &field.r#type);
        }
    }

    /// Maps a named type no reference reached, through a declaration of its
    /// name, or of its name without the `I` of an interface.
    fn visit_by_name(&mut self, name: &str) {
        let find = |wanted: &str| {
            self.source
                .graph
                .declarations
                .iter()
                .find(|declaration| declaration.name == wanted)
                .and_then(|declaration| declaration.link)
        };
        let link = find(name).or_else(|| name.strip_prefix('I').and_then(find));
        if let Some(link) = link {
            self.visit_type(name, self.source.node_type(link));
        }
    }

    /// Returns the site of an item.
    ///
    /// A field a spread brings stands at the file and the line of the spread
    /// field and carries the documentation block of its own declaration.
    fn site(&self, item: Item) -> Option<SiteNode> {
        match item {
            Item::Named(index) => self.named(index).map(|named| named.site.clone()),
            Item::Field { owner, index } => {
                let field = self.source.field(owner, index)?;
                Some(match &field.spread {
                    Some(spread) => SiteNode {
                        file: spread.site.file.clone(),
                        line: spread.site.line,
                        docs: field.site.docs.clone(),
                        blocks: field.site.blocks,
                    },
                    None => field.site.clone(),
                })
            }
            Item::Method { owner, index } => self
                .source
                .method(owner, index)
                .map(|method| method.site.clone()),
            Item::Parameter {
                owner,
                method,
                index,
            } => self
                .source
                .method(owner, method)
                .and_then(|method| method.params.get(usize::try_from(index).ok()?))
                .map(|param| param.site.clone()),
            Item::Variant { owner, index } => self
                .named(owner)
                .and_then(|named| named.variants.get(usize::try_from(index).ok()?))
                .map(|variant| variant.site.clone()),
            Item::CallableParameter { node, index } => self
                .source
                .callable(node)
                .and_then(|callable| callable.params.get(usize::try_from(index).ok()?))
                .map(|param| param.site.clone()),
        }
    }

    /// Returns the heritage of a type item, with the declared name of each
    /// base.
    fn heritage(&self, item: Item) -> Vec<AuthoringHeritage> {
        let Item::Named(index) = item else {
            return Vec::new();
        };
        let Some(named) = self.named(index) else {
            return Vec::new();
        };
        named
            .heritage
            .iter()
            .map(|entry| AuthoringHeritage {
                kind: if entry.is_extends {
                    "extends"
                } else {
                    "implements"
                },
                base: self
                    .named(entry.base)
                    .map_or_else(String::new, |base| base.name.clone()),
            })
            .collect()
    }

    /// Returns the node of `path`, with its item or with no kind.
    fn describe(&self, path: String) -> AuthoringNode {
        let Some((kind, item)) = self.sites.get(&path).copied() else {
            return AuthoringNode {
                path,
                kind: None,
                file: None,
                line: None,
                docs: None,
                blocks: 0,
                heritage: Vec::new(),
                mutation: None,
                decorators: Vec::new(),
                guards: Vec::new(),
            };
        };
        let site = self.site(item);
        let site = site.as_ref();
        let method = match item {
            Item::Method { owner, index } => self.source.method(owner, index),
            _ => None,
        };
        AuthoringNode {
            path,
            kind: Some(kind),
            file: site.map(|site| site.file.clone()),
            line: site.map(|site| site.line),
            docs: site
                .filter(|site| site.blocks > 0)
                .map(|site| site.docs.clone()),
            blocks: site.map_or(0, |site| site.blocks),
            heritage: self.heritage(item),
            mutation: method.and_then(|method| method.entry.as_ref().map(|entry| entry.mutation)),
            decorators: method.map_or_else(Vec::new, |method| {
                method
                    .decorators
                    .iter()
                    .map(|decorator| (decorator.text.clone(), decorator.line))
                    .collect()
            }),
            guards: method.map_or_else(Vec::new, |method| method.guards.clone()),
        }
    }
}
