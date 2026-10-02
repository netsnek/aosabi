// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the lowering of the roots of a plain crate into the dump of a
 * descriptor graph, the types the third type source answers for.
 */

//! The lowering of a plain crate.
//!
//! The derive path describes every type of an application as a static
//! descriptor, and the descriptor graph of `aosabi-infer` presents each of
//! them as the TypeScript construct that means the same. A plain crate
//! carries no descriptor, so this module writes the dump
//! of the graph the derive path would build for the same types,
//! `descriptors.json`, from the reader's model, and the third type source
//! answers through it. Every Rust construct lowers to the node of the
//! TypeScript construct that means the same.
//!
//! | Rust | Node | TypeScript |
//! | --- | --- | --- |
//! | a struct of named fields or of none | a named object | a class, its public fields first and then the public `&self` methods of every inherent block that belongs to the use in the order of the walk |
//! | an enum of unit variants | a named enum declaration | an `enum`, whose doc comment is its description wherever it stands |
//! | an enum whose variants each hold one type | a named union | a union alias, which the union promotion of the inference prints |
//! | `Option<T>` | a nullable | `T \| null` |
//! | `Vec<T>`, `VecDeque<T>`, a slice and an array | a list | `T[]` |
//! | `Box<T>`, `Rc<T>`, `Arc<T>`, `Cow<T>` and a reference | the node of `T` | `T` |
//! | `aosabi::mirror::Unchecked<T>` under the hidden feature `mirror` | the node of `T`, as the derive path describes it | `T`, the type a cast of the reference declares |
//! | `Result<T, E>`, `aosabi::Result<T>` and `aosabi::mirror::Result<T>` | the node of `T` | a function that answers `T` or throws |
//! | the return of an `async fn` | a promise | `Promise<T>` |
//! | the return `impl Stream<Item = T>` of the facade, or a `Result` of it | a repeater | `Repeater<T>` |
//! | the return `MutationPayload<T>` of the facade, or a `Result` of it | a promise of the intersection of an anonymous object of the public fields of the struct `T`, each nullable, and the anonymous object of `userErrors` every payload shares | the payload of `mutation(...)` |
//! | `String`, `str` and `char` | a string | `string` |
//! | `bool` | a boolean | `boolean` |
//! | `i8`, `i16`, `i32`, `u8` and `u16` | the scalar `Int` | `Int` |
//! | `f32` and `f64` | the scalar `Float` | `Float` |
//! | `i64`, `u32`, `u64`, `i128`, `u128`, `isize`, `usize` and `aosabi::Number` | a number | `number` |
//! | `aosabi::json::Map` | the object `Record` with no property | `Record<string, unknown>`, which the inference prints `JSONObject` |
//! | the markers `ID`, `Int`, `Float`, `Date`, `File`, `JSON` and `Any` | the scalars of the markers and the any marker | the branded scalars |
//! | `()` | void | `void` |
//! | `!` and `Infallible` | never | `never` |
//!
//! Rust numbers print by their width on this path, while the derive path keeps
//! every number printing `Number`. The scalars `Int` and `Float` of a number
//! are the nodes the markers of the facade describe, read from their
//! descriptors in `aosabi-core`, so a field of `i32` and a field of the marker
//! `Int` print alike.
//!
//! A member takes the name the rule of [`graphql_name`] gives it, and an input
//! takes the name the inference gives its site, rule 6 of
//! `docs/schema-inference.md` section 5.1, since a struct presents as a class
//! with no alias symbol. A member without `pub` is a private property of the
//! class, as the derive path records it, so the class keeps its identity and no
//! private member reaches the schema, and its type is the any marker, since the
//! inference reads no more of it. A member of a type of a path dependency is
//! private unless it is `pub`, since the crate of the application does not see
//! it. A function with no receiver is no member, and a member whose name opens
//! with two underscores is none either. A generic type takes one node for each
//! GraphQL type its uses print, its parameters substituted, and a lifetime is
//! ignored. Two uses whose arguments print one GraphQL type, `Wrap<i32>` beside
//! `Wrap<u8>`, share the node and are two Rust types, so the lowering lowers
//! the fields, the methods and the payloads of every Rust type a use names
//! under the arguments of that use, once for each, and so reaches every use
//! their own types make, `Inner<u8>` beside `Inner<i32>`. An inherent block
//! belongs to the struct its self type names through every type alias, and of a
//! generic struct to the uses whose arguments it matches, so `impl Page<User>`
//! lends its methods to `Page<User>` alone, and a block whose uses the builder
//! cannot tell is refused at its line. A type of a path dependency lowers as a
//! type of the crate, whose source the reader reads.
//!
//! The doc comment of a struct, a field and a method, and of every enum of
//! unit variants, is its description, read as the derive path reads it,
//! wherever a field, a list, an option, an argument or the field of an input
//! holds the type, as the inference reads the comment of a class, of a member
//! and of a TypeScript `enum` from the symbol of its type. The comment of a
//! variant and of an enum whose variants each hold one type reaches no
//! artifact, as the inference prints no description of an enum value and
//! reads none of a union alias, whose type carries no symbol, and the dump
//! keeps both for the authoring map. An argument takes the `@param` lines of
//! its method, and an argument of a method that answers the payload of a
//! mutation takes none, as the inference reads no comment through the rest
//! parameter of the function its wrapper answers.
//!
//! A method that answers `MutationPayload<T>` of the facade, or a `Result` of
//! one, from a plain `fn` or an `async fn`, is the twin of an entry under
//! `mutation(...)`, so it answers the promise of the return type of that
//! wrapper,
//! `{[K in keyof Awaited<R>]: Awaited<R>[K] | null} & {userErrors: UserError[]}`:
//! the intersection of an object with no name of its own that holds the public
//! fields of the struct `T` in their order, each nullable with its comment,
//! and the one object `{userErrors: UserError[]}` every payload shares, which
//! the schema prints as one type with `userErrors: [UserError!]!` last. The
//! inference names the intersection after the field it is reached through and
//! then each of its members, so at the first payload the mapped object, unless
//! it holds no field, and the object of `userErrors` take names of the counter
//! of that field, which a later type of that name skips, `Close_2` beside
//! `Close` and `Rename_2` for a struct `Rename` beside the payload of
//! `rename`. The payload of one struct is one intersection, as the mapped type
//! of one class is one type, so two methods that answer the payload of the
//! same struct answer one type, named after the first. The payload of a type
//! that is no struct of the crate is refused, and so is a struct with a public
//! method, which the inference prints in the payload while the spread of the
//! wrapper drops it from the answer, `MutationPayload` anywhere but in the
//! return type of a method, and a return type that names a type alias of a
//! payload, whose refusal names the alias.
//!
//! An argument reads into a value the engine owns, through the `FromArgument`
//! of the Rust type its parameter names. The lowering records the uses of a
//! struct each parameter reads into and marks every Rust type an argument
//! reaches through them and through the public fields of each, so the glue
//! reads an input object into exactly those, and a use that only answers takes
//! no reader, whatever node it shares. A type of a parameter or of a public
//! field of a marked use that the lowering describes and no reader takes, a
//! borrow, `str`, `char`, `Rc`, `Arc`, `Cow`, `VecDeque`, a slice, an array, a
//! `Result`, a promise or a repeater of the facade, a payload enum, `()` or a
//! type with no value, is refused at the line that writes it, with the owned
//! type to write in its place.
//!
//! The roots are the parts the entry registers. A root is one object of the
//! public members of every part in registration order, the twin of the root
//! object literal of the index of a TypeScript service, whose members are
//! arrow functions, as the
//! derive path presents the root of a fragment of `graphql!` and the merged
//! roots of `compose`. A name two parts hold is refused, as the plan's rule D4
//! asks. A part with no public member registers nothing, so an empty root
//! leaves no trace. Every construct the source cannot state is refused with
//! the file and the line of the author, and the refusal says what to write in
//! its place.

use std::rc::Rc;

use aosabi_core::descriptor::{
    Date,
    Described,
    File,
    Float,
    ID,
    Int,
    JSON,
    Primitive,
    TypeExpr,
    TypeInfo,
    TypeKind,
    TypeLink,
    UserError,
    Visibility, //
};
use aosabi_json::{
    JsObject,
    JsValue,
    map::IndexMap, //
};
use syn::Type;

use super::{
    Place,
    Refusal,
    Registration,
    RootKind,
    docs::{
        self,
        Documentation, //
    },
    entry::type_arguments,
    model::{
        Data,
        Docs,
        Enumeration,
        Kind,
        Method,
        Receiver,
        Shape, //
    },
    names::{
        NameKind,
        graphql_name, //
    },
    reach::{
        ALIAS_DEPTH,
        Arg,
        Block,
        Cx,
        INSTANTIATIONS,
        Unmatched,
        applies,
        blocks,
        forbidden,
        macro_calls,
        type_text,
        zip, //
    },
    resolve::{
        Glob,
        GlobOf,
        Miss,
        Ns,
        Res,
        Resolver, //
    }, //
};

/// The roots in the order of the dump.
const ROOTS: [RootKind; 3] = [RootKind::Query, RootKind::Mutation, RootKind::Subscription];

/// The name of the payload of a mutation in the facade.
const PAYLOAD: &str = "MutationPayload";

/// The file of the reference that declares the wrapper of a mutation, which
/// the two objects of a payload name as their source, as the derive path
/// does.
const MUTATION_TS: &str = "aosabi/core/mutation.ts";

/// Returns the name a root takes in the schema.
fn root_name(kind: RootKind) -> &'static str {
    match kind {
        RootKind::Query => "Query",
        RootKind::Mutation => "Mutation",
        RootKind::Subscription => "Subscription",
    }
}

/// A site that writes a type: its key, as the reach names a member, and its
/// place.
struct Site {
    /// The key, `Type.field`, `Type.method` or `Type.method(param)`.
    key: String,
    /// The place of the site.
    place: Place,
}

/// One public member of a lowered struct, which a root of several parts
/// lists again.
#[derive(Clone)]
struct Member {
    /// The GraphQL name.
    name: String,
    /// The dump of the field or of the method.
    value: JsValue,
    /// Whether it is a method.
    method: bool,
    /// The node of the type of a field, `None` for a method.
    ty: Option<u32>,
    /// The key of the member, for a refusal.
    key: String,
    /// The place of the member.
    place: Place,
}

/// What the glue of a plain crate needs of the types the lowering lowered,
/// beside the dump, which the module `glue` turns into the generated module of
/// the crate.
#[derive(Default)]
pub(crate) struct Plan {
    /// Every use of a struct, once for each Rust type the uses name, in the
    /// order the lowering finishes its members.
    pub(crate) objects: Vec<PlanObject>,
    /// Every use of an enum, in the same way.
    pub(crate) enums: Vec<PlanEnum>,
}

/// One use of a struct of the crate, as its glue reads it.
#[derive(Clone)]
pub(crate) struct PlanObject {
    /// The struct, as an index into the items.
    pub(crate) item: usize,
    /// The node of the use in the dump, which the uses that print one
    /// GraphQL type share.
    pub(crate) node: u32,
    /// The arguments of the use.
    pub(crate) args: Rc<Vec<Option<Arg>>>,
    /// The Rust type of the arguments of the use, as `rust_type` of the
    /// lowering writes it, which tells the use from another use of its node.
    pub(crate) rust: String,
    /// Whether the fields of the struct are named.
    pub(crate) named: bool,
    /// Every named field with its GraphQL name, `None` for a field the API
    /// does not see.
    pub(crate) fields: Vec<(String, Option<String>)>,
    /// The public methods with a receiver of every inherent block of the use.
    pub(crate) methods: Vec<PlanMethod>,
    /// Whether an argument reaches the Rust type of the use, which the
    /// lowering decides once every use stands.
    pub(crate) input: bool,
}

/// One public method of an inherent block, as its glue reads it.
#[derive(Clone)]
pub(crate) struct PlanMethod {
    /// The name of the method.
    pub(crate) name: String,
    /// The GraphQL name of the method.
    pub(crate) key: String,
    /// Whether it is an `async fn`.
    pub(crate) asyncness: bool,
    /// Whether it answers a stream, `impl Stream<Item = T>` or a `Result` of
    /// one.
    pub(crate) stream: bool,
    /// Whether it answers the payload of a mutation, `MutationPayload<T>` or
    /// a `Result` of one, which its glue runs under the mutation wrapper.
    pub(crate) payload: bool,
    /// Every parameter, in order.
    pub(crate) params: Vec<PlanParam>,
}

/// What a method answers, which decides its node and the glue of its call.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Answer {
    /// A value, or a `Result` of one.
    Value,
    /// A stream, `impl Stream<Item = T>` or a `Result` of one.
    Stream,
    /// The payload of a mutation, `MutationPayload<T>` or a `Result` of one.
    Payload,
}

