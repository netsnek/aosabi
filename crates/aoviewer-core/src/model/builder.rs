// SPDX-License-Identifier: MIT
/*
 * aoviewer-core model builder: the builder that fills a model in schema order
 * and resolves names at the end.
 *
 * A field may name a type declared later, as introspection results and the
 * intermediate representation both do, so names resolve when the builder
 * finishes.
 */

//! The model builder.
//!
//! Every loop of the builder runs over an arena or a list whose length
//! [`ReadLimits`] caps when the element is added, over the wrappers of a type
//! reference, which [`ReadLimits::max_depth`] caps, over the locations of one
//! directive, which the constant `MAX_LOCATIONS` caps, or over the three
//! operation roots.

use alloc::{
    boxed::Box,
    format,
    string::String,
    vec::Vec, //
};

use super::{
    arena::{
        Arg,
        ArgId,
        Deprecation,
        Directive,
        DirectiveId,
        EnumValue,
        Field,
        FieldId,
        Kind,
        Model,
        Name,
        Roots,
        Type,
        TypeId,
        TypeRef,
        ValueId, //
    },
    query::RefKind,
};
use crate::limits::ReadLimits;

/// The most locations one directive declares.
///
/// GraphQL defines nineteen directive locations, and the cap leaves room for a
/// later edition of it.
///
/// [specification]: https://spec.graphql.org/October2021/
const MAX_LOCATIONS: usize = 32;

/// A type reference by name, before the builder resolves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeRefSpec {
    /// A named type.
    Named(String),
    /// A list of the inner reference.
    List(Box<TypeRefSpec>),
    /// The inner reference, never null.
    NonNull(Box<TypeRefSpec>),
}

/// A reason the builder refuses a schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelError {
    /// Two elements of one scope share a name, which is written as the path
    /// of the second one, such as `Organization.projects`.
    Duplicate {
        /// The path of the duplicate.
        name: String,
    },
    /// An element names a type that the schema does not declare.
    Dangling {
        /// The path of the element that names it, or `schema` for a root.
        from: String,
        /// The name that resolves to nothing.
        name: String,
    },
    /// A type is named as an interface to implement and is no interface.
    NotAnInterface {
        /// The name of that type.
        name: String,
    },
    /// A type is named as a union member or a root and is no object.
    NotAnObject {
        /// The name of that type.
        name: String,
    },
    /// A name does not match `[_A-Za-z][_0-9A-Za-z]*`.
    InvalidName {
        /// The refused name.
        name: String,
    },
    /// A count or a length exceeds its limit, which is 0 for an element the
    /// kind of its holder does not admit.
    Limit {
        /// What was counted or measured.
        what: &'static str,
        /// The limit.
        limit: u64,
    },
}

/// The holder of an argument while the builder runs.
#[derive(Clone, Copy, Debug)]
enum ArgOwner {
    Field(FieldId),
    Directive(DirectiveId),
}

/// A type as the builder holds it, its members and interfaces by name.
#[derive(Debug)]
struct StageType {
    kind: Kind,
    name: String,
    description: Option<String>,
    fields: Vec<FieldId>,
    values: Vec<ValueId>,
    members: Vec<String>,
    implements: Vec<String>,
    specified_by_url: Option<String>,
}

/// A field as the builder holds it, its type by name.
#[derive(Debug)]
struct StageField {
    owner: TypeId,
    name: String,
    description: Option<String>,
    type_ref: TypeRefSpec,
    args: Vec<ArgId>,
    default_value: Option<String>,
    deprecation: Option<Deprecation>,
}

/// An argument as the builder holds it, its type by name.
#[derive(Debug)]
struct StageArg {
    owner: ArgOwner,
    name: String,
    description: Option<String>,
    type_ref: TypeRefSpec,
    default_value: Option<String>,
    deprecation: Option<Deprecation>,
}

/// An enum value as the builder holds it.
#[derive(Debug)]
struct StageValue {
    owner: TypeId,
    name: String,
    description: Option<String>,
    deprecation: Option<Deprecation>,
}

/// A directive definition as the builder holds it.
#[derive(Debug)]
struct StageDirective {
    name: String,
    description: Option<String>,
    locations: Vec<String>,
    args: Vec<ArgId>,
    repeatable: bool,
}

/// Fills a model in schema order.
///
/// Names resolve at [`Builder::finish`], so a field may name a type declared
/// later, as introspection JSON and the intermediate representation both do.
/// The builder never sorts: schema order is kept where order has meaning, and
/// everything else sorts later by [󰀁](crate::sort::name_order).
///
/// A method without a `Result` records its first defect, and
/// [`Builder::finish`] returns it.
#[derive(Debug)]
pub struct Builder {
    limits: ReadLimits,
    description: Option<String>,
    roots: [Option<String>; 3],
    types: Vec<StageType>,
    fields: Vec<StageField>,
    args: Vec<StageArg>,
    values: Vec<StageValue>,
    directives: Vec<StageDirective>,
    deferred: Option<ModelError>,
}

/// Returns `i` as a 32 bit id, which the limits keep in range.
fn id32(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

/// Returns whether `name` matches `[_A-Za-z][_0-9A-Za-z]*`.
fn is_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b == b'_' || b.is_ascii_alphabetic())
        && bytes.all(|b| b == b'_' || b.is_ascii_alphanumeric())
}

impl Builder {
    /// Returns an empty builder that holds `limits`.
    #[must_use]
    pub fn new(limits: &ReadLimits) -> Self {
        Self {
            limits: *limits,
            description: None,
            roots: [None, None, None],
            types: Vec::new(),
            fields: Vec::new(),
            args: Vec::new(),
            values: Vec::new(),
            directives: Vec::new(),
            deferred: None,
        }
    }

    /// Sets the description of the schema to `text`.
    pub fn schema_description(&mut self, text: &str) {
        match self.text(Some(text), "bytes of a description") {
            Ok(text) => self.description = text,
            Err(e) => self.defer(e),
        }
    }

    /// Names the three operation roots, each by the name of an object type.
    pub fn roots(
        &mut self,
        query: Option<&str>,
        mutation: Option<&str>,
        subscription: Option<&str>,
    ) {
        let names = [query, mutation, subscription];
        for (slot, name) in self.roots.iter_mut().zip(names) {
            *slot = name.map(String::from);
        }
        for name in names.into_iter().flatten() {
            if let Err(e) = self.check_name(name) {
                self.defer(e);
            }
        }
    }

    /// Adds a named type of `kind` after every type added before it.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::InvalidName`] or [`ModelError::Limit`] for a
    /// refused name, description or count. A duplicate name is refused by
    /// [`Builder::finish`].
    pub fn add_type(
        &mut self,
        kind: Kind,
        name: &str,
        description: Option<&str>,
    ) -> Result<TypeId, ModelError> {
        if self.types.len() >= self.limits.max_types as usize {
            return Err(ModelError::Limit {
                what: "types",
                limit: u64::from(self.limits.max_types),
            });
        }
        self.check_name(name)?;
        let description = self.text(description, "bytes of a description")?;
        self.types.push(StageType {
            kind,
            name: String::from(name),
            description,
            fields: Vec::new(),
            values: Vec::new(),
            members: Vec::new(),
            implements: Vec::new(),
            specified_by_url: None,
        });
        Ok(TypeId(id32(self.types.len() - 1)))
    }

    /// Sets the URL the scalar `ty` names as its specification.
    pub fn set_specified_by_url(&mut self, ty: TypeId, url: &str) {
        let what = "bytes of a specifiedByURL";
        let result = self.text(Some(url), what).and_then(|url| {
            let stage = self.stage_type(ty, &[Kind::Scalar], "specifiedByURL of a non scalar")?;
            stage.specified_by_url = url;
            Ok(())
        });
        if let Err(e) = result {
            self.defer(e);
        }
    }

    /// Adds a field, or an input field when `on` is an input type, after the
    /// fields added to `on` before it.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::InvalidName`] or [`ModelError::Limit`] for a
    /// refused name, text, count or wrapper depth, and a limit of 0 when `on`
    /// is neither an object, an interface nor an input type.
    pub fn add_field(
        &mut self,
        on: TypeId,
        name: &str,
        type_ref: &TypeRefSpec,
        description: Option<&str>,
        default_value: Option<&str>,
        deprecation: Option<Deprecation>,
    ) -> Result<FieldId, ModelError> {
        self.check_member(name, type_ref)?;
        let field = StageField {
            owner: on,
            name: String::from(name),
            description: self.text(description, "bytes of a description")?,
            type_ref: type_ref.clone(),
            args: Vec::new(),
            default_value: self.text(default_value, "bytes of a default value")?,
            deprecation: self.deprecation(deprecation)?,
        };
        let id = FieldId(id32(self.fields.len()));
        let kinds = [Kind::Object, Kind::Interface, Kind::Input];
        self.stage_type(on, &kinds, "fields of an enum, a union or a scalar")?
            .fields
            .push(id);
        self.fields.push(field);
        Ok(id)
    }