/// What the return type of a method is to the payload of a mutation.
enum PayloadValue<'t> {
    /// A type that is no `MutationPayload` of the facade.
    Not,
    /// `MutationPayload<T>` of the facade, with its type argument.
    Of(&'t Type),
    /// `MutationPayload` of the facade written with no argument.
    Bare,
}

/// The payload of a mutation whose nodes wait until every node of the
/// lowering stands, since the struct it copies the fields of may still be
/// lowered when the method is.
struct PayloadSite {
    /// The node of the payload, the intersection.
    node: u32,
    /// The node of the type argument of `MutationPayload`.
    of: u32,
    /// The type argument as the source writes it.
    text: String,
    /// Whether the lowering refused a type it met in the type argument,
    /// whose refusal then stands for a payload of a type that is no struct.
    refused: bool,
    /// The key of the method, which a refusal names.
    key: String,
    /// The GraphQL name of the method, which the name of the mapped object
    /// carries.
    field: String,
    /// The place of the method.
    place: Place,
}

/// One parameter of a public method, as its glue reads it.
#[derive(Clone)]
pub(crate) struct PlanParam {
    /// The GraphQL name of the argument.
    pub(crate) name: String,
    /// The key of the parameter, `Type.method(param)`, which a refusal names.
    pub(crate) key: String,
    /// The uses of a struct the argument reads into.
    pub(crate) reads: Reads,
}

/// The uses of a struct a value of a type reads into, each the struct as an
/// index into the items with the arguments of the use as the site that
/// reaches it writes them.
pub(crate) type Reads = Vec<(usize, UseArgs)>;

/// The arguments of a use of a struct or an enum, `None` for a parameter no
/// argument substitutes.
pub(crate) type UseArgs = Rc<Vec<Option<Arg>>>;

/// A type the lowering describes and no reader of the engine takes, at the
/// place that writes it.
struct Unread {
    /// The type as the source writes it.
    text: String,
    /// The place that writes it.
    place: Place,
    /// What to write in its place.
    convention: String,
}

/// What an argument of a type reads into.
#[derive(Default)]
struct ArgumentReach {
    /// The uses of a struct it reads into.
    reads: Reads,
    /// Every type of it no reader of the engine takes.
    unread: Vec<Unread>,
}

/// One use of an enum of the crate, as its glue reads it.
#[derive(Clone)]
pub(crate) struct PlanEnum {
    /// The enum, as an index into the items.
    pub(crate) item: usize,
    /// The arguments of the use.
    pub(crate) args: Rc<Vec<Option<Arg>>>,
    /// Every variant with its GraphQL value, for an enum of unit variants,
    /// or with `None`, for an enum whose variants each hold one type.
    pub(crate) variants: Vec<(String, Option<String>)>,
    /// The node of the type each variant holds, in the order of the
    /// variants, for an enum whose variants each hold one type.
    pub(crate) members: Vec<u32>,
}

/// The state of the lowering.
struct Lowering<'a> {
    /// The resolver of the crate.
    resolver: &'a Resolver<'a>,
    /// The name of the package, the crate of every node of the crate.
    package: &'a str,
    /// The name of the entry, which the Rust text of a root names.
    entry: &'a str,
    /// The inherent impl blocks of each item, in the order of the walk.
    impls: Vec<Vec<Block>>,
    /// Whether the lowering refused each impl block, which it refuses once.
    refused_blocks: Vec<bool>,
    /// The macro calls at the level of items, with their places and their
    /// libraries.
    macros: Vec<(String, Place, usize)>,
    /// The nodes, `None` while a node is reserved and its members are
    /// lowered.
    types: Vec<Option<JsValue>>,
    /// The node of each type identity.
    keys: IndexMap<String, u32>,
    /// The concrete uses of each generic item taken so far.
    uses: IndexMap<usize, usize>,
    /// The arguments each node of a struct was lowered with.
    lowered: IndexMap<u32, Rc<Vec<Option<Arg>>>>,
    /// Every node of a struct and impl block for which the lowering refused
    /// a use that shares the node, which it refuses once.
    shared: Vec<(u32, usize)>,
    /// The Rust type of the arguments of every use of a generic struct or
    /// enum whose members the lowering lowered, with the node of the use, so
    /// it lowers the members of each Rust type once.
    rust_types: Vec<(u32, String)>,
    /// The public members of each lowered struct, by node.
    members: IndexMap<u32, Vec<Member>>,
    /// The refusals so far, in the order the lowering meets them.
    refusals: Vec<Refusal>,
    /// What the glue needs of each use of a struct and of an enum.
    plan: Plan,
    /// The payloads of mutations whose nodes the lowering writes last.
    payloads: Vec<PayloadSite>,
}

/// Lowers the roots `registrations` of the crate the resolver holds, whose
/// package is `package` and whose entry is the function `entry`, into the
/// dump of a descriptor graph.
///
/// Answers the dump, every construct the lowering refuses, each at the place
/// of the author, and the plan of the glue of every use of a struct and of
/// an enum the dump holds. A dump beside a refusal is incomplete and the
/// caller writes nothing from it.
pub(crate) fn lower(
    resolver: &Resolver<'_>,
    package: &str,
    entry: &str,
    registrations: &[Registration],
) -> (JsValue, Vec<Refusal>, Plan) {
    let mut lowering = Lowering {
        resolver,
        package,
        entry,
        impls: blocks(resolver),
        refused_blocks: vec![false; resolver.items.len()],
        macros: macro_calls(resolver),
        types: Vec::new(),
        keys: IndexMap::default(),
        uses: IndexMap::default(),
        lowered: IndexMap::default(),
        shared: Vec::new(),
        rust_types: Vec::new(),
        members: IndexMap::default(),
        refusals: Vec::new(),
        plan: Plan::default(),
        payloads: Vec::new(),
    };

    let mut roots = JsObject::new();
    for kind in ROOTS {
        let parts: Vec<&Registration> = registrations
            .iter()
            .filter(|registration| registration.kind == kind)
            .collect();
        let root = lowering.root(kind, &parts);
        roots.insert(root_name(kind), root.map_or(JsValue::Null, JsValue::from));
    }
    lowering.finish_payloads();
    lowering.mark_inputs();

    let mut dump = JsObject::new();
    dump.insert("version", 1_u32);
    dump.insert("crate", package);
    dump.insert("roots", roots);
    dump.insert(
        "types",
        lowering
            .types
            .into_iter()
            .map(|node| node.unwrap_or(JsValue::Null))
            .collect::<Vec<JsValue>>(),
    );
    dump.insert("declarations", Vec::<JsValue>::new());
    (JsValue::Object(dump), lowering.refusals, lowering.plan)
}