    /// Adds an argument to the field `on` after the arguments added to it
    /// before.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::InvalidName`] or [`ModelError::Limit`] for a
    /// refused name, text, count or wrapper depth, and a limit of 0 when `on`
    /// is an input field.
    pub fn add_arg(
        &mut self,
        on: FieldId,
        name: &str,
        type_ref: &TypeRefSpec,
        description: Option<&str>,
        default_value: Option<&str>,
        deprecation: Option<Deprecation>,
    ) -> Result<ArgId, ModelError> {
        self.check_member(name, type_ref)?;
        let arg = StageArg {
            owner: ArgOwner::Field(on),
            name: String::from(name),
            description: self.text(description, "bytes of a description")?,
            type_ref: type_ref.clone(),
            default_value: self.text(default_value, "bytes of a default value")?,
            deprecation: self.deprecation(deprecation)?,
        };
        let id = ArgId(id32(self.args.len()));
        let owner = self.stage_field(on)?.owner;
        let kinds = [Kind::Object, Kind::Interface];
        self.stage_type(owner, &kinds, "arguments of an input field")?;
        self.stage_field(on)?.args.push(id);
        self.args.push(arg);
        Ok(id)
    }

    /// Sets the description of the argument `arg`, which the reader of
    /// `ir.json` fills where the served schema carries none.
    ///
    /// The argument is named by the id [`Builder::add_arg`] returned, so a
    /// field of many arguments costs no search for each of them.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::Dangling`] when the builder holds no argument
    /// `arg`, and [`ModelError::Limit`] for a refused text.
    pub fn set_arg_description(&mut self, arg: ArgId, description: &str) -> Result<(), ModelError> {
        let description = self.text(Some(description), "bytes of a description")?;
        let stage = self
            .args
            .get_mut(arg.index())
            .ok_or_else(|| foreign("argument", arg.index()))?;
        stage.description = description;
        Ok(())
    }

    /// Adds a value to the enum `on` after the values added to it before.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::InvalidName`] or [`ModelError::Limit`] for a
    /// refused name, text or count, and a limit of 0 when `on` is no enum.
    pub fn add_value(
        &mut self,
        on: TypeId,
        name: &str,
        description: Option<&str>,
        deprecation: Option<Deprecation>,
    ) -> Result<ValueId, ModelError> {
        self.check_count()?;
        self.check_name(name)?;
        let value = StageValue {
            owner: on,
            name: String::from(name),
            description: self.text(description, "bytes of a description")?,
            deprecation: self.deprecation(deprecation)?,
        };
        let id = ValueId(id32(self.values.len()));
        self.stage_type(on, &[Kind::Enum], "values of a type that is no enum")?
            .values
            .push(id);
        self.values.push(value);
        Ok(id)
    }

    /// Adds the object called `member` to the union `union`.
    ///
    /// A union names each member once, so its list holds at most
    /// [`ReadLimits::max_types`] names, and [`Builder::finish`] refuses a
    /// member named twice.
    pub fn add_member(&mut self, union: TypeId, member: &str) {
        let max = self.limits.max_types;
        let result = self.check_name(member).and_then(|()| {
            let stage =
                self.stage_type(union, &[Kind::Union], "members of a type that is no union")?;
            push_name(&mut stage.members, member, "members of a union", max)
        });
        if let Err(e) = result {
            self.defer(e);
        }
    }

    /// Adds the interface called `interface` to the interfaces `ty`
    /// implements.
    ///
    /// A type names each interface once, so its list holds at most
    /// [`ReadLimits::max_types`] names, and [`Builder::finish`] refuses an
    /// interface named twice.
    pub fn add_implements(&mut self, ty: TypeId, interface: &str) {
        let max = self.limits.max_types;
        let result = self.check_name(interface).and_then(|()| {
            let kinds = [Kind::Object, Kind::Interface];
            let stage = self.stage_type(ty, &kinds, "interfaces of a type that implements none")?;
            push_name(
                &mut stage.implements,
                interface,
                "interfaces of a type",
                max,
            )
        });
        if let Err(e) = result {
            self.defer(e);
        }
    }

    /// Adds a directive definition after the directives added before it.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::InvalidName`] or [`ModelError::Limit`] for a
    /// refused name, location, text or count.
    pub fn add_directive(
        &mut self,
        name: &str,
        description: Option<&str>,
        locations: &[&str],
        repeatable: bool,
    ) -> Result<DirectiveId, ModelError> {
        if self.directives.len() >= self.limits.max_types as usize {
            return Err(ModelError::Limit {
                what: "directives",
                limit: u64::from(self.limits.max_types),
            });
        }
        if locations.len() > MAX_LOCATIONS {
            return Err(ModelError::Limit {
                what: "locations of a directive",
                limit: MAX_LOCATIONS as u64,
            });
        }
        self.check_name(name)?;
        for location in locations {
            self.check_name(location)?;
        }
        self.directives.push(StageDirective {
            name: String::from(name),
            description: self.text(description, "bytes of a description")?,
            locations: locations.iter().copied().map(String::from).collect(),
            args: Vec::new(),
            repeatable,
        });
        Ok(DirectiveId(id32(self.directives.len() - 1)))
    }

    /// Adds an argument to the directive `on` after the arguments added to it
    /// before.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::InvalidName`] or [`ModelError::Limit`] for a
    /// refused name, text, count or wrapper depth, and
    /// [`ModelError::Dangling`] for an id of another builder.
    pub fn add_directive_arg(
        &mut self,
        on: DirectiveId,
        name: &str,
        type_ref: &TypeRefSpec,
        description: Option<&str>,
        default_value: Option<&str>,
    ) -> Result<ArgId, ModelError> {
        self.check_member(name, type_ref)?;
        let arg = StageArg {
            owner: ArgOwner::Directive(on),
            name: String::from(name),
            description: self.text(description, "bytes of a description")?,
            type_ref: type_ref.clone(),
            default_value: self.text(default_value, "bytes of a default value")?,
            deprecation: None,
        };
        let id = ArgId(id32(self.args.len()));
        let Some(directive) = self.directives.get_mut(on.index()) else {
            return Err(foreign("directive", on.index()));
        };
        directive.args.push(id);
        self.args.push(arg);
        Ok(id)
    }

    /// Resolves every name, refuses a dangling or duplicate one, and computes
    /// the implementer and referrer tables once.
    ///
    /// # Errors
    ///
    /// Returns the first defect a method without a `Result` recorded, then
    /// [`ModelError::Duplicate`] for two elements of one scope sharing a name,
    /// [`ModelError::Dangling`] for a name no type carries, and
    /// [`ModelError::NotAnObject`] or [`ModelError::NotAnInterface`] for a
    /// member, a root or an interface of the wrong kind.
    pub fn finish(self) -> Result<Model, ModelError> {
        if let Some(e) = self.deferred {
            return Err(e);
        }
        let names = NameTable::build(&self);
        let by_name = self.types_by_name(&names)?;
        let resolver = Resolver {
            stage: &self.types,
            by_name: &by_name,
        };
        self.check_duplicates(&names)?;
        let types = self.finish_types(&names, &resolver)?;
        let fields = self.finish_fields(&names, &resolver)?;
        let args = self.finish_args(&names, &resolver)?;
        let roots = self.finish_roots(&resolver)?;
        let (implementers, implementer_starts) = implementer_table(&types);
        let (referrers, referrer_starts) = referrer_table(&types, &fields, &args);
        let values = self.finish_values(&names);
        let directives = self.finish_directives(&names);
        let (name_text, name_ends) = names.into_text();
        Ok(Model {
            description: self.description,
            roots,
            order: (0..types.len()).map(|i| TypeId(id32(i))).collect(),
            types,
            fields,
            args,
            values,
            directive_order: (0..directives.len())
                .map(|i| DirectiveId(id32(i)))
                .collect(),
            directives,
            name_text,
            name_ends,
            by_name,
            implementers,
            implementer_starts,
            referrers,
            referrer_starts,
        })
    }

    /// Records `e` unless a defect is recorded already.
    fn defer(&mut self, e: ModelError) {
        if self.deferred.is_none() {
            self.deferred = Some(e);
        }
    }