impl Lowering<'_> {
    /// Adds the refusal `message` at `place`.
    fn refuse(&mut self, place: Place, message: String) {
        self.refusals.push(Refusal { place, message });
    }

    /// Returns the path of the item `index` from the crate root.
    fn path_of(&self, index: usize) -> String {
        self.resolver.path_of(index)
    }

    /// Returns the name of the package of the crate of the item `index`.
    fn package_of(&self, index: usize) -> &str {
        &self.resolver.libraries[self.resolver.krate(index)].package
    }

    /// Returns the node of the identity `key`, making it with `make` when it
    /// is new.
    fn leaf(&mut self, key: &str, make: impl FnOnce() -> JsValue) -> u32 {
        if let Some(node) = self.keys.get(key) {
            return *node;
        }
        let node = self.push(Some(make()));
        self.keys.insert(String::from(key), node);
        node
    }

    /// Adds a node and answers its position.
    fn push(&mut self, node: Option<JsValue>) -> u32 {
        let position = u32::try_from(self.types.len()).unwrap_or(u32::MAX);
        self.types.push(node);
        position
    }

    /// Returns the node of a primitive, `string`, `number` or `boolean`.
    fn primitive(&mut self, primitive: &'static str) -> u32 {
        self.leaf(primitive, || {
            let mut object = node("primitive", primitive);
            object.insert("primitive", primitive);
            JsValue::Object(object)
        })
    }

    /// Returns the node of a type with no further key, `void`, `any` or
    /// `never`.
    fn bare(&mut self, expr: &'static str) -> u32 {
        self.leaf(expr, || JsValue::Object(node(expr, expr)))
    }

    /// Returns the node of a wrapper `expr` of the node `of`, `nullable`,
    /// `list`, `promise` or `repeater`.
    fn wrapper(&mut self, expr: &'static str, of: u32) -> u32 {
        let key = format!("{expr}:{of}");
        let rust = format!("{expr}<{of}>");
        self.leaf(&key, || {
            let mut object = node(expr, &rust);
            object.insert("of", of);
            JsValue::Object(object)
        })
    }

    /// Returns the node of a type a descriptor of `aosabi-core` describes, a
    /// marker of the facade or the JSON object of `aosabi-json`.
    fn described(&mut self, key: &str, expr: TypeExpr) -> u32 {
        match expr {
            TypeExpr::Named(info) => self.leaf(key, || static_node(info)),
            _ => self.bare("any"),
        }
    }

    /// Returns the node of the root `kind` of the parts `parts`, or `None`
    /// when no part holds a public member.
    fn root(&mut self, kind: RootKind, parts: &[&Registration]) -> Option<u32> {
        let mut filled: Vec<(u32, &Registration)> = Vec::new();
        for part in parts {
            let site = Site {
                key: part.path.clone(),
                place: part.place.clone(),
            };
            if !self.generics(part.item).is_empty() {
                self.refuse(part.place.clone(), refuse_generic_root(&part.path));
                continue;
            }
            let node = self.instance(part.item, Vec::new(), &site);
            if self
                .members
                .get(&node)
                .is_some_and(|members| !members.is_empty())
            {
                filled.push((node, part));
            }
        }
        if filled.is_empty() {
            return None;
        }
        Some(self.root_object(kind, &filled))
    }

    /// Returns the object of the root `kind` that holds the public members of
    /// every part of `parts` in registration order, the fields of the parts
    /// first and their methods after them, and refuses a name two parts hold.
    ///
    /// The object is the twin of the root object literal of the index of a
    /// TypeScript service, `Query: { greeting: () => .. }`, whose members are
    /// arrow functions, so a member presents as an arrow function whatever its
    /// name, as the root of a fragment of `graphql!` and the merged roots of
    /// `compose` present on the derive path. A class method named after a
    /// scalar in lower case, `int` or `date`, would take that scalar by the
    /// naming of the type of the method in the inference, which a member of a
    /// root never does.
    fn root_object(&mut self, kind: RootKind, parts: &[(u32, &Registration)]) -> u32 {
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut seen: Vec<Member> = Vec::new();
        for (node, part) in parts {
            let members = self.members.get(node).cloned().unwrap_or_default();
            for member in members {
                if let Some(first) = seen.iter().find(|known| known.name == member.name) {
                    let message = refuse_root_name(kind, &member.name, first, &member);
                    self.refuse(part.place.clone(), message);
                    continue;
                }
                if member.method {
                    methods.push(member.value.clone());
                } else {
                    fields.push(member.value.clone());
                }
                seen.push(member);
            }
        }
        let first = &parts[0].1;
        let source = self.resolver.items[first.item].place.clone();
        let mut object = node("named", &format!("{}().{}", self.entry, root_name(kind)));
        object.insert("kind", "object");
        object.insert("name", root_name(kind));
        object.insert("krate", self.package);
        object.insert("source", source_value(&source));
        object.insert("docs", "");
        object.insert("tags", Vec::<JsValue>::new());
        object.insert("site", site_value(&source, &Documentation::default()));
        object.insert("typename", JsValue::Null);
        object.insert("anonymous", false);
        object.insert("fields", fields);
        object.insert("methods", methods);
        object.insert("variants", Vec::<JsValue>::new());
        object.insert("members", Vec::<JsValue>::new());
        object.insert("typeArguments", Vec::<JsValue>::new());
        object.insert("heritage", Vec::<JsValue>::new());
        object.insert("fragment", "");
        self.push(Some(JsValue::Object(object)))
    }

    /// Returns the type parameters of the struct or the enum `index`.
    fn generics(&self, index: usize) -> &[String] {
        match &self.resolver.items[index].kind {
            Kind::Struct(data) => &data.generics,
            Kind::Enum(variants) => &variants.generics,
            _ => &[],
        }
    }

    /// Returns the node of the type `ty`, written in the context `cx` at
    /// `site`.
    ///
    /// A type the source cannot state is refused at the site and answers the
    /// any marker, so the lowering goes on and finds every refusal of the
    /// crate in one run.
    fn ty(&mut self, ty: &Type, cx: &Cx, site: &Site) -> u32 {
        match ty {
            Type::Paren(inner) => self.ty(&inner.elem, cx, site),
            Type::Group(inner) => self.ty(&inner.elem, cx, site),
            Type::Reference(inner) => self.ty(&inner.elem, cx, site),
            Type::Slice(inner) => {
                let of = self.ty(&inner.elem, cx, site);
                self.wrapper("list", of)
            }
            Type::Array(inner) => {
                let of = self.ty(&inner.elem, cx, site);
                self.wrapper("list", of)
            }
            Type::Never(_) => self.bare("never"),
            Type::Tuple(tuple) if tuple.elems.is_empty() => self.bare("void"),
            Type::Path(path) => self.path(path, ty, cx, site),
            Type::ImplTrait(_) | Type::TraitObject(_) => {
                self.refuse_type(site, refuse_trait(&type_text(ty), &site.key))
            }
            Type::Macro(_) => self.refuse_type(site, refuse_macro(&type_text(ty), &site.key)),
            _ => self.refuse_type(site, refuse_shape(ty, &site.key)),
        }
    }

    /// Refuses the type of `site` with `message` and answers the any marker.
    fn refuse_type(&mut self, site: &Site, message: String) -> u32 {
        self.refuse(site.place.clone(), message);
        self.bare("any")
    }

    /// Returns the node of the path type `path`, which is `ty`, written in
    /// `cx` at `site`.
    fn path(&mut self, path: &syn::TypePath, ty: &Type, cx: &Cx, site: &Site) -> u32 {
        let text = type_text(ty);
        if path.qself.is_some() {
            return self.refuse_type(site, refuse_associated(&text, &site.key));
        }
        let segments = Resolver::segments(&path.path);
        let leading = path.path.leading_colon.is_some();
        if !leading
            && segments.len() == 1
            && type_arguments(&path.path).is_empty()
            && let Some((name, arg)) = cx.params.iter().find(|(name, _)| *name == segments[0])
        {
            if let Some(arg) = arg {
                let arg = arg.clone();
                return self.ty(&arg.ty, &arg.cx, site);
            }
            let message = refuse_generic(name, &cx.owner, &site.key);
            return self.refuse_type(site, message);
        }
        if segments.first().is_some_and(|first| first == "Self") && !leading {
            if segments.len() > 1 {
                return self.refuse_type(site, refuse_associated(&text, &site.key));
            }
            let Some((owner, args)) = &cx.self_item else {
                return self.refuse_type(site, refuse_associated(&text, &site.key));
            };
            let args = args.as_ref().clone();
            return self.instance(*owner, args, site);
        }
        let arguments: Vec<Arg> = type_arguments(&path.path)
            .into_iter()
            .map(|argument| Arg {
                ty: argument.clone(),
                cx: cx.clone(),
            })
            .collect();
        match self
            .resolver
            .resolve(cx.module, leading, &segments, Ns::Type)
        {
            Ok(Res::Item(index)) => self.named(index, arguments, &text, cx, site),
            Ok(Res::Std(named)) => self.standard(&named, &arguments, &text, site),
            Ok(Res::Facade(named)) => self.facade(&named, &arguments, &text, site),
            Ok(Res::Prim(name)) => self.number_or_primitive(&name),
            Ok(Res::Extern(named)) => {
                let message = refuse_extern(&text, &named, &site.key);
                self.refuse_type(site, message)
            }
            Ok(Res::Module(_) | Res::Variant(..)) => {
                self.refuse_type(site, refuse_no_type(&text, &site.key))
            }
            Err(Miss::Associated) => self.refuse_type(site, refuse_associated(&text, &site.key)),
            Err(Miss::Unknown { name, globs }) => {
                let krate = self.resolver.modules[cx.module].krate;
                let message = self.refuse_unknown(&name, &globs, &text, &site.key, krate);
                self.refuse_type(site, message)
            }
        }
    }

    /// Returns the node of the primitive type `name`, a Rust number by its
    /// width.
    fn number_or_primitive(&mut self, name: &str) -> u32 {
        match name {
            "bool" => self.primitive("boolean"),
            "i8" | "i16" | "i32" | "u8" | "u16" => {
                self.described("marker:Int", <Int as Described>::TYPE)
            }
            "f32" | "f64" => self.described("marker:Float", <Float as Described>::TYPE),
            "i64" | "u32" | "u64" | "i128" | "u128" | "isize" | "usize" => self.primitive("number"),
            // `str` and `char`, the two other primitives a resolution
            // answers.
            _ => self.primitive("string"),
        }
    }

    /// Returns the node of the name of `std`, `core` or `alloc` whose
    /// segments after the crate are `named`, with the type arguments
    /// `arguments`, written as `text` at `site`.
    fn standard(&mut self, named: &[String], arguments: &[Arg], text: &str, site: &Site) -> u32 {
        let last = named.last().map_or("", String::as_str);
        match last {
            "String" => self.primitive("string"),
            "Infallible" => self.bare("never"),
            "Option" => self.wrapped(arguments, "nullable", text, site),
            "Vec" | "VecDeque" => self.wrapped(arguments, "list", text, site),
            "Box" | "Rc" | "Arc" | "Cow" | "Result" => self.wrapped(arguments, "", text, site),
            _ => self.refuse_type(site, refuse_std(text, &site.key)),
        }
    }

    /// Returns the node of the name of the facade whose segments after
    /// `aosabi` are `named`, with the type arguments `arguments`, written as
    /// `text` at `site`.
    fn facade(&mut self, named: &[String], arguments: &[Arg], text: &str, site: &Site) -> u32 {
        let last = named.last().map_or("", String::as_str);
        match last {
            "ID" => self.described("marker:ID", <ID as Described>::TYPE),
            "Int" => self.described("marker:Int", <Int as Described>::TYPE),
            "Float" => self.described("marker:Float", <Float as Described>::TYPE),
            "Date" => self.described("marker:Date", <Date as Described>::TYPE),
            "File" => self.described("marker:File", <File as Described>::TYPE),
            "JSON" | "JsValue" | "Value" => {
                self.described("marker:JSON", <JSON as Described>::TYPE)
            }
            "Map" | "JsObject" => {
                self.described("record", <aosabi_json::JsObject as Described>::TYPE)
            }
            "Any" => self.bare("any"),
            "Number" => self.primitive("number"),
            "Promise" => self.wrapped(arguments, "promise", text, site),
            "Repeater" => self.wrapped(arguments, "repeater", text, site),
            "Result" | "Unchecked" => self.wrapped(arguments, "", text, site),
            PAYLOAD => self.refuse_type(site, refuse_payload_place(text, &site.key)),
            _ => self.refuse_type(site, refuse_facade(text, &site.key)),
        }
    }

    /// Returns the node of the first type argument of a wrapper of the
    /// argument list `arguments`, wrapped in a node of the kind `expr`, or
    /// itself for the empty kind of a box and a `Result`.
    fn wrapped(&mut self, arguments: &[Arg], expr: &'static str, text: &str, site: &Site) -> u32 {
        let Some(arg) = arguments.first() else {
            return self.refuse_type(site, refuse_no_argument(text, &site.key));
        };
        let of = self.ty(&arg.ty, &arg.cx, site);
        if expr.is_empty() {
            of
        } else {
            self.wrapper(expr, of)
        }
    }

    /// Returns the node of the item `index` a type `text` names with the
    /// arguments `arguments`, written in `cx` at `site`.
    fn named(
        &mut self,
        index: usize,
        arguments: Vec<Arg>,
        text: &str,
        cx: &Cx,
        site: &Site,
    ) -> u32 {
        let resolver = self.resolver;
        let item = &resolver.items[index];
        match &item.kind {
            Kind::Struct(_) | Kind::Enum(_) => {
                let count = self.generics(index).len();
                let mut args: Vec<Option<Arg>> = arguments.into_iter().map(Some).collect();
                args.resize(count, None);
                self.instance(index, args, site)
            }
            Kind::Alias(alias) => {
                if cx.aliases >= ALIAS_DEPTH {
                    return self.refuse_type(site, refuse_alias_chain(text, &site.key));
                }
                let args: Vec<Option<Arg>> = arguments.into_iter().map(Some).collect();
                let target = Cx {
                    module: item.module,
                    params: Rc::new(zip(&alias.generics, &args)),
                    self_item: None,
                    owner: Rc::from(self.path_of(index).as_str()),
                    aliases: cx.aliases + 1,
                };
                self.ty(&alias.ty, &target, site)
            }
            Kind::Trait => self.refuse_type(site, refuse_trait(text, &site.key)),
            Kind::Union => self.refuse_type(site, refuse_union(text, &site.key)),
            _ => self.refuse_type(site, refuse_no_type(text, &site.key)),
        }
    }

    /// Returns the node of the struct or the enum `index` with the arguments
    /// `args`, lowering it on its first use with arguments that print its
    /// GraphQL type, and lowering its members again for a later use that
    /// names a Rust type of its own.
    fn instance(&mut self, index: usize, args: Vec<Option<Arg>>, site: &Site) -> u32 {
        let path = self.path_of(index);
        let mut arg_nodes = Vec::with_capacity(args.len());
        for (position, arg) in args.iter().enumerate() {
            if let Some(arg) = arg {
                arg_nodes.push(self.ty(&arg.ty, &arg.cx, site));
                continue;
            }
            let name = self
                .generics(index)
                .get(position)
                .cloned()
                .unwrap_or_default();
            let message = refuse_generic(&name, &path, &site.key);
            arg_nodes.push(self.refuse_type(site, message));
        }
        let key = format!(
            "item:{index}<{}>",
            arg_nodes
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        if let Some(node) = self.keys.get(&key).copied() {
            self.shared_use(index, node, &args, site);
            if !args.is_empty() {
                self.reuse(index, node, &Rc::new(args), &arg_nodes, site);
            }
            return node;
        }
        if !arg_nodes.is_empty() {
            let taken = self.uses.get(&index).copied().unwrap_or(0);
            if taken >= INSTANTIATIONS {
                return self.refuse_type(site, refuse_instantiations(&path, &site.key));
            }
            self.uses.insert(index, taken + 1);
        }
        let node = self.push(None);
        self.keys.insert(key, node);
        let args = Rc::new(args);
        if !args.is_empty() {
            let rust = self.rust_type(&args);
            self.rust_types.push((node, rust));
        }
        self.lowered.insert(node, Rc::clone(&args));
        let value = self.item(index, node, &args, &arg_nodes);
        self.types[usize::try_from(node).unwrap_or(usize::MAX)] = Some(value);
        node
    }

    /// Returns the dump of the struct or the enum `index` at the position
    /// `node` with the arguments `args` lowered to `arg_nodes`, and records
    /// its public members and the glue of the use.
    fn item(
        &mut self,
        index: usize,
        node: u32,
        args: &Rc<Vec<Option<Arg>>>,
        arg_nodes: &[u32],
    ) -> JsValue {
        let resolver = self.resolver;
        match &resolver.items[index].kind {
            Kind::Struct(data) => self.structure(index, node, data, args, arg_nodes),
            Kind::Enum(variants) => self.enumeration(index, variants, args, arg_nodes),
            _ => JsValue::Null,
        }
    }

    /// Lowers the members of the use with the arguments `args` of the struct
    /// or the enum `index` at `site`, whose arguments lower to `arg_nodes`
    /// and so to the node `node` an earlier use made, when the use names a
    /// Rust type the lowering has not lowered.
    ///
    /// Two uses whose arguments print one GraphQL type, `Wrap<i32>` beside
    /// `Wrap<u8>` or `Wrap<Rc<Tag>>` beside `Wrap<Arc<Tag>>`, share the node
    /// and are two Rust types, and the glue implements its traits for each.
    /// A field, a method or a payload of the later use names the types of
    /// its own arguments, `Inner<u8>` where the first use names `Inner<i32>`,
    /// so the lowering lowers them again under those arguments, which
    /// reaches every use they make and records the glue of the later use. The
    /// node keeps the dump of the first use, which the second lowering
    /// repeats, and a refusal the second lowering repeats is dropped. A Rust
    /// type counts as a concrete use of its type, so a type that names
    /// itself with a new argument at every step, `Node<Box<T>>` in `Node<T>`,
    /// stops at the bound of [`INSTANTIATIONS`] and is refused.
    fn reuse(
        &mut self,
        index: usize,
        node: u32,
        args: &Rc<Vec<Option<Arg>>>,
        arg_nodes: &[u32],
        site: &Site,
    ) {
        let rust = self.rust_type(args);
        if self
            .rust_types
            .iter()
            .any(|(known, text)| *known == node && *text == rust)
        {
            return;
        }
        let taken = self.uses.get(&index).copied().unwrap_or(0);
        if taken >= INSTANTIATIONS {
            let message = refuse_instantiations(&self.path_of(index), &site.key);
            self.refuse(site.place.clone(), message);
            return;
        }
        self.uses.insert(index, taken + 1);
        self.rust_types.push((node, rust));
        let from = self.refusals.len();
        // The dump repeats the node of the first use.
        let _ = self.item(index, node, args, arg_nodes);
        self.drop_repeated(from);
    }

    /// Returns the Rust type of the arguments `args` of a use, as the glue
    /// names each from the crate root, or as the source writes one the glue
    /// cannot name, which the glue refuses at its line.
    fn rust_type(&self, args: &[Option<Arg>]) -> String {
        args.iter()
            .map(|arg| match arg {
                Some(arg) => super::glue::type_text(self.resolver, &arg.ty, &arg.cx)
                    .unwrap_or_else(|| type_text(&arg.ty)),
                None => String::from("_"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Drops every refusal from the position `from` on that repeats one
    /// before it, the same message at the same place.
    fn drop_repeated(&mut self, from: usize) {
        let fresh = self.refusals.split_off(from);
        for refusal in fresh {
            let repeated = self
                .refusals
                .iter()
                .any(|known| known.place == refusal.place && known.message == refusal.message);
            if !repeated {
                self.refusals.push(refusal);
            }
        }
    }

    /// Refuses at `site` the use with the arguments `args` of the struct
    /// `index` whose node `node` a use with other arguments made, when an
    /// inherent block belongs to one of the two uses alone.
    ///
    /// Two arguments that lower to one node, `i32` and `u8` to `Int` or
    /// `String` and `&str` to `String`, make one GraphQL type of the two
    /// uses, which cannot hold the methods of a block for one of them and
    /// lack them for the other.
    fn shared_use(&mut self, index: usize, node: u32, args: &[Option<Arg>], site: &Site) {
        let resolver = self.resolver;
        if !matches!(resolver.items[index].kind, Kind::Struct(_)) {
            return;
        }
        let Some(first) = self.lowered.get(&node).cloned() else {
            return;
        };
        let path = self.path_of(index);
        for block in self.impls[index].clone() {
            let belongs = |args: &[Option<Arg>]| {
                matches!(applies(resolver, &block, index, args), Ok(Some(_)))
            };
            if belongs(&first) == belongs(args) || self.shared.contains(&(node, block.item)) {
                continue;
            }
            self.shared.push((node, block.item));
            let Kind::Impl(imp) = &resolver.items[block.item].kind else {
                continue;
            };
            let message = refuse_shared_use(
                &use_text(&path, &first),
                &use_text(&path, args),
                &type_text(&imp.self_ty),
                &resolver.items[block.item].place,
            );
            self.refuse(site.place.clone(), message);
        }
    }

    /// Returns the node of the struct `index` at the position `node`, whose
    /// fields are `data`, with the arguments `args` lowered to `arg_nodes`,
    /// and records its public members.
    fn structure(
        &mut self,
        index: usize,
        node: u32,
        data: &Data,
        args: &Rc<Vec<Option<Arg>>>,
        arg_nodes: &[u32],
    ) -> JsValue {
        let resolver = self.resolver;
        let item = &resolver.items[index];
        let path = self.path_of(index);
        let documentation = self.documentation(&path, &item.docs);
        if data.shape == Shape::Tuple && !data.fields.is_empty() {
            self.refuse(item.place.clone(), refuse_tuple_struct(&path));
        }
        let mut members = Vec::new();
        let mut glue_fields = Vec::new();
        let mut glue_methods = Vec::new();
        let fields = self.fields(index, data, args, &mut members, &mut glue_fields);
        let methods = self.methods(index, args, &mut members, &mut glue_methods);
        self.duplicates(&members);
        self.members.insert(node, members);
        self.plan.objects.push(PlanObject {
            item: index,
            node,
            args: Rc::clone(args),
            rust: self.rust_type(args),
            named: data.shape == Shape::Named,
            fields: glue_fields,
            methods: glue_methods,
            input: false,
        });
        let rust = instance_text(&path, &self.types, arg_nodes);
        let package = self.package_of(index);
        let mut object = node_named(&rust, "object", &item.name, package, &item.place);
        object.insert("docs", documentation.comment.as_str());
        object.insert("tags", tags_value(&documentation));
        object.insert("site", site_value(&item.place, &documentation));
        object.insert("typename", JsValue::Null);
        object.insert("anonymous", false);
        object.insert("fields", fields);
        object.insert("methods", methods);
        object.insert("variants", Vec::<JsValue>::new());
        object.insert("members", Vec::<JsValue>::new());
        object.insert("typeArguments", indices(arg_nodes));
        object.insert("heritage", Vec::<JsValue>::new());
        JsValue::Object(object)
    }

    /// Returns the dump of the named fields of the struct `index`, whose
    /// fields are `data`, with the arguments `args`, adds its public fields
    /// to `members`, and adds every named field to `glue` with the GraphQL
    /// name it answers under, `None` for one the API does not see.
    fn fields(
        &mut self,
        index: usize,
        data: &Data,
        args: &Rc<Vec<Option<Arg>>>,
        members: &mut Vec<Member>,
        glue: &mut Vec<(String, Option<String>)>,
    ) -> Vec<JsValue> {
        let mut fields = Vec::new();
        if data.shape != Shape::Named {
            return fields;
        }
        let path = self.path_of(index);
        let cx = Cx {
            module: self.resolver.items[index].module,
            params: Rc::new(zip(&data.generics, args)),
            self_item: Some((index, Rc::clone(args))),
            owner: Rc::from(path.as_str()),
            aliases: 0,
        };
        for field in &data.fields {
            if forbidden(&field.name) {
                glue.push((field.name.clone(), None));
                continue;
            }
            let key = format!("{path}.{}", field.name);
            let name = graphql_name(NameKind::Field, &field.name);
            if !self.resolver.exposes(index, &field.vis) {
                glue.push((field.name.clone(), None));
                fields.push(private_field(&name, &field.place, self.bare("any")));
                continue;
            }
            glue.push((field.name.clone(), Some(name.clone())));
            let documentation = self.documentation(&key, &field.docs);
            let site = Site {
                key: key.clone(),
                place: field.place.clone(),
            };
            let ty = self.ty(&field.ty, &cx, &site);
            let value = field_value(&name, &documentation, &field.place, ty);
            fields.push(value.clone());
            members.push(Member {
                name,
                value,
                method: false,
                ty: Some(ty),
                key,
                place: field.place.clone(),
            });
        }
        fields
    }

    /// Returns the dump of the methods with a receiver of every inherent
    /// block of the struct `index` that belongs to the use with the arguments
    /// `args`, in the order of the walk, and adds its public methods to
    /// `members`.
    ///
    /// A block whose uses the builder cannot tell is refused once, at its
    /// line, and lends no method to any use.
    fn methods(
        &mut self,
        index: usize,
        args: &Rc<Vec<Option<Arg>>>,
        members: &mut Vec<Member>,
        glue: &mut Vec<PlanMethod>,
    ) -> Vec<JsValue> {
        let resolver = self.resolver;
        let path = self.path_of(index);
        let mut methods = Vec::new();
        for block in self.impls[index].clone() {
            let Kind::Impl(imp) = &resolver.items[block.item].kind else {
                continue;
            };
            let bound = match applies(resolver, &block, index, args) {
                Ok(Some(bound)) => bound,
                Ok(None) => continue,
                Err(unmatched) => {
                    if !self.refused_blocks[block.item] {
                        self.refused_blocks[block.item] = true;
                        let text = type_text(&imp.self_ty);
                        let place = resolver.items[block.item].place.clone();
                        self.refuse(place, refuse_block(&text, &path, &unmatched));
                    }
                    continue;
                }
            };
            for method in &imp.methods {
                if method.function.receiver.is_none() || forbidden(&method.name) {
                    continue;
                }
                let name = graphql_name(NameKind::Method, &method.name);
                if !resolver.exposes(index, &method.vis) {
                    methods.push(private_method(&name, &method.place, self.bare("any")));
                    continue;
                }
                // A type parameter of the method itself stands unsubstituted,
                // so a type it names is refused as a type parameter.
                let mut params = bound.clone();
                params.extend(
                    method
                        .function
                        .generics
                        .iter()
                        .map(|name| (name.clone(), None)),
                );
                let cx = Cx {
                    module: resolver.items[block.item].module,
                    params: Rc::new(params),
                    self_item: Some((index, Rc::clone(args))),
                    owner: Rc::from(format!("{path}.{}", method.name).as_str()),
                    aliases: 0,
                };
                let key = format!("{path}.{}", method.name);
                let (value, params, answer) = self.method(method, &name, &key, &cx);
                glue.push(PlanMethod {
                    name: method.name.clone(),
                    key: name.clone(),
                    asyncness: method.function.asyncness,
                    stream: answer == Answer::Stream,
                    payload: answer == Answer::Payload,
                    params,
                });
                methods.push(value.clone());
                members.push(Member {
                    name,
                    value,
                    method: true,
                    ty: None,
                    key,
                    place: method.place.clone(),
                });
            }
        }
        methods
    }

    /// Returns the dump of the public method `method` named `name` at `key`,
    /// whose types are written in `cx`, with the glue of every parameter and
    /// what the method answers, and refuses every type of a parameter that no
    /// reader of the engine takes.
    fn method(
        &mut self,
        method: &Method,
        name: &str,
        key: &str,
        cx: &Cx,
    ) -> (JsValue, Vec<PlanParam>, Answer) {
        let function = &method.function;
        if function.receiver != Some(Receiver::Shared) {
            let form = function.receiver.unwrap_or(Receiver::Shared);
            self.refuse(method.place.clone(), refuse_receiver(key, form));
        }
        if !function.generics.is_empty() {
            self.refuse(method.place.clone(), refuse_method_generics(key));
        }
        let documentation = self.documentation(key, &method.docs);
        // An argument of a field under `mutation(...)` reads through the rest
        // parameter of the function the wrapper answers, which carries no
        // comment, so the inference prints no description for it whatever the
        // comment of the resolver says.
        let payload = function.output.as_ref().is_some_and(|output| {
            let answered = self.result_value(output, cx).unwrap_or(output);
            !matches!(self.payload_value(answered, cx), PayloadValue::Not)
        });
        let mut params = Vec::new();
        let mut glue = Vec::new();
        for param in &function.params {
            if !param.named {
                self.refuse(param.place.clone(), refuse_pattern(key));
                continue;
            }
            let graphql = graphql_name(NameKind::Argument, &param.name);
            let site = Site {
                key: format!("{key}({})", param.name),
                place: param.place.clone(),
            };
            let ty = self.ty(&param.ty, cx, &site);
            let mut reach = ArgumentReach::default();
            self.reads(&param.ty, cx, &site.place, &mut reach);
            for unread in reach.unread {
                let message = refuse_unread(&unread, &site.key, &site.key);
                self.refuse(unread.place, message);
            }
            glue.push(PlanParam {
                name: graphql.clone(),
                key: site.key.clone(),
                reads: reach.reads,
            });
            let docs = if payload {
                Documentation::default()
            } else {
                docs::parameter(&graphql, &documentation)
            };
            let mut object = JsObject::new();
            object.insert("name", graphql.as_str());
            object.insert("type", ty);
            object.insert("optional", false);
            object.insert("docs", docs.comment.as_str());
            object.insert("tags", tags_value(&docs));
            object.insert("site", site_value(&param.place, &docs));
            params.push(JsValue::Object(object));
        }
        let site = Site {
            key: String::from(key),
            place: method.place.clone(),
        };
        let (returns, answer) = match &function.output {
            Some(output) => self.returns(output, cx, &site, name),
            None => (self.bare("void"), Answer::Value),
        };
        if answer == Answer::Stream && function.asyncness {
            self.refuse(method.place.clone(), refuse_async_stream(key));
        }
        // The payload of a mutation is the promise of the wrapper, whether
        // the method is an `async fn` or a plain one, as the function
        // `mutation(...)` answers is async.
        let is_async = function.asyncness || answer == Answer::Payload;
        let returns = if function.asyncness && answer != Answer::Payload {
            self.wrapper("promise", returns)
        } else {
            returns
        };
        let mut object = JsObject::new();
        object.insert("name", name);
        object.insert("docs", documentation.comment.as_str());
        object.insert("tags", tags_value(&documentation));
        object.insert("site", site_value(&method.place, &documentation));
        object.insert("visibility", "public");
        object.insert("isAsync", is_async);
        object.insert("params", params);
        object.insert("returns", returns);
        (JsValue::Object(object), glue, answer)
    }

    /// Returns the node of the return type `output` of the method of the
    /// field `field` and what the method answers, refusing `impl Future` for
    /// the `async fn` a plain crate writes.
    ///
    /// A method that answers `impl Stream<Item = T>` of the facade, or a
    /// `Result` of one, answers the repeater of `T`, the `Repeater<T>` of a
    /// subscription field of a TypeScript service, which the glue drives
    /// through `complete_stream`. A method that answers `MutationPayload<T>` of
    /// the facade, or a `Result` of one, answers the promise of the payload of
    /// a mutation over `T`, which the glue runs under the mutation wrapper.
    fn returns(&mut self, output: &Type, cx: &Cx, site: &Site, field: &str) -> (u32, Answer) {
        let resolver = self.resolver;
        let answered = self.result_value(output, cx).unwrap_or(output);
        if let Type::ImplTrait(bounds) = answered
            && let Some(item) = resolver.stream_item(cx.module, &bounds.bounds)
        {
            let of = self.ty(item, cx, site);
            return (self.wrapper("repeater", of), Answer::Stream);
        }
        match self.payload_value(answered, cx) {
            PayloadValue::Not => {
                if let Some((alias, place)) = self.payload_alias(answered, cx) {
                    let text = type_text(output);
                    let message = refuse_payload_alias(&alias, &place, &text, &site.key);
                    return (self.refuse_type(site, message), Answer::Payload);
                }
            }
            PayloadValue::Of(value) => {
                return (self.payload(value, cx, site, field), Answer::Payload);
            }
            PayloadValue::Bare => {
                let text = type_text(answered);
                return (
                    self.refuse_type(site, refuse_no_argument(&text, &site.key)),
                    Answer::Payload,
                );
            }
        }
        if let Type::ImplTrait(bounds) = output {
            let future = bounds.bounds.iter().any(|bound| {
                matches!(bound, syn::TypeParamBound::Trait(bound)
                    if bound.path.segments.last().is_some_and(|last| last.ident == "Future"))
            });
            if future {
                return (
                    self.refuse_type(site, refuse_future(&site.key)),
                    Answer::Value,
                );
            }
        }
        (self.ty(output, cx, site), Answer::Value)
    }

    /// Returns what `ty`, written in `cx`, is to the payload of a mutation:
    /// the `MutationPayload` of the facade with its type argument, the same
    /// written with no argument, or any other type.
    fn payload_value<'t>(&self, ty: &'t Type, cx: &Cx) -> PayloadValue<'t> {
        let Type::Path(path) = ty else {
            return PayloadValue::Not;
        };
        if path.qself.is_some() {
            return PayloadValue::Not;
        }
        let segments = Resolver::segments(&path.path);
        let leading = path.path.leading_colon.is_some();
        let Ok(Res::Facade(named)) = self
            .resolver
            .resolve(cx.module, leading, &segments, Ns::Type)
        else {
            return PayloadValue::Not;
        };
        if named.last().is_none_or(|last| last != PAYLOAD) {
            return PayloadValue::Not;
        }
        type_arguments(&path.path)
            .into_iter()
            .next()
            .map_or(PayloadValue::Bare, PayloadValue::Of)
    }

    /// Returns the path and the place of the type alias that `ty`, written in
    /// `cx`, names when the alias stands for the payload of a mutation,
    /// `MutationPayload<T>` or a `Result` of it, itself or through a chain of
    /// aliases, and `None` for every other type.
    ///
    /// The builder reads a payload where the return type of a method writes it
    /// and follows no type alias to it, so the refusal of such a return names
    /// the alias the signature writes.
    fn payload_alias(&self, ty: &Type, cx: &Cx) -> Option<(String, Place)> {
        let resolver = self.resolver;
        let mut named = None;
        let mut current = ty;
        let mut module = cx.module;
        for _ in 0..ALIAS_DEPTH {
            let Type::Path(path) = current else {
                return None;
            };
            if path.qself.is_some() {
                return None;
            }
            let segments = Resolver::segments(&path.path);
            let leading = path.path.leading_colon.is_some();
            let Ok(Res::Item(index)) = resolver.resolve(module, leading, &segments, Ns::Type)
            else {
                return None;
            };
            let item = &resolver.items[index];
            let Kind::Alias(alias) = &item.kind else {
                return None;
            };
            let first = *named.get_or_insert(index);
            let target = Cx {
                module: item.module,
                params: Rc::new(Vec::new()),
                self_item: None,
                owner: Rc::from(self.path_of(index).as_str()),
                aliases: 0,
            };
            let answered = self.result_value(&alias.ty, &target).unwrap_or(&alias.ty);
            if !matches!(self.payload_value(answered, &target), PayloadValue::Not) {
                return Some((self.path_of(first), resolver.items[first].place.clone()));
            }
            current = answered;
            module = item.module;
        }
        None
    }

    /// Returns the node of the promise of the payload of a mutation over the
    /// type `value`, written in `cx` at `site`, the answer of the method of
    /// the field `field`.
    ///
    /// The node of the payload is reserved here and written once every node
    /// stands, by [`Self::finish_payloads`], and the payload of one node is
    /// one intersection, as the mapped type of one class is one type.
    fn payload(&mut self, value: &Type, cx: &Cx, site: &Site, field: &str) -> u32 {
        let before = self.refusals.len();
        let of = self.ty(value, cx, site);
        let refused = self.refusals.len() > before;
        let key = format!("payload:{of}");
        let node = if let Some(node) = self.keys.get(&key) {
            *node
        } else {
            let node = self.push(None);
            self.keys.insert(key, node);
            self.payloads.push(PayloadSite {
                node,
                of,
                text: type_text(value),
                refused,
                key: site.key.clone(),
                field: String::from(field),
                place: site.place.clone(),
            });
            node
        };
        self.wrapper("promise", node)
    }

    /// Writes every payload of a mutation, the intersection of the object of
    /// the public fields of its struct in their order, each nullable with its
    /// comment, and the one object of `userErrors` every payload shares, and
    /// refuses a payload of a type that is no struct and of a struct with a
    /// public method.
    ///
    /// The object of `userErrors` names no type parameter of the wrapper, so
    /// the checker gives every payload the same one, and the inference names
    /// it once, after the field of the first payload it meets, which takes a
    /// name of the counter of that field.
    fn finish_payloads(&mut self) {
        let payloads = core::mem::take(&mut self.payloads);
        if payloads.is_empty() {
            return;
        }
        let user_errors = self.user_errors_object();
        for site in payloads {
            let fields = self.payload_fields(&site);
            let rust = format!("{PAYLOAD}<{}>", site.text);
            let name = format!("{}Payload", site.field);
            let mapped = anonymous_object(&format!("{rust} fields"), &name, 3, fields);
            let mapped = self.push(Some(mapped));
            let mut object = node("intersection", &rust);
            object.insert("members", indices(&[mapped, user_errors]));
            let position = usize::try_from(site.node).unwrap_or(usize::MAX);
            if let Some(slot) = self.types.get_mut(position) {
                *slot = Some(JsValue::Object(object));
            }
        }
    }

    /// Returns the node of the object `{userErrors: UserError[]}` of the
    /// return type of the wrapper, over the node of the `UserError` of
    /// `aosabi-core`, which `static_link` writes from its descriptor.
    fn user_errors_object(&mut self) -> u32 {
        let key = "payload:userErrors";
        if let Some(node) = self.keys.get(key) {
            return *node;
        }
        let user_errors = self.static_link(TypeLink::of::<Vec<UserError>>());
        let mut errors = JsObject::new();
        errors.insert("name", "userErrors");
        errors.insert("docs", "");
        errors.insert("tags", Vec::<JsValue>::new());
        errors.insert("site", empty_site());
        errors.insert("visibility", "public");
        errors.insert("type", user_errors);
        let object = anonymous_object(
            "{userErrors: UserError[]}",
            "UserErrors",
            53,
            vec![JsValue::Object(errors)],
        );
        let node = self.push(Some(object));
        self.keys.insert(String::from(key), node);
        node
    }

    /// Returns the public fields of the struct of the payload `site`, each of
    /// a nullable type, and refuses a type that is no struct and a struct
    /// with a public method.
    ///
    /// A private member stays out, as the mapped type of the reference keys
    /// the public members of a class alone, and a field keeps its comment.
    fn payload_fields(&mut self, site: &PayloadSite) -> Vec<JsValue> {
        let Some(members) = self.members.get(&site.of).cloned() else {
            if !site.refused {
                self.refuse(
                    site.place.clone(),
                    refuse_payload_value(&site.text, &site.key),
                );
            }
            return Vec::new();
        };
        let mut fields = Vec::new();
        for member in members {
            let Some(ty) = member.ty else {
                self.refuse(
                    member.place.clone(),
                    refuse_payload_method(&member.key, &site.key),
                );
                continue;
            };
            let JsValue::Object(mut field) = member.value else {
                continue;
            };
            let nullable = self.nullable_of(ty);
            field.insert("type", nullable);
            fields.push(JsValue::Object(field));
        }
        fields
    }

    /// Returns the node of `T | null` over the node `inner`, which is
    /// `inner` itself when it is nullable already.
    fn nullable_of(&mut self, inner: u32) -> u32 {
        let position = usize::try_from(inner).unwrap_or(usize::MAX);
        let nullable = matches!(
            self.types.get(position),
            Some(Some(JsValue::Object(object)))
                if matches!(object.get("expr"), Some(JsValue::String(expr)) if expr.as_str() == Some("nullable"))
        );
        if nullable {
            inner
        } else {
            self.wrapper("nullable", inner)
        }
    }

    /// Returns the node of the type `link` a descriptor of `aosabi-core`
    /// describes, an object with its fields as the derive path walks it, the
    /// `UserError` of a payload of a mutation among them.
    fn static_link(&mut self, link: TypeLink) -> u32 {
        match link.resolve() {
            TypeExpr::Primitive(Primitive::String) => self.primitive("string"),
            TypeExpr::Primitive(Primitive::Number) => self.primitive("number"),
            TypeExpr::Primitive(Primitive::Boolean) => self.primitive("boolean"),
            TypeExpr::Nullable(inner) => {
                let of = self.static_link(inner);
                self.wrapper("nullable", of)
            }
            TypeExpr::List(inner) => {
                let of = self.static_link(inner);
                self.wrapper("list", of)
            }
            TypeExpr::Fallible(inner) | TypeExpr::Boxed(inner) => self.static_link(inner),
            TypeExpr::Named(info) if matches!(info.kind, TypeKind::Object) => {
                self.static_object(info)
            }
            expr => self.described(&format!("static:{}", link.rust_name()), expr),
        }
    }

    /// Returns the node of the object `info` a descriptor of `aosabi-core`
    /// describes, with its fields, their comments and their sites.
    fn static_object(&mut self, info: &'static TypeInfo) -> u32 {
        let key = format!("object:{}:{}", info.krate, info.name);
        if let Some(node) = self.keys.get(&key) {
            return *node;
        }
        let node = self.push(None);
        self.keys.insert(key, node);
        let mut fields = Vec::new();
        for field in info.fields {
            let ty = self.static_link(field.r#type);
            let mut object = JsObject::new();
            object.insert("name", field.name);
            object.insert("docs", field.docs.comment);
            object.insert("tags", static_tags(&field.docs));
            object.insert("site", static_site(&field.site));
            object.insert(
                "visibility",
                if matches!(field.visibility, Visibility::Private) {
                    "private"
                } else {
                    "public"
                },
            );
            object.insert("type", ty);
            fields.push(JsValue::Object(object));
        }
        let mut value = static_node(info);
        if let JsValue::Object(object) = &mut value {
            object.insert("fields", fields);
        }
        let position = usize::try_from(node).unwrap_or(usize::MAX);
        if let Some(slot) = self.types.get_mut(position) {
            *slot = Some(value);
        }
        node
    }

    /// Returns the first type argument of `ty`, written in `cx`, when `ty` is
    /// the `Result` of the standard library or of the facade, the value a
    /// method that may throw answers.
    fn result_value<'t>(&self, ty: &'t Type, cx: &Cx) -> Option<&'t Type> {
        let Type::Path(path) = ty else {
            return None;
        };
        if path.qself.is_some() {
            return None;
        }
        let segments = Resolver::segments(&path.path);
        let leading = path.path.leading_colon.is_some();
        let Ok(Res::Std(named) | Res::Facade(named)) =
            self.resolver
                .resolve(cx.module, leading, &segments, Ns::Type)
        else {
            return None;
        };
        if named.last().is_none_or(|last| last != "Result") {
            return None;
        }
        type_arguments(&path.path).into_iter().next()
    }

    /// Marks every use of a struct an argument reaches, through the parameter
    /// types and through the public fields of every use it reaches, so the
    /// glue reads an input object into exactly those, and refuses every type
    /// of a public field of a marked use that no reader of the engine takes.
    ///
    /// The mark belongs to the Rust type of the use and not to its node. Two
    /// uses that print one GraphQL type, `Wrap<Box<Tag>>` beside
    /// `Wrap<Rc<Tag>>`, are two Rust types, and an argument reads into the
    /// Rust type its parameter names alone, so a use that only answers takes
    /// no reader of an input object, whose field types may have none. The walk
    /// reads the fields of a use under the arguments the site that reaches it
    /// writes, so a field type that takes its type from an argument of the
    /// use is refused at the line of that argument, and a refusal names the
    /// parameter whose argument reaches the field first, in the order of the
    /// walk. A use the lowering refused, which no glue holds, stops the walk.
    fn mark_inputs(&mut self) {
        let mut todo: Vec<(usize, UseArgs, String)> = Vec::new();
        for object in &self.plan.objects {
            for param in object.methods.iter().flat_map(|method| &method.params) {
                for (index, args) in &param.reads {
                    todo.push((*index, Rc::clone(args), param.key.clone()));
                }
            }
        }
        todo.reverse();
        let mut seen: Vec<(usize, String, String)> = Vec::new();
        let mut refused: Vec<(Place, String, String)> = Vec::new();
        while let Some((index, args, via)) = todo.pop() {
            let rust = self.rust_type(&args);
            if seen
                .iter()
                .any(|(known, text, from)| *known == index && *text == rust && *from == via)
            {
                continue;
            }
            seen.push((index, rust.clone(), via.clone()));
            let mut held = false;
            for object in &mut self.plan.objects {
                if object.item == index && object.rust == rust {
                    object.input = true;
                    held = true;
                }
            }
            let resolver = self.resolver;
            let Kind::Struct(data) = &resolver.items[index].kind else {
                continue;
            };
            if !held || data.shape != Shape::Named {
                continue;
            }
            let path = self.path_of(index);
            let cx = Cx {
                module: resolver.items[index].module,
                params: Rc::new(zip(&data.generics, &args)),
                self_item: Some((index, Rc::clone(&args))),
                owner: Rc::from(path.as_str()),
                aliases: 0,
            };
            for field in &data.fields {
                if forbidden(&field.name) || !resolver.exposes(index, &field.vis) {
                    continue;
                }
                let mut reach = ArgumentReach::default();
                self.reads(&field.ty, &cx, &field.place, &mut reach);
                let field_key = format!("{path}.{}", field.name);
                for unread in reach.unread {
                    let known = (unread.place.clone(), field_key.clone(), unread.text.clone());
                    if refused.contains(&known) {
                        continue;
                    }
                    refused.push(known);
                    let message = refuse_unread(&unread, &field_key, &via);
                    self.refuse(unread.place, message);
                }
                for (next, next_args) in reach.reads.into_iter().rev() {
                    todo.push((next, next_args, via.clone()));
                }
            }
        }
    }

    /// Adds to `reach` what an argument of the type `ty`, written in `cx` at
    /// `place`, reads into: every use of a struct, and every type no reader
    /// of the engine takes with what to write in its place.
    ///
    /// The engine reads an argument into a value it owns through the
    /// `FromArgument` of its type: a string, a number, a boolean, a marker or
    /// a JSON value of the facade, an `Option`, a `Vec` and a `Box` of a type
    /// it reads, an enum of unit variants, and a struct whose glue reads an
    /// input object. A borrow, `str`, `char`, `Rc`, `Arc`, `Cow`, `VecDeque`,
    /// a slice, an array, a `Result`, a promise and a repeater of the facade,
    /// an enum whose variants hold a value, `()` and a type with no value have
    /// no reader. A type the lowering refuses adds nothing, since its refusal
    /// stands already.
    fn reads(&self, ty: &Type, cx: &Cx, place: &Place, reach: &mut ArgumentReach) {
        let unread = |convention: String| Unread {
            text: type_text(ty),
            place: self.place_of(ty, cx, place),
            convention,
        };
        match ty {
            Type::Paren(inner) => self.reads(&inner.elem, cx, place, reach),
            Type::Group(inner) => self.reads(&inner.elem, cx, place, reach),
            Type::Reference(inner) => reach.unread.push(unread(owned_of(Some(&inner.elem)))),
            Type::Slice(inner) => reach.unread.push(unread(list_of(Some(&inner.elem)))),
            Type::Array(inner) => reach.unread.push(unread(list_of(Some(&inner.elem)))),
            Type::Never(_) => reach.unread.push(unread(no_value())),
            Type::Tuple(tuple) if tuple.elems.is_empty() => reach.unread.push(unread(no_value())),
            Type::Path(path) if path.qself.is_none() => self.path_reads(path, ty, cx, place, reach),
            _ => {}
        }
    }

    /// Adds to `reach` what an argument of the path type `path`, which is
    /// `ty`, written in `cx` at `place`, reads into, as [`Self::reads`] says.
    fn path_reads(
        &self,
        path: &syn::TypePath,
        ty: &Type,
        cx: &Cx,
        place: &Place,
        reach: &mut ArgumentReach,
    ) {
        let segments = Resolver::segments(&path.path);
        let leading = path.path.leading_colon.is_some();
        let arguments = type_arguments(&path.path);
        if !leading
            && segments.len() == 1
            && arguments.is_empty()
            && let Some((_, arg)) = cx.params.iter().find(|(name, _)| *name == segments[0])
        {
            if let Some(arg) = arg {
                self.reads(&arg.ty, &arg.cx, place, reach);
            }
            return;
        }
        if !leading && segments.first().is_some_and(|first| first == "Self") {
            if segments.len() == 1
                && let Some((owner, args)) = &cx.self_item
            {
                self.item_reads(*owner, args, ty, cx, place, reach);
            }
            return;
        }
        let first = arguments.first().copied();
        let unread = |convention: String| Unread {
            text: type_text(ty),
            place: self.place_of(ty, cx, place),
            convention,
        };
        let resolver = self.resolver;
        match resolver.resolve(cx.module, leading, &segments, Ns::Type) {
            Ok(Res::Item(index)) => {
                let item = &resolver.items[index];
                let mut args: Vec<Option<Arg>> = arguments
                    .iter()
                    .map(|argument| {
                        Some(Arg {
                            ty: (*argument).clone(),
                            cx: cx.clone(),
                        })
                    })
                    .collect();
                match &item.kind {
                    Kind::Struct(_) | Kind::Enum(_) => {
                        args.resize(self.generics(index).len(), None);
                        self.item_reads(index, &args, ty, cx, place, reach);
                    }
                    Kind::Alias(alias) if cx.aliases < ALIAS_DEPTH => {
                        let target = Cx {
                            module: item.module,
                            params: Rc::new(zip(&alias.generics, &args)),
                            self_item: None,
                            owner: Rc::from(self.path_of(index).as_str()),
                            aliases: cx.aliases + 1,
                        };
                        self.reads(&alias.ty, &target, place, reach);
                    }
                    _ => {}
                }
            }
            Ok(Res::Std(named)) => match named.last().map_or("", String::as_str) {
                "Option" | "Vec" | "Box" => {
                    if let Some(first) = first {
                        self.reads(first, cx, place, reach);
                    }
                }
                "VecDeque" => reach.unread.push(unread(list_of(first))),
                "Rc" | "Arc" | "Cow" => reach.unread.push(unread(owned_of(first))),
                "Result" => reach.unread.push(unread(value_of(first))),
                "Infallible" => reach.unread.push(unread(no_value())),
                _ => {}
            },
            Ok(Res::Facade(named)) => {
                if matches!(
                    named.last().map(String::as_str),
                    Some("Promise" | "Repeater" | "Result")
                ) {
                    reach.unread.push(unread(value_of(first)));
                }
            }
            Ok(Res::Prim(name)) if name == "str" || name == "char" => {
                reach.unread.push(unread(String::from("`String`")));
            }
            _ => {}
        }
    }

    /// Adds to `reach` what an argument of the use of the struct or the enum
    /// `index` with the arguments `args`, written as `ty` in `cx` at `place`,
    /// reads into: the use of a struct itself, nothing for an enum of unit
    /// variants, and a type no reader takes for an enum whose variants hold a
    /// value, since an input of GraphQL holds no union.
    fn item_reads(
        &self,
        index: usize,
        args: &[Option<Arg>],
        ty: &Type,
        cx: &Cx,
        place: &Place,
        reach: &mut ArgumentReach,
    ) {
        match &self.resolver.items[index].kind {
            Kind::Struct(_) => reach.reads.push((index, Rc::new(args.to_vec()))),
            Kind::Enum(variants)
                if !variants.variants.is_empty()
                    && variants
                        .variants
                        .iter()
                        .all(|variant| variant.shape != Shape::Unit) =>
            {
                let first = variants
                    .variants
                    .first()
                    .and_then(|variant| variant.fields.first())
                    .map(|field| type_text(&field.ty));
                reach.unread.push(Unread {
                    text: type_text(ty),
                    place: self.place_of(ty, cx, place),
                    convention: payload_of(first.as_deref()),
                });
            }
            _ => {}
        }
    }

    /// Returns the place of the type `ty`, written in `cx`, the line of its
    /// first token in the file of the module that writes it, or `place` for
    /// a type that carries no line.
    fn place_of(&self, ty: &Type, cx: &Cx, place: &Place) -> Place {
        let line = syn::spanned::Spanned::span(ty).start().line;
        if line == 0 {
            return place.clone();
        }
        Place {
            file: self.resolver.modules[cx.module].file.clone(),
            line,
        }
    }

    /// Returns the node of the enum `index`, whose variants are `variants`,
    /// with the arguments `args` lowered to `arg_nodes`.
    fn enumeration(
        &mut self,
        index: usize,
        variants: &Enumeration,
        args: &Rc<Vec<Option<Arg>>>,
        arg_nodes: &[u32],
    ) -> JsValue {
        let resolver = self.resolver;
        let item = &resolver.items[index];
        let path = self.path_of(index);
        let documentation = self.documentation(&path, &item.docs);
        let units = variants
            .variants
            .iter()
            .filter(|variant| variant.shape == Shape::Unit)
            .count();
        if variants.variants.is_empty() {
            self.refuse(item.place.clone(), refuse_empty_enum(&path));
        } else if units > 0 && units < variants.variants.len() {
            self.refuse(item.place.clone(), refuse_mixed_enum(&path));
        }
        let declaration = units == variants.variants.len();
        let cx = Cx {
            module: item.module,
            params: Rc::new(zip(&variants.generics, args)),
            self_item: Some((index, Rc::clone(args))),
            owner: Rc::from(path.as_str()),
            aliases: 0,
        };
        let mut values = Vec::new();
        let mut members = Vec::new();
        let mut member_nodes = Vec::new();
        let mut glue = Vec::new();
        for variant in &variants.variants {
            let key = format!("{path}.{}", variant.name);
            if declaration {
                let documentation = self.documentation(&key, &variant.docs);
                let value = graphql_name(NameKind::Variant, &variant.name);
                glue.push((variant.name.clone(), Some(value.clone())));
                let mut object = JsObject::new();
                object.insert("name", variant.name.as_str());
                object.insert("value", value.as_str());
                object.insert("docs", documentation.comment.as_str());
                object.insert("tags", tags_value(&documentation));
                object.insert("site", site_value(&variant.place, &documentation));
                values.push(JsValue::Object(object));
                continue;
            }
            if variant.shape != Shape::Tuple || variant.fields.len() != 1 {
                if variant.shape != Shape::Unit {
                    self.refuse(variant.place.clone(), refuse_variant(&key));
                }
                continue;
            }
            let site = Site {
                key,
                place: variant.fields[0].place.clone(),
            };
            glue.push((variant.name.clone(), None));
            let member = self.ty(&variant.fields[0].ty, &cx, &site);
            member_nodes.push(member);
            members.push(JsValue::from(member));
        }
        self.plan.enums.push(PlanEnum {
            item: index,
            args: Rc::clone(args),
            variants: glue,
            members: member_nodes,
        });
        let kind = if declaration {
            "enumDeclaration"
        } else {
            "union"
        };
        let rust = instance_text(&path, &self.types, arg_nodes);
        let package = self.package_of(index);
        let mut object = node_named(&rust, kind, &item.name, package, &item.place);
        object.insert("docs", documentation.comment.as_str());
        object.insert("tags", tags_value(&documentation));
        object.insert("site", site_value(&item.place, &documentation));
        object.insert("typename", JsValue::Null);
        object.insert("anonymous", false);
        object.insert("fields", Vec::<JsValue>::new());
        object.insert("methods", Vec::<JsValue>::new());
        object.insert("variants", values);
        object.insert("members", members);
        object.insert("typeArguments", indices(arg_nodes));
        object.insert("heritage", Vec::<JsValue>::new());
        JsValue::Object(object)
    }

    /// Returns the documentation of the item `key` whose doc attributes are
    /// `docs`, refusing a doc text a macro writes and `#[deprecated]`.
    fn documentation(&mut self, key: &str, docs: &Docs) -> Documentation {
        if let Some(place) = &docs.unread {
            self.refuse(place.clone(), refuse_doc_unread(key));
        }
        if let Some(place) = &docs.deprecated {
            self.refuse(place.clone(), refuse_deprecated(key));
        }
        docs::item(docs)
    }

    /// Refuses every second public member of `members` that takes a GraphQL
    /// name another member took, since a type holds each field once.
    fn duplicates(&mut self, members: &[Member]) {
        for (position, member) in members.iter().enumerate() {
            if let Some(first) = members[..position]
                .iter()
                .find(|known| known.name == member.name)
            {
                let message = refuse_member_name(&member.name, first, member);
                self.refuse(member.place.clone(), message);
            }
        }
    }

    /// Returns the message of a name `name` no item of the library `krate`
    /// declares, with what may make it: each glob in scope of a crate whose
    /// names the reader does not hold whole, or a macro call of that crate.
    fn refuse_unknown(
        &self,
        name: &str,
        globs: &[Glob],
        text: &str,
        key: &str,
        krate: usize,
    ) -> String {
        if !globs.is_empty() {
            let named: Vec<String> = globs
                .iter()
                .map(|glob| {
                    let of = match glob.of {
                        GlobOf::Std => "the standard library",
                        GlobOf::Facade => "the facade",
                        GlobOf::Other => "another crate",
                    };
                    format!("`{}` of {of} at {}", glob.path, glob.place)
                })
                .collect();
            return format!(
                "the type `{text}` of `{key}` names `{name}`, which nothing the crate declares holds, and a glob may import it, {}, whose source the builder does not read. Name a type of the crate, or of the standard library or the facade by its path",
                named.join(" or ")
            );
        }
        let calls: Vec<String> = self
            .macros
            .iter()
            .filter(|(_, _, of)| *of == krate)
            .map(|(path, place, _)| format!("{path}! at {place}"))
            .collect();
        if !calls.is_empty() {
            return format!(
                "the type `{text}` of `{key}` names `{name}`, which nothing the crate declares holds, beside the macro calls {}, whose items the builder cannot see. Write the type out in the crate",
                calls.join(", ")
            );
        }
        format!(
            "the type `{text}` of `{key}` names `{name}`, which nothing the crate declares holds. Declare the type in the crate"
        )
    }
}

/// Returns a node of the kind `expr` with the Rust text `rust`, the first two
/// keys of every node of the dump.
fn node(expr: &str, rust: &str) -> JsObject {
    let mut object = JsObject::new();
    object.insert("expr", expr);
    object.insert("rust", rust);
    object
}

/// Returns a named node of the kind `kind` named `name` of the crate
/// `krate` declared at `place`, up to its key `source`.
fn node_named(rust: &str, kind: &str, name: &str, krate: &str, place: &Place) -> JsObject {
    let mut object = node("named", rust);
    object.insert("kind", kind);
    object.insert("name", name);
    object.insert("krate", krate);
    object.insert("source", source_value(place));
    object
}

/// Returns the node of the static descriptor `info`, a scalar marker of the
/// facade or the JSON object of `aosabi-json`, as the derive path dumps it.
fn static_node(info: &TypeInfo) -> JsValue {
    let kind = match info.kind {
        TypeKind::Scalar => "scalar",
        TypeKind::Enum => "enum",
        TypeKind::EnumDeclaration => "enumDeclaration",
        TypeKind::Union => "union",
        _ => "object",
    };
    let mut object = node("named", info.name);
    object.insert("kind", kind);
    object.insert("name", info.name);
    object.insert("krate", info.krate);
    let mut source = JsObject::new();
    source.insert("file", info.source.file);
    source.insert("line", info.source.line);
    source.insert("column", info.source.column);
    object.insert("source", source);
    object.insert("docs", info.docs.comment);
    object.insert("tags", static_tags(&info.docs));
    object.insert("site", static_site(&info.site));
    object.insert(
        "typename",
        info.typename.map_or(JsValue::Null, JsValue::from),
    );
    object.insert("anonymous", info.anonymous);
    for key in [
        "fields",
        "methods",
        "variants",
        "members",
        "typeArguments",
        "heritage",
    ] {
        object.insert(key, Vec::<JsValue>::new());
    }
    JsValue::Object(object)
}

/// Returns the dump of the tags of the documentation `docs` of a descriptor.
fn static_tags(docs: &aosabi_core::descriptor::Docs) -> Vec<JsValue> {
    docs.tags
        .iter()
        .map(|tag| {
            let mut object = JsObject::new();
            object.insert("name", tag.name);
            object.insert("text", tag.text);
            JsValue::Object(object)
        })
        .collect()
}

/// Returns the dump of the site `site` of a descriptor.
fn static_site(site: &aosabi_core::descriptor::Site) -> JsValue {
    let mut object = JsObject::new();
    object.insert("file", site.file);
    object.insert("line", site.line);
    object.insert(
        "docs",
        site.docs
            .iter()
            .map(|line| JsValue::from(*line))
            .collect::<Vec<JsValue>>(),
    );
    object.insert("blocks", site.blocks);
    JsValue::Object(object)
}

/// Returns the node of an object with no name of its own and the fields
/// `fields`, one of the two members of the return type of the wrapper of a
/// mutation, which the reference declares at the column `column` of
/// `mutation.ts:78`, the mapped object at column 3 and the object of
/// `userErrors` at column 53.
fn anonymous_object(rust: &str, name: &str, column: u32, fields: Vec<JsValue>) -> JsValue {
    let mut source = JsObject::new();
    source.insert("file", MUTATION_TS);
    source.insert("line", 78_u32);
    source.insert("column", column);
    let mut object = node("named", rust);
    object.insert("kind", "object");
    object.insert("name", name);
    object.insert("krate", "aosabi-core");
    object.insert("source", source);
    object.insert("docs", "");
    object.insert("tags", Vec::<JsValue>::new());
    object.insert("site", empty_site());
    object.insert("typename", JsValue::Null);
    object.insert("anonymous", true);
    object.insert("fields", fields);
    for key in [
        "methods",
        "variants",
        "members",
        "typeArguments",
        "heritage",
    ] {
        object.insert(key, Vec::<JsValue>::new());
    }
    JsValue::Object(object)
}

/// Returns the dump of a site with no file and no documentation, the site of
/// a member the reference's mapped type of a payload adds.
fn empty_site() -> JsValue {
    let mut object = JsObject::new();
    object.insert("file", "");
    object.insert("line", 0_u32);
    object.insert("docs", Vec::<JsValue>::new());
    object.insert("blocks", 0_u32);
    JsValue::Object(object)
}

/// Returns the Rust text of the instance of `path` with the argument nodes
/// `arg_nodes`, for the dump and a message alone.
fn instance_text(path: &str, types: &[Option<JsValue>], arg_nodes: &[u32]) -> String {
    if arg_nodes.is_empty() {
        return String::from(path);
    }
    let texts: Vec<String> = arg_nodes
        .iter()
        .map(|node| {
            usize::try_from(*node)
                .ok()
                .and_then(|index| types.get(index))
                .and_then(Option::as_ref)
                .and_then(|value| match value {
                    JsValue::Object(object) => match object.get("rust") {
                        Some(JsValue::String(text)) => text.as_str().map(String::from),
                        _ => None,
                    },
                    _ => None,
                })
                .unwrap_or_else(|| String::from("_"))
        })
        .collect();
    format!("{path}<{}>", texts.join(", "))
}

/// Returns the Rust text of a use of the struct at `path` with the arguments
/// `args`, as the source writes them.
fn use_text(path: &str, args: &[Option<Arg>]) -> String {
    let texts: Vec<String> = args
        .iter()
        .map(|arg| {
            arg.as_ref()
                .map_or_else(|| String::from("_"), |arg| type_text(&arg.ty))
        })
        .collect();
    format!("{path}<{}>", texts.join(", "))
}

/// Returns the dump of the source position of an item at `place`.
fn source_value(place: &Place) -> JsValue {
    let mut object = JsObject::new();
    object.insert("file", path_text(&place.file).as_str());
    object.insert("line", u32::try_from(place.line).unwrap_or(u32::MAX));
    object.insert("column", 1_u32);
    JsValue::Object(object)
}

/// Returns the dump of the site of an item at `place` documented by
/// `documentation`.
fn site_value(place: &Place, documentation: &Documentation) -> JsValue {
    let mut object = JsObject::new();
    object.insert("file", path_text(&place.file).as_str());
    object.insert("line", u32::try_from(place.line).unwrap_or(u32::MAX));
    object.insert(
        "docs",
        documentation
            .lines
            .iter()
            .map(|line| JsValue::from(line.as_str()))
            .collect::<Vec<JsValue>>(),
    );
    object.insert(
        "blocks",
        u32::try_from(documentation.blocks).unwrap_or(u32::MAX),
    );
    JsValue::Object(object)
}

/// Returns the dump of the tags of `documentation`.
fn tags_value(documentation: &Documentation) -> Vec<JsValue> {
    documentation
        .tags
        .iter()
        .map(|tag| {
            let mut object = JsObject::new();
            object.insert("name", tag.name.as_str());
            object.insert("text", tag.text().as_str());
            JsValue::Object(object)
        })
        .collect()
}

/// Returns the dump of the public field `name` of the type `ty` at `place`.
fn field_value(name: &str, documentation: &Documentation, place: &Place, ty: u32) -> JsValue {
    let mut object = JsObject::new();
    object.insert("name", name);
    object.insert("docs", documentation.comment.as_str());
    object.insert("tags", tags_value(documentation));
    object.insert("site", site_value(place, documentation));
    object.insert("visibility", "public");
    object.insert("type", ty);
    JsValue::Object(object)
}

/// Returns the dump of the private field `name` at `place`, whose type is the
/// node `any`, since the inference reads that the field is there and no more.
fn private_field(name: &str, place: &Place, any: u32) -> JsValue {
    let mut object = JsObject::new();
    object.insert("name", name);
    object.insert("docs", "");
    object.insert("tags", Vec::<JsValue>::new());
    object.insert("site", site_value(place, &Documentation::default()));
    object.insert("visibility", "private");
    object.insert("type", any);
    JsValue::Object(object)
}

/// Returns the dump of the private method `name` at `place`, which takes
/// nothing and answers the node `any`.
fn private_method(name: &str, place: &Place, any: u32) -> JsValue {
    let mut object = JsObject::new();
    object.insert("name", name);
    object.insert("docs", "");
    object.insert("tags", Vec::<JsValue>::new());
    object.insert("site", site_value(place, &Documentation::default()));
    object.insert("visibility", "private");
    object.insert("isAsync", false);
    object.insert("params", Vec::<JsValue>::new());
    object.insert("returns", any);
    JsValue::Object(object)
}

/// Returns the node positions `nodes` as a JSON array.
fn indices(nodes: &[u32]) -> Vec<JsValue> {
    nodes.iter().map(|node| JsValue::from(*node)).collect()
}

/// Returns `path` with `/` between its parts.
fn path_text(path: &std::path::Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Returns the words that name the receiver form `form` in a refusal.
fn receiver_text(form: Receiver) -> &'static str {
    match form {
        Receiver::Shared => "`&self`",
        Receiver::Unique => "`&mut self`",
        Receiver::Owned => "`self` by value",
        Receiver::Typed => "a receiver of another type",
    }
}

/// The refusal of a public method that takes `self` in another form than
/// `&self`.
fn refuse_receiver(key: &str, form: Receiver) -> String {
    format!(
        "the method `{key}` takes {}, and a field takes `&self`, since the app keeps every value it serves once and lends it to each request. Take `&self`, or keep the method private when it is no field",
        receiver_text(form)
    )
}

/// The refusal of a public method with type parameters.
fn refuse_method_generics(key: &str) -> String {
    format!(
        "the method `{key}` takes type parameters, and a field has one type. Write the concrete types in its signature"
    )
}

/// The refusal of a parameter that is a pattern.
fn refuse_pattern(key: &str) -> String {
    format!(
        "a parameter of `{key}` is a pattern, and the name of an argument is the name of its parameter. Bind the parameter to one name"
    )
}

/// The refusal of a second member of a type with the GraphQL name `name`.
fn refuse_member_name(name: &str, first: &Member, second: &Member) -> String {
    format!(
        "`{}` and `{}` at {} both print the GraphQL name `{name}`, and a type holds each field once. Rename one of them",
        second.key, first.key, first.place
    )
}

/// The refusal of a name two parts of the root `kind` hold.
fn refuse_root_name(kind: RootKind, name: &str, first: &Member, second: &Member) -> String {
    format!(
        "the root `{}` holds the field `{name}` twice, as `{}` at {} and as `{}` at {}, and a root holds each field once. Rename one of them",
        root_name(kind),
        first.key,
        first.place,
        second.key,
        second.place
    )
}

/// The refusal of a root of a generic type.
fn refuse_generic_root(path: &str) -> String {
    format!(
        "the root `{path}` takes type parameters, which a registration does not substitute. Register a struct with no type parameter"
    )
}

/// The refusal of a struct of fields by position.
fn refuse_tuple_struct(path: &str) -> String {
    format!(
        "`{path}` is a struct of fields by position, which has no GraphQL form. Name its fields, or write `aosabi::json::Map` for a JSON object"
    )
}

/// The refusal of an enum that mixes unit variants with variants of a value.
fn refuse_mixed_enum(path: &str) -> String {
    format!(
        "`{path}` mixes variants with no data and variants that hold a value, and a GraphQL enum holds variants with no data alone and a union variants of one type each. Write two enums"
    )
}

/// The refusal of an enum with no variant.
fn refuse_empty_enum(path: &str) -> String {
    format!("`{path}` has no variant, so no value has its type and it has no GraphQL form")
}

/// The refusal of a variant of named fields or of several fields.
fn refuse_variant(key: &str) -> String {
    format!(
        "the variant `{key}` holds named fields or several fields, and a variant of a GraphQL union holds one type. Hold one struct of named fields"
    )
}

/// The refusal of a doc text a macro writes.
fn refuse_doc_unread(key: &str) -> String {
    format!(
        "the doc comment of `{key}` is the value of a macro, which the builder cannot read, and a doc comment is text of the schema. Write the text as `///` lines"
    )
}

/// The refusal of `#[deprecated]` on a described item.
fn refuse_deprecated(key: &str) -> String {
    format!(
        "`#[deprecated]` stands on `{key}` and has no GraphQL form, because Aosabi emits no deprecation. Write `@deprecated` in the doc comment"
    )
}

/// The refusal of a type parameter no concrete use substitutes.
fn refuse_generic(name: &str, owner: &str, key: &str) -> String {
    let site = if owner == key {
        String::new()
    } else {
        format!(" at `{key}`")
    };
    format!(
        "the type parameter `{name}` of `{owner}`{site} is substituted by no concrete use, and a GraphQL type has one shape. Reach the type with a concrete type in place of `{name}`"
    )
}

/// The refusal of a generic type that takes more concrete uses than the
/// builder substitutes.
fn refuse_instantiations(path: &str, key: &str) -> String {
    format!(
        "`{path}` at `{key}` takes more than {INSTANTIATIONS} concrete uses, and the builder stops substituting it. Write a type of its own for some of them"
    )
}

/// The refusal of a trait object or of `impl Trait`.
fn refuse_trait(text: &str, key: &str) -> String {
    format!(
        "the type `{text}` of `{key}` names a trait and no type. Write the concrete type, or an enum of the kinds"
    )
}

/// The refusal of a function that answers `impl Future`.
fn refuse_future(key: &str) -> String {
    format!(
        "the method `{key}` answers `impl Future`, which names a trait and no type. Write it as an `async fn`, whose answer is a promise"
    )
}

/// The refusal of the payload of a mutation over `text`, which is no struct
/// of the crate, at the method `key`.
fn refuse_payload_value(text: &str, key: &str) -> String {
    format!(
        "the payload of the mutation `{key}` holds `{text}`, which is no struct of the crate, while the payload is the public fields of a struct, each nullable, beside `userErrors`, as `mutation(...)` spreads the object its resolver answers. Write `{PAYLOAD}<T>` over a struct of named fields"
    )
}

/// The refusal of the public method `method` of the struct of the payload of
/// the mutation `key`.
fn refuse_payload_method(method: &str, key: &str) -> String {
    format!(
        "the method `{method}` is a public method of the struct of the payload of the mutation `{key}`, which the inferred schema prints in the payload while the spread of `mutation(...)` drops it from the answer. Write the payload as a struct of fields alone, or the method without `pub`"
    )
}

/// The refusal of the return type `text` of the method `key`, which names the
/// type alias `alias` declared at `place` of the payload of a mutation.
fn refuse_payload_alias(alias: &str, place: &Place, text: &str, key: &str) -> String {
    format!(
        "the return type `{text}` of `{key}` names the type alias `{alias}` at {place}, which stands for the payload of a mutation, while the builder reads a payload where the return type of a method writes `{PAYLOAD}<T>` itself and follows no type alias to it. Write `{PAYLOAD}<T>`, or a `Result` of it, as the return type of `{key}` in place of `{alias}`"
    )
}

/// The refusal of the payload of a mutation `text` at `key`, anywhere but the
/// return type of a method.
fn refuse_payload_place(text: &str, key: &str) -> String {
    format!(
        "the type `{text}` of `{key}` is the payload of a mutation, which a method answers as its whole return type alone, as `mutation(...)` wraps a resolver. Write `{PAYLOAD}<T>`, or a `Result` of it, as the return type of the method itself"
    )
}

/// The refusal of an `async fn` that answers a stream.
fn refuse_async_stream(key: &str) -> String {
    format!(
        "the method `{key}` is an `async fn` that answers a stream, whose glue calls the method when the field subscribes and has no promise to await. Write it as a plain `fn` that answers the stream, and await inside the executor of `stream::task`"
    )
}

/// The refusal of a type a macro makes.
fn refuse_macro(text: &str, key: &str) -> String {
    format!(
        "the type `{text}` of `{key}` is one a macro makes, which the builder cannot see. Write the type out where it is used"
    )
}

/// The refusal of an associated type.
fn refuse_associated(text: &str, key: &str) -> String {
    format!(
        "the type `{text}` of `{key}` is an associated type, which the compiler resolves. Write the concrete type in its place"
    )
}

/// The refusal of a chain of type aliases the builder stops following.
fn refuse_alias_chain(text: &str, key: &str) -> String {
    format!(
        "the type of `{key}` leads through more than {ALIAS_DEPTH} type aliases, and the builder stops at the alias `{text}` after it follows {ALIAS_DEPTH} of them. Write the type out where it is used"
    )
}

/// The refusal of a type with no GraphQL form, a tuple, a function pointer,
/// a raw pointer or a type the compiler infers.
fn refuse_shape(ty: &Type, key: &str) -> String {
    let text = type_text(ty);
    let (form, convention) = match ty {
        Type::Tuple(_) => (
            "a tuple",
            "Write `Vec<T>` for a list or a struct of named fields for a record",
        ),
        Type::BareFn(_) => (
            "a function pointer",
            "Write a method of the type, which is a field",
        ),
        Type::Ptr(_) => ("a raw pointer", "Write the type it points at"),
        Type::Infer(_) => ("a type the compiler infers", "Write the type out"),
        _ => ("a form of a type", "Write a struct of named fields"),
    };
    format!("the type `{text}` of `{key}` is {form}, which has no GraphQL form. {convention}")
}

/// The refusal of a Rust union.
fn refuse_union(text: &str, key: &str) -> String {
    format!(
        "the type `{text}` of `{key}` is a union of Rust, which has no GraphQL form. Write an enum whose variants each hold one type"
    )
}

/// The refusal of a type of a crate whose source the builder does not read.
fn refuse_extern(text: &str, named: &[String], key: &str) -> String {
    format!(
        "the type `{text}` of `{key}` is a type of the crate `{}`, which the manifest names with no path, a crate of a registry or of git, whose source the builder does not read. Write a struct of this crate or of a path dependency that states its fields, with `From` from the other type",
        named.first().map_or("", String::as_str)
    )
}

/// The refusal of the use `second` of a generic struct, which prints the
/// GraphQL type of the use `first`, while the impl block of the self type
/// `block` at `place` belongs to one of them alone.
fn refuse_shared_use(first: &str, second: &str, block: &str, place: &Place) -> String {
    format!(
        "the uses `{first}` and `{second}` print one GraphQL type, while the impl block of `{block}` at {place} belongs to one of them alone, and a field belongs to every use of its type. Write the same type argument for both uses, or a struct of its own for one of them"
    )
}

/// The refusal of an inherent impl block whose self type is `text`, of the
/// struct at `path`, whose uses the builder cannot tell for the reason
/// `unmatched`.
fn refuse_block(text: &str, path: &str, unmatched: &Unmatched) -> String {
    let convention = match unmatched {
        Unmatched::Nested(_) => {
            "Write a block over bare type parameters, `impl<T> Page<T>`, or a block for each concrete type, `impl Page<User>`"
        }
        Unmatched::Missing(_) => "Write every type argument of the self type",
        Unmatched::Bound(..) => {
            "Bound the parameter of the struct with the same trait, or write the methods in a block with no bound"
        }
        Unmatched::Other => {
            "Bound a type parameter of the block, or write the methods in a block with no where clause"
        }
    };
    format!(
        "{}, and a field belongs to every use of its type. {convention}",
        unmatched.detail(text, path, "builder")
    )
}

/// The refusal of a type of the standard library with no GraphQL form.
fn refuse_std(text: &str, key: &str) -> String {
    format!(
        "the type `{text}` of `{key}` is a type of the standard library with no GraphQL form. Write `Vec<T>` for a list, a struct of named fields for a record, or `aosabi::json::Map` for a JSON object"
    )
}

/// The refusal of a name of the facade with no GraphQL form.
fn refuse_facade(text: &str, key: &str) -> String {
    format!(
        "the type `{text}` of `{key}` is a name of the facade with no GraphQL form on the source path. Write a struct of named fields, or one of the markers `ID`, `Int`, `Float`, `Number`, `Date`, `File`, `JSON` and `Any`"
    )
}

/// The refusal of the type `unread` of the site `key`, a parameter or a
/// field of a struct the argument of the parameter `via` reaches, which no
/// reader of the engine takes.
fn refuse_unread(unread: &Unread, key: &str, via: &str) -> String {
    let reached = if key == via {
        String::new()
    } else {
        format!(", which the argument of `{via}` reaches,")
    };
    format!(
        "the type `{}` of `{key}`{reached} has no reader of the engine, which reads an argument into a value it owns: a string, a number, a boolean, a marker of the facade, an enum of unit variants, a struct of the crate, or an `Option`, a `Vec` or a `Box` of one. Write {}",
        unread.text, unread.convention
    )
}

/// What to write in place of a borrow, an `Rc`, an `Arc` or a `Cow` of the
/// type `of`: `String` for a string, `Vec<T>` for a slice, and the type or a
/// box of it for any other type.
fn owned_of(of: Option<&Type>) -> String {
    match of {
        Some(Type::Slice(slice)) => list_of(Some(&slice.elem)),
        Some(Type::Paren(inner)) => owned_of(Some(&inner.elem)),
        Some(Type::Group(inner)) => owned_of(Some(&inner.elem)),
        Some(of) if type_text(of) == "str" => String::from("`String`"),
        Some(of) => {
            let text = type_text(of);
            format!("`{text}` or `Box<{text}>`")
        }
        None => String::from("the type it holds"),
    }
}

/// What to write in place of a list of the type `of` that no reader takes.
fn list_of(of: Option<&Type>) -> String {
    of.map_or_else(
        || String::from("a `Vec`"),
        |of| format!("`Vec<{}>`", type_text(of)),
    )
}

/// What to write in place of a `Result`, a promise or a repeater of the type
/// `of`, which an argument holds as its value.
fn value_of(of: Option<&Type>) -> String {
    of.map_or_else(
        || String::from("the type it holds"),
        |of| format!("`{}`, the value an argument holds", type_text(of)),
    )
}

/// What to write in place of a type with no value an argument holds, `()`,
/// `!` or `Infallible`.
fn no_value() -> String {
    String::from(
        "a type that holds a value, or no parameter or field at all, since an argument of it holds nothing",
    )
}

/// What to write in place of an enum whose variants hold a value, whose
/// first variant holds the type `first`.
fn payload_of(first: Option<&str>) -> String {
    let first = first.map_or_else(String::new, |first| format!(" `{first}`"));
    format!(
        "the type of one variant, such as the type{first} of its first, whose input the inference prints in its place, since an input of GraphQL holds no union"
    )
}

/// The refusal of a wrapper written with no type argument.
fn refuse_no_argument(text: &str, key: &str) -> String {
    format!("the type `{text}` of `{key}` wraps a type it does not name. Write the type argument")
}

/// The refusal of a path that names no type.
fn refuse_no_type(text: &str, key: &str) -> String {
    format!("the type `{text}` of `{key}` names no type. Name a struct or an enum")
}