    /// Checks `name` against the grammar of names and its length limit.
    fn check_name(&self, name: &str) -> Result<(), ModelError> {
        if name.len() > usize::from(self.limits.max_name_bytes) {
            return Err(ModelError::Limit {
                what: "bytes of a name",
                limit: u64::from(self.limits.max_name_bytes),
            });
        }
        if !is_name(name) {
            return Err(ModelError::InvalidName {
                name: String::from(name),
            });
        }
        Ok(())
    }

    /// Checks that one more member fits under the member limit.
    fn check_count(&self) -> Result<(), ModelError> {
        let count = self.fields.len() + self.args.len() + self.values.len();
        if count >= self.limits.max_members as usize {
            return Err(ModelError::Limit {
                what: "fields, arguments and values",
                limit: u64::from(self.limits.max_members),
            });
        }
        Ok(())
    }

    /// Checks the count, the name and the type reference of a new field or
    /// argument.
    fn check_member(&self, name: &str, type_ref: &TypeRefSpec) -> Result<(), ModelError> {
        self.check_count()?;
        self.check_name(name)?;
        let mut cur = type_ref;
        for _ in 0..=self.limits.max_depth {
            match cur {
                TypeRefSpec::Named(name) => return self.check_name(name),
                TypeRefSpec::List(inner) | TypeRefSpec::NonNull(inner) => cur = inner,
            }
        }
        Err(ModelError::Limit {
            what: "wrappers of a type reference",
            limit: u64::from(self.limits.max_depth),
        })
    }

    /// Copies a description, a default value, a deprecation reason or a URL
    /// under its length limit, and names a refused one by `what`.
    fn text(&self, text: Option<&str>, what: &'static str) -> Result<Option<String>, ModelError> {
        match text {
            Some(text) if text.len() > self.limits.max_description_bytes as usize => {
                Err(ModelError::Limit {
                    what,
                    limit: u64::from(self.limits.max_description_bytes),
                })
            }
            _ => Ok(text.map(String::from)),
        }
    }

    /// Checks the reason of a deprecation under its length limit.
    fn deprecation(
        &self,
        deprecation: Option<Deprecation>,
    ) -> Result<Option<Deprecation>, ModelError> {
        if let Some(reason) = deprecation.as_ref().and_then(|d| d.reason.as_deref()) {
            self.text(Some(reason), "bytes of a deprecation reason")?;
        }
        Ok(deprecation)
    }

    /// Returns the type `ty` when its kind is one of `kinds`, and a limit of
    /// 0 on `what` otherwise.
    fn stage_type(
        &mut self,
        ty: TypeId,
        kinds: &[Kind],
        what: &'static str,
    ) -> Result<&mut StageType, ModelError> {
        let stage = self
            .types
            .get_mut(ty.index())
            .ok_or_else(|| foreign("type", ty.index()))?;
        if !kinds.contains(&stage.kind) {
            return Err(ModelError::Limit { what, limit: 0 });
        }
        Ok(stage)
    }

    /// Returns the field `on`.
    fn stage_field(&mut self, on: FieldId) -> Result<&mut StageField, ModelError> {
        self.fields
            .get_mut(on.index())
            .ok_or_else(|| foreign("field", on.index()))
    }

    /// Returns the path `Type.field` of the field `on`.
    fn field_path(&self, on: FieldId) -> String {
        let Some(field) = self.fields.get(on.index()) else {
            return String::from("schema");
        };
        let owner = self
            .types
            .get(field.owner.index())
            .map_or("", |t| t.name.as_str());
        format!("{owner}.{}", field.name)
    }

    /// Returns the path of the argument `id`, `Type.field(arg:)` or
    /// `@directive(arg:)`.
    fn arg_path(&self, id: usize) -> String {
        let Some(arg) = self.args.get(id) else {
            return String::from("schema");
        };
        match arg.owner {
            ArgOwner::Field(f) => format!("{}({}:)", self.field_path(f), arg.name),
            ArgOwner::Directive(d) => {
                let owner = self
                    .directives
                    .get(d.index())
                    .map_or("", |d| d.name.as_str());
                format!("@{owner}({}:)", arg.name)
            }
        }
    }

    /// Returns every type sorted by name, and refuses a duplicate name.
    fn types_by_name(&self, names: &NameTable<'_>) -> Result<Vec<TypeId>, ModelError> {
        let mut by_name: Vec<(Name, TypeId)> = self
            .types
            .iter()
            .enumerate()
            .map(|(i, t)| (names.id(&t.name), TypeId(id32(i))))
            .collect();
        by_name.sort_unstable();
        if let Some(pair) = by_name.windows(2).find(|w| w[0].0 == w[1].0) {
            return Err(ModelError::Duplicate {
                name: self.types[pair[1].1.index()].name.clone(),
            });
        }
        Ok(by_name.into_iter().map(|(_, id)| id).collect())
    }

    /// Refuses two fields of one type, two values of one enum, two arguments
    /// of one field or directive, or two directives sharing a name.
    fn check_duplicates(&self, names: &NameTable<'_>) -> Result<(), ModelError> {
        let mut scratch = Vec::new();
        for t in &self.types {
            let fields = t
                .fields
                .iter()
                .map(|f| self.fields[f.index()].name.as_str());
            if let Some(name) = duplicate(&mut scratch, names, fields) {
                return Err(ModelError::Duplicate {
                    name: format!("{}.{name}", t.name),
                });
            }
            let values = t
                .values
                .iter()
                .map(|v| self.values[v.index()].name.as_str());
            if let Some(name) = duplicate(&mut scratch, names, values) {
                return Err(ModelError::Duplicate {
                    name: format!("{}.{name}", t.name),
                });
            }
        }
        let owners = self.fields.iter().map(|f| &f.args);
        for args in owners.chain(self.directives.iter().map(|d| &d.args)) {
            let texts = args.iter().map(|a| self.args[a.index()].name.as_str());
            if let Some(name) = duplicate(&mut scratch, names, texts) {
                let mut same = args.iter().filter(|a| self.args[a.index()].name == name);
                let second = same.nth(1).map_or(0, |a| a.index());
                return Err(ModelError::Duplicate {
                    name: self.arg_path(second),
                });
            }
        }
        let directives = self.directives.iter().map(|d| d.name.as_str());
        if let Some(name) = duplicate(&mut scratch, names, directives) {
            return Err(ModelError::Duplicate {
                name: format!("@{name}"),
            });
        }
        Ok(())
    }

    /// Resolves the members and interfaces of every type, and refuses one
    /// named twice in one list.
    fn finish_types(
        &self,
        names: &NameTable<'_>,
        resolver: &Resolver<'_>,
    ) -> Result<Vec<Type>, ModelError> {
        let mut types = Vec::with_capacity(self.types.len());
        let mut scratch = Vec::new();
        for t in &self.types {
            let members = resolver.all(&t.name, &t.members, Kind::Object)?;
            let implements = resolver.all(&t.name, &t.implements, Kind::Interface)?;
            for list in [&members, &implements] {
                scratch.clear();
                scratch.extend_from_slice(list);
                scratch.sort_unstable();
                if let Some(pair) = scratch.windows(2).find(|w| w[0] == w[1]) {
                    return Err(ModelError::Duplicate {
                        name: format!("{}.{}", t.name, self.types[pair[0].index()].name),
                    });
                }
            }
            types.push(Type {
                name: names.id(&t.name),
                kind: t.kind,
                description: t.description.clone(),
                fields: t.fields.clone(),
                values: t.values.clone(),
                members,
                implements,
                specified_by_url: t.specified_by_url.clone(),
            });
        }
        Ok(types)
    }

    /// Resolves the type of every field.
    fn finish_fields(
        &self,
        names: &NameTable<'_>,
        resolver: &Resolver<'_>,
    ) -> Result<Vec<Field>, ModelError> {
        let mut fields = Vec::with_capacity(self.fields.len());
        for (i, f) in self.fields.iter().enumerate() {
            let type_ref = resolver
                .type_ref(&f.type_ref)
                .map_err(|name| ModelError::Dangling {
                    from: self.field_path(FieldId(id32(i))),
                    name,
                })?;
            fields.push(Field {
                owner: f.owner,
                name: names.id(&f.name),
                description: f.description.clone(),
                type_ref,
                args: f.args.clone(),
                default_value: f.default_value.clone(),
                deprecation: f.deprecation.clone(),
            });
        }
        Ok(fields)
    }

    /// Resolves the type of every argument.
    fn finish_args(
        &self,
        names: &NameTable<'_>,
        resolver: &Resolver<'_>,
    ) -> Result<Vec<Arg>, ModelError> {
        let mut args = Vec::with_capacity(self.args.len());
        for (i, a) in self.args.iter().enumerate() {
            let type_ref = resolver
                .type_ref(&a.type_ref)
                .map_err(|name| ModelError::Dangling {
                    from: self.arg_path(i),
                    name,
                })?;
            args.push(Arg {
                owner: match a.owner {
                    ArgOwner::Field(f) => f,
                    ArgOwner::Directive(_) => FieldId::DIRECTIVE,
                },
                name: names.id(&a.name),
                description: a.description.clone(),
                type_ref,
                default_value: a.default_value.clone(),
                deprecation: a.deprecation.clone(),
            });
        }
        Ok(args)
    }

    /// Interns the name of every enum value.
    fn finish_values(&self, names: &NameTable<'_>) -> Vec<EnumValue> {
        self.values
            .iter()
            .map(|v| EnumValue {
                owner: v.owner,
                name: names.id(&v.name),
                description: v.description.clone(),
                deprecation: v.deprecation.clone(),
            })
            .collect()
    }

    /// Interns the name of every directive.
    fn finish_directives(&self, names: &NameTable<'_>) -> Vec<Directive> {
        self.directives
            .iter()
            .map(|d| Directive {
                name: names.id(&d.name),
                description: d.description.clone(),
                locations: d.locations.clone(),
                args: d.args.clone(),
                repeatable: d.repeatable,
            })
            .collect()
    }

    /// Resolves the three roots, each of which is an object.
    fn finish_roots(&self, resolver: &Resolver<'_>) -> Result<Roots, ModelError> {
        let mut ids = [None; 3];
        for (id, name) in ids.iter_mut().zip(&self.roots) {
            if let Some(name) = name {
                *id = Some(resolver.one("schema", name, Kind::Object)?);
            }
        }
        let [query, mutation, subscription] = ids;
        Ok(Roots {
            query,
            mutation,
            subscription,
        })
    }
}

/// Appends `name` to a list of type names that holds at most `max` of them.
fn push_name(
    list: &mut Vec<String>,
    name: &str,
    what: &'static str,
    max: u32,
) -> Result<(), ModelError> {
    if list.len() >= max as usize {
        return Err(ModelError::Limit {
            what,
            limit: u64::from(max),
        });
    }
    list.push(String::from(name));
    Ok(())
}

/// Returns the error for an id that another builder handed out.
fn foreign(what: &str, index: usize) -> ModelError {
    ModelError::Dangling {
        from: String::from("builder"),
        name: format!("{what} #{index}"),
    }
}

/// Returns the first name that `texts` carries twice, in the order of
/// `names`.
fn duplicate<'t>(
    scratch: &mut Vec<Name>,
    names: &NameTable<'t>,
    texts: impl Iterator<Item = &'t str>,
) -> Option<&'t str> {
    scratch.clear();
    scratch.extend(texts.map(|t| names.id(t)));
    scratch.sort_unstable();
    let pair = scratch.windows(2).find(|w| w[0] == w[1])?;
    names.table.get(pair[0].index()).copied()
}

/// The names of a builder, unique and sorted by their bytes.
struct NameTable<'b> {
    table: Vec<&'b str>,
}

impl<'b> NameTable<'b> {
    /// Collects the names of every type, field, argument, value and
    /// directive of `builder`.
    fn build(builder: &'b Builder) -> Self {
        let mut table: Vec<&str> = Vec::new();
        table.extend(builder.types.iter().map(|t| t.name.as_str()));
        table.extend(builder.fields.iter().map(|f| f.name.as_str()));
        table.extend(builder.args.iter().map(|a| a.name.as_str()));
        table.extend(builder.values.iter().map(|v| v.name.as_str()));
        table.extend(builder.directives.iter().map(|d| d.name.as_str()));
        table.sort_unstable();
        table.dedup();
        Self { table }
    }

    /// Returns the interned name of `text`, which the table holds.
    fn id(&self, text: &str) -> Name {
        let i = self.table.binary_search(&text).unwrap_or_else(|i| i);
        Name(id32(i))
    }

    /// Returns the table as one text and the end of each name inside it.
    fn into_text(self) -> (String, Vec<usize>) {
        let mut text = String::new();
        let mut ends = Vec::with_capacity(self.table.len());
        for name in self.table {
            text.push_str(name);
            ends.push(text.len());
        }
        (text, ends)
    }
}

/// The lookup of a type by name while the builder finishes.
struct Resolver<'b> {
    stage: &'b [StageType],
    by_name: &'b [TypeId],
}

impl Resolver<'_> {
    /// Returns the type called `name`.
    fn find(&self, name: &str) -> Option<TypeId> {
        self.by_name
            .binary_search_by(|id| self.stage[id.index()].name.as_str().cmp(name))
            .ok()
            .map(|i| self.by_name[i])
    }

    /// Resolves `name`, named by `from`, to a type of `kind`.
    fn one(&self, from: &str, name: &str, kind: Kind) -> Result<TypeId, ModelError> {
        let Some(id) = self.find(name) else {
            return Err(ModelError::Dangling {
                from: String::from(from),
                name: String::from(name),
            });
        };
        if self.stage[id.index()].kind == kind {
            return Ok(id);
        }
        let name = String::from(name);
        Err(match kind {
            Kind::Interface => ModelError::NotAnInterface { name },
            _ => ModelError::NotAnObject { name },
        })
    }

    /// Resolves every name of `list`, named by `from`, to a type of `kind`.
    fn all(&self, from: &str, list: &[String], kind: Kind) -> Result<Vec<TypeId>, ModelError> {
        list.iter().map(|name| self.one(from, name, kind)).collect()
    }

    /// Resolves a type reference, or returns the name that resolves to
    /// nothing.
    fn type_ref(&self, spec: &TypeRefSpec) -> Result<TypeRef, String> {
        // The builder checked the depth when the reference was added, so the
        // walk ends at the named type within `ReadLimits::max_depth` steps.
        let mut lists = Vec::new();
        let mut cur = spec;
        let name = loop {
            match cur {
                TypeRefSpec::Named(name) => break name,
                TypeRefSpec::List(inner) => {
                    lists.push(true);
                    cur = inner;
                }
                TypeRefSpec::NonNull(inner) => {
                    lists.push(false);
                    cur = inner;
                }
            }
        };
        let id = self.find(name).ok_or_else(|| name.clone())?;
        let mut type_ref = TypeRef::Named(id);
        for &list in lists.iter().rev() {
            type_ref = if list {
                TypeRef::List(Box::new(type_ref))
            } else {
                TypeRef::NonNull(Box::new(type_ref))
            };
        }
        Ok(type_ref)
    }
}

/// Groups `pairs` of a type and an entry by the type, keeping their order,
/// and returns the entries with the start of each type's run.
fn group<T: Copy>(count: usize, pairs: &[(TypeId, T)]) -> (Vec<T>, Vec<usize>) {
    let mut starts = alloc::vec![0; count + 1];
    for (ty, _) in pairs {
        starts[ty.index() + 1] += 1;
    }
    for i in 0..count {
        starts[i + 1] += starts[i];
    }
    let mut next = starts.clone();
    let mut out = Vec::with_capacity(pairs.len());
    out.extend(pairs.iter().map(|&(_, entry)| entry));
    for &(ty, entry) in pairs {
        out[next[ty.index()]] = entry;
        next[ty.index()] += 1;
    }
    (out, starts)
}

/// Computes the implementers of every interface in schema order.
fn implementer_table(types: &[Type]) -> (Vec<TypeId>, Vec<usize>) {
    let mut pairs = Vec::new();
    for (i, t) in types.iter().enumerate() {
        pairs.extend(t.implements.iter().map(|&iface| (iface, TypeId(id32(i)))));
    }
    group(types.len(), &pairs)
}

/// Computes every field and argument naming each type, in the order of the
/// fields of the types in schema order.
fn referrer_table(
    types: &[Type],
    fields: &[Field],
    args: &[Arg],
) -> (Vec<(FieldId, RefKind)>, Vec<usize>) {
    let mut pairs: Vec<(TypeId, (FieldId, RefKind))> = Vec::new();
    let mut targets = Vec::new();
    for t in types {
        let kind = if t.kind == Kind::Input {
            RefKind::InputField
        } else {
            RefKind::Field
        };
        for &f in &t.fields {
            let field = &fields[f.index()];
            pairs.push((field.type_ref.named(), (f, kind)));
            // Each target of the arguments lands in a run of its own, so the
            // targets are deduplicated in any order and the field is named
            // once per run.
            targets.clear();
            targets.extend(field.args.iter().map(|a| args[a.index()].type_ref.named()));
            targets.sort_unstable();
            targets.dedup();
            pairs.extend(targets.iter().map(|&t| (t, (f, RefKind::Argument))));
        }
    }
    group(types.len(), &pairs)
}
