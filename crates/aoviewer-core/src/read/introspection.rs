// SPDX-License-Identifier: MIT
/*
 * aoviewer-core read introspection: the reader of an introspection result.
 *
 * It reads the answer of the introspection query with or without its data
 * envelope.
 */

//! The introspection reader.
//!
//! The reader keeps what `buildClientSchema` of graphql-js reads of each kind
//! of type and passes over the rest, so the fields of an object, the input
//! fields of an input type, the values of an enum and the possible types of a
//! union reach the model, and the possible types of an interface, which the
//! model computes from the `implements` lists, do not. A list that is `null`
//! or absent holds nothing.

use alloc::{
    borrow::Cow,
    boxed::Box,
    string::String,
    vec::Vec, //
};

use super::{
    Cursor,
    ReadError, //
};
use crate::{
    limits::ReadLimits,
    model::{
        Builder,
        Deprecation,
        Kind,
        Model,
        TypeRefSpec, //
    },
};

/// A named type as the reader holds it until its object ends, because the
/// keys of an object may stand in any order.
#[derive(Debug, Default)]
struct RawType<'a> {
    kind: Option<Kind>,
    name: Option<Cow<'a, str>>,
    description: Option<Cow<'a, str>>,
    specified_by_url: Option<Cow<'a, str>>,
    fields: Vec<RawField<'a>>,
    input_fields: Vec<RawValue<'a>>,
    interfaces: Vec<Cow<'a, str>>,
    values: Vec<RawEnumValue<'a>>,
    possible_types: Vec<Cow<'a, str>>,
}

/// A field of an object or an interface.
#[derive(Debug)]
struct RawField<'a> {
    name: Cow<'a, str>,
    description: Option<Cow<'a, str>>,
    args: Vec<RawValue<'a>>,
    type_ref: TypeRefSpec,
    deprecation: Option<Deprecation>,
}

/// An input value, which is an argument or an input field.
#[derive(Debug)]
struct RawValue<'a> {
    name: Cow<'a, str>,
    description: Option<Cow<'a, str>>,
    type_ref: TypeRefSpec,
    default_value: Option<Cow<'a, str>>,
    deprecation: Option<Deprecation>,
}

/// A value of an enum.
#[derive(Debug)]
struct RawEnumValue<'a> {
    name: Cow<'a, str>,
    description: Option<Cow<'a, str>>,
    deprecation: Option<Deprecation>,
}

/// Reads an introspection result, `{"data":{"__schema":..}}` or
/// `{"__schema":..}`.
///
/// The types keep the order of the `types` list, which is schema order, and
/// the directives the order of the `directives` list. A member is deprecated
/// when `isDeprecated` is `true` or `deprecationReason` is a string, which
/// agrees with graphql-js on every answer it writes itself, as it reads the
/// reason alone. The deprecation of a directive argument is read and dropped,
/// because [`Builder::add_directive_arg`] takes none.
///
/// # Errors
///
/// Returns [`ReadError::Json`] for bytes that are no JSON document within
/// `limits`, [`ReadError::Shape`] for a document without a `__schema` or with
/// a value of the wrong kind, and [`ReadError::Model`] for a schema the
/// builder refuses or a count past `limits`.
pub fn from_introspection(bytes: &[u8], limits: &ReadLimits) -> Result<Model, ReadError> {
    let mut c = Cursor::new(bytes, limits);
    let mut b = Builder::new(limits);
    let mut schemas = 0u32;
    c.object()?;
    let mut seen = 0;
    while let Some(key) = c.key()? {
        match key.as_ref() {
            "data" => c.member("data", 1 << 0, &mut seen, |c| data(c, &mut b, &mut schemas))?,
            "__schema" => c.member("__schema", 1 << 1, &mut seen, |c| {
                one_schema(c, &mut b, &mut schemas)
            })?,
            _ => c.skip()?,
        }
    }
    c.end()?;
    if schemas == 0 {
        return Err(c.shape("an object with data or __schema"));
    }
    Ok(b.finish()?)
}

/// Reads the `data` member of an execution result.
fn data(c: &mut Cursor<'_>, b: &mut Builder, schemas: &mut u32) -> Result<(), ReadError> {
    c.object()?;
    let before = *schemas;
    let mut seen = 0;
    while let Some(key) = c.key()? {
        match key.as_ref() {
            "__schema" => c.member("__schema", 1 << 0, &mut seen, |c| one_schema(c, b, schemas))?,
            _ => c.skip()?,
        }
    }
    if *schemas == before {
        return Err(c.shape("an object with __schema"));
    }
    Ok(())
}

/// Reads a `__schema` object, refusing a second one in the document.
fn one_schema(c: &mut Cursor<'_>, b: &mut Builder, schemas: &mut u32) -> Result<(), ReadError> {
    if *schemas > 0 {
        return Err(c.shape("one __schema in the document"));
    }
    *schemas += 1;
    schema(c, b)
}

/// Reads the `__schema` object into `b`.
fn schema(c: &mut Cursor<'_>, b: &mut Builder) -> Result<(), ReadError> {
    c.object()?;
    let mut roots: [Option<Cow<'_, str>>; 3] = [None, None, None];
    let mut seen = 0;
    while let Some(key) = c.key()? {
        match key.as_ref() {
            "description" => {
                let text = c.member("description", 1 << 0, &mut seen, Cursor::string_or_null)?;
                if let Some(text) = text {
                    b.schema_description(&text);
                }
            }
            "queryType" => roots[0] = c.member("queryType", 1 << 1, &mut seen, root)?,
            "mutationType" => roots[1] = c.member("mutationType", 1 << 2, &mut seen, root)?,
            "subscriptionType" => {
                roots[2] = c.member("subscriptionType", 1 << 3, &mut seen, root)?;
            }
            "types" => c.member("types", 1 << 4, &mut seen, |c| c.list(|c| ty(c, b)))?,
            "directives" => c.member("directives", 1 << 5, &mut seen, |c| {
                c.list_or_null(|c| directive(c, b))
            })?,
            _ => c.skip()?,
        }
    }
    let [query, mutation, subscription] = &roots;
    b.roots(
        query.as_deref(),
        mutation.as_deref(),
        subscription.as_deref(),
    );
    Ok(())
}

/// Reads a root reference, `{"name":..}` or `null`.
fn root<'a>(c: &mut Cursor<'a>) -> Result<Option<Cow<'a, str>>, ReadError> {
    if !c.object_or_null()? {
        return Ok(None);
    }
    named_body(c).map(Some)
}

/// Reads a reference to a named type, as `interfaces` and `possibleTypes`
/// list them, and returns the name.
fn named<'a>(c: &mut Cursor<'a>) -> Result<Cow<'a, str>, ReadError> {
    c.object()?;
    named_body(c)
}

/// Reads the members of an open object that names a type and returns the
/// name.
fn named_body<'a>(c: &mut Cursor<'a>) -> Result<Cow<'a, str>, ReadError> {
    let mut name = None;
    let mut seen = 0;
    while let Some(key) = c.key()? {
        match key.as_ref() {
            "name" => name = c.member("name", 1 << 0, &mut seen, Cursor::string_or_null)?,
            _ => c.skip()?,
        }
    }
    name.ok_or_else(|| c.shape("a reference with a name"))
}

/// Reads one element of `types` and adds it to `b`.
fn ty(c: &mut Cursor<'_>, b: &mut Builder) -> Result<(), ReadError> {
    let raw = raw_type(c)?;
    let kind = raw.kind.ok_or_else(|| c.shape("a type with a kind"))?;
    let name = raw.name.ok_or_else(|| c.shape("a type with a name"))?;
    let id = b.add_type(kind, &name, raw.description.as_deref())?;
    match kind {
        Kind::Object | Kind::Interface => {
            for f in raw.fields {
                let field = b.add_field(
                    id,
                    &f.name,
                    &f.type_ref,
                    f.description.as_deref(),
                    None,
                    f.deprecation,
                )?;
                for a in f.args {
                    b.add_arg(
                        field,
                        &a.name,
                        &a.type_ref,
                        a.description.as_deref(),
                        a.default_value.as_deref(),
                        a.deprecation,
                    )?;
                }
            }
            for interface in &raw.interfaces {
                b.add_implements(id, interface);
            }
        }
        Kind::Input => {
            for v in raw.input_fields {
                b.add_field(
                    id,
                    &v.name,
                    &v.type_ref,
                    v.description.as_deref(),
                    v.default_value.as_deref(),
                    v.deprecation,
                )?;
            }
        }
        Kind::Enum => {
            for v in raw.values {
                b.add_value(id, &v.name, v.description.as_deref(), v.deprecation)?;
            }
        }
        Kind::Union => {
            for member in &raw.possible_types {
                b.add_member(id, member);
            }
        }
        Kind::Scalar => {
            if let Some(url) = &raw.specified_by_url {
                b.set_specified_by_url(id, url);
            }
        }
    }
    Ok(())
}

/// Reads the object of one named type.
fn raw_type<'a>(c: &mut Cursor<'a>) -> Result<RawType<'a>, ReadError> {
    c.object()?;
    let mut t = RawType::default();
    let mut seen = 0;
    while let Some(key) = c.key()? {
        match key.as_ref() {
            "kind" => t.kind = Some(c.member("kind", 1 << 0, &mut seen, named_kind)?),
            "name" => t.name = Some(c.member("name", 1 << 1, &mut seen, Cursor::string)?),
            "description" => {
                t.description =
                    c.member("description", 1 << 2, &mut seen, Cursor::string_or_null)?;
            }
            // A server of graphql-js 15.1 or a later 15 release spells the
            // field `specifiedByUrl`, which graphql-js 16.0.0 renamed to
            // `specifiedByURL`, and a result carries one of the two.
            "specifiedByURL" => {
                t.specified_by_url =
                    c.member("specifiedByURL", 1 << 3, &mut seen, Cursor::string_or_null)?;
            }
            "specifiedByUrl" => {
                t.specified_by_url =
                    c.member("specifiedByUrl", 1 << 3, &mut seen, Cursor::string_or_null)?;
            }
            "fields" => c.member("fields", 1 << 4, &mut seen, |c| {
                c.list_or_null(|c| {
                    c.count_member()?;
                    t.fields.push(field(c)?);
                    Ok(())
                })
            })?,
            "inputFields" => c.member("inputFields", 1 << 5, &mut seen, |c| {
                c.list_or_null(|c| {
                    c.count_member()?;
                    t.input_fields.push(input_value(c)?);
                    Ok(())
                })
            })?,
            "interfaces" => c.member("interfaces", 1 << 6, &mut seen, |c| {
                c.list_or_null(|c| {
                    c.count_name(t.interfaces.len(), "interfaces of a type")?;
                    t.interfaces.push(named(c)?);
                    Ok(())
                })
            })?,
            "enumValues" => c.member("enumValues", 1 << 7, &mut seen, |c| {
                c.list_or_null(|c| {
                    c.count_member()?;
                    t.values.push(enum_value(c)?);
                    Ok(())
                })
            })?,
            "possibleTypes" => c.member("possibleTypes", 1 << 8, &mut seen, |c| {
                c.list_or_null(|c| {
                    c.count_name(t.possible_types.len(), "members of a union")?;
                    t.possible_types.push(named(c)?);
                    Ok(())
                })
            })?,
            _ => c.skip()?,
        }
    }
    Ok(t)
}

/// Reads the kind of a named type.
fn named_kind(c: &mut Cursor<'_>) -> Result<Kind, ReadError> {
    let text = c.string()?;
    match text.as_ref() {
        "OBJECT" => Ok(Kind::Object),
        "INTERFACE" => Ok(Kind::Interface),
        "UNION" => Ok(Kind::Union),
        "ENUM" => Ok(Kind::Enum),
        "INPUT_OBJECT" => Ok(Kind::Input),
        "SCALAR" => Ok(Kind::Scalar),
        _ => Err(c.shape("the kind of a named type")),
    }
}

/// Reads a field of an object or an interface.
fn field<'a>(c: &mut Cursor<'a>) -> Result<RawField<'a>, ReadError> {
    c.object()?;
    let mut name = None;
    let mut description = None;
    let mut args = Vec::new();
    let mut type_ref = None;
    let mut is_deprecated = None;
    let mut reason = None;
    let mut seen = 0;
    while let Some(key) = c.key()? {
        match key.as_ref() {
            "name" => name = Some(c.member("name", 1 << 0, &mut seen, Cursor::string)?),
            "description" => {
                description = c.member("description", 1 << 1, &mut seen, Cursor::string_or_null)?;
            }
            "args" => c.member("args", 1 << 2, &mut seen, |c| {
                c.list_or_null(|c| {
                    c.count_member()?;
                    args.push(input_value(c)?);
                    Ok(())
                })
            })?,
            "type" => type_ref = Some(c.member("type", 1 << 3, &mut seen, type_ref_of)?),
            "isDeprecated" => {
                is_deprecated =
                    c.member("isDeprecated", 1 << 4, &mut seen, Cursor::boolean_or_null)?;
            }
            "deprecationReason" => {
                reason = c.member(
                    "deprecationReason",
                    1 << 5,
                    &mut seen,
                    Cursor::string_or_null,
                )?;
            }
            _ => c.skip()?,
        }
    }
    Ok(RawField {
        name: name.ok_or_else(|| c.shape("a field with a name"))?,
        description,
        args,
        type_ref: type_ref.ok_or_else(|| c.shape("a field with a type"))?,
        deprecation: deprecation(is_deprecated, reason),
    })
}

/// Reads an input value, which is an argument or an input field.
fn input_value<'a>(c: &mut Cursor<'a>) -> Result<RawValue<'a>, ReadError> {
    c.object()?;
    let mut name = None;
    let mut description = None;
    let mut type_ref = None;
    let mut default_value = None;
    let mut is_deprecated = None;
    let mut reason = None;
    let mut seen = 0;
    while let Some(key) = c.key()? {
        match key.as_ref() {
            "name" => name = Some(c.member("name", 1 << 0, &mut seen, Cursor::string)?),
            "description" => {
                description = c.member("description", 1 << 1, &mut seen, Cursor::string_or_null)?;
            }
            "type" => type_ref = Some(c.member("type", 1 << 2, &mut seen, type_ref_of)?),
            "defaultValue" => {
                default_value =
                    c.member("defaultValue", 1 << 3, &mut seen, Cursor::string_or_null)?;
            }
            "isDeprecated" => {
                is_deprecated =
                    c.member("isDeprecated", 1 << 4, &mut seen, Cursor::boolean_or_null)?;
            }
            "deprecationReason" => {
                reason = c.member(
                    "deprecationReason",
                    1 << 5,
                    &mut seen,
                    Cursor::string_or_null,
                )?;
            }
            _ => c.skip()?,
        }
    }
    Ok(RawValue {
        name: name.ok_or_else(|| c.shape("an input value with a name"))?,
        description,
        type_ref: type_ref.ok_or_else(|| c.shape("an input value with a type"))?,
        default_value,
        deprecation: deprecation(is_deprecated, reason),
    })
}

/// Reads a value of an enum.
fn enum_value<'a>(c: &mut Cursor<'a>) -> Result<RawEnumValue<'a>, ReadError> {
    c.object()?;
    let mut name = None;
    let mut description = None;
    let mut is_deprecated = None;
    let mut reason = None;
    let mut seen = 0;
    while let Some(key) = c.key()? {
        match key.as_ref() {
            "name" => name = Some(c.member("name", 1 << 0, &mut seen, Cursor::string)?),
            "description" => {
                description = c.member("description", 1 << 1, &mut seen, Cursor::string_or_null)?;
            }
            "isDeprecated" => {
                is_deprecated =
                    c.member("isDeprecated", 1 << 2, &mut seen, Cursor::boolean_or_null)?;
            }
            "deprecationReason" => {
                reason = c.member(
                    "deprecationReason",
                    1 << 3,
                    &mut seen,
                    Cursor::string_or_null,
                )?;
            }
            _ => c.skip()?,
        }
    }
    Ok(RawEnumValue {
        name: name.ok_or_else(|| c.shape("an enum value with a name"))?,
        description,
        deprecation: deprecation(is_deprecated, reason),
    })
}

/// Returns the deprecation that `isDeprecated` and `deprecationReason` state
/// together.
fn deprecation(is_deprecated: Option<bool>, reason: Option<Cow<'_, str>>) -> Option<Deprecation> {
    (is_deprecated == Some(true) || reason.is_some()).then(|| Deprecation {
        reason: reason.map(Cow::into_owned),
    })
}

/// Reads one directive definition and adds it to `b`.
fn directive(c: &mut Cursor<'_>, b: &mut Builder) -> Result<(), ReadError> {
    c.object()?;
    let mut name = None;
    let mut description = None;
    let mut repeatable = None;
    let mut locations: Vec<Cow<'_, str>> = Vec::new();
    let mut args = Vec::new();
    let mut seen = 0;
    while let Some(key) = c.key()? {
        match key.as_ref() {
            "name" => name = Some(c.member("name", 1 << 0, &mut seen, Cursor::string)?),
            "description" => {
                description = c.member("description", 1 << 1, &mut seen, Cursor::string_or_null)?;
            }
            "isRepeatable" => {
                repeatable =
                    c.member("isRepeatable", 1 << 2, &mut seen, Cursor::boolean_or_null)?;
            }
            "locations" => c.member("locations", 1 << 3, &mut seen, |c| {
                c.list_or_null(|c| {
                    c.count_name(locations.len(), "locations of a directive")?;
                    locations.push(c.string()?);
                    Ok(())
                })
            })?,
            "args" => c.member("args", 1 << 4, &mut seen, |c| {
                c.list_or_null(|c| {
                    c.count_member()?;
                    args.push(input_value(c)?);
                    Ok(())
                })
            })?,
            _ => c.skip()?,
        }
    }
    let name = name.ok_or_else(|| c.shape("a directive with a name"))?;
    let locations: Vec<&str> = locations.iter().map(AsRef::as_ref).collect();
    let id = b.add_directive(
        &name,
        description.as_deref(),
        &locations,
        repeatable == Some(true),
    )?;
    for a in args {
        b.add_directive_arg(
            id,
            &a.name,
            &a.type_ref,
            a.description.as_deref(),
            a.default_value.as_deref(),
        )?;
    }
    Ok(())
}

/// One object of a type reference while the reader is inside it.
#[derive(Debug, Default)]
struct RefLevel<'a> {
    kind: Option<Cow<'a, str>>,
    name: Option<Cow<'a, str>>,
    of_type: Option<TypeRefSpec>,
    seen: u32,
}

/// Reads a type reference, `{"kind":..,"name":..,"ofType":..}` nested once
/// per wrapper.
///
/// The objects nest as deep as the wrappers do, and the parser caps that
/// nesting at [`ReadLimits::max_depth`], so the reader holds one level per
/// open object and needs no recursion.
fn type_ref_of(c: &mut Cursor<'_>) -> Result<TypeRefSpec, ReadError> {
    c.object()?;
    let mut levels = alloc::vec![RefLevel::default()];
    loop {
        let Some(level) = levels.last_mut() else {
            return Err(c.shape("a type reference"));
        };
        if let Some(key) = c.key()? {
            match key.as_ref() {
                "kind" => {
                    level.kind = Some(c.member("kind", 1 << 0, &mut level.seen, Cursor::string)?);
                }
                "name" => {
                    level.name =
                        c.member("name", 1 << 1, &mut level.seen, Cursor::string_or_null)?;
                }
                "ofType" => {
                    c.enter("ofType");
                    if level.seen & 1 << 2 != 0 {
                        return Err(c.shape("a key given once"));
                    }
                    level.seen |= 1 << 2;
                    if c.object_or_null()? {
                        levels.push(RefLevel::default());
                    } else {
                        c.leave();
                    }
                }
                _ => c.skip()?,
            }
            continue;
        }
        let level = levels.pop().unwrap_or_default();
        let spec = ref_level(c, level)?;
        match levels.last_mut() {
            Some(outer) => {
                c.leave();
                outer.of_type = Some(spec);
            }
            None => return Ok(spec),
        }
    }
}

/// Returns the reference one finished object of a type reference states.
///
/// A wrapper takes its inner reference from `ofType`, and a named type takes
/// its name and ignores `ofType`, as `buildClientSchema` reads it. A non null
/// wrapper around another is refused, as `assertNullableType` refuses it
/// there.
fn ref_level(c: &Cursor<'_>, level: RefLevel<'_>) -> Result<TypeRefSpec, ReadError> {
    let Some(kind) = level.kind else {
        return Err(c.shape("a type reference with a kind"));
    };
    match kind.as_ref() {
        "NON_NULL" => match level.of_type {
            Some(TypeRefSpec::NonNull(_)) => Err(c.shape("a nullable type inside NON_NULL")),
            Some(inner) => Ok(TypeRefSpec::NonNull(Box::new(inner))),
            None => Err(c.shape("a wrapper with an ofType")),
        },
        "LIST" => match level.of_type {
            Some(inner) => Ok(TypeRefSpec::List(Box::new(inner))),
            None => Err(c.shape("a wrapper with an ofType")),
        },
        "OBJECT" | "INTERFACE" | "UNION" | "ENUM" | "INPUT_OBJECT" | "SCALAR" => match level.name {
            Some(name) => Ok(TypeRefSpec::Named(String::from(name))),
            None => Err(c.shape("a named type with a name")),
        },
        _ => Err(c.shape("the kind of a type reference")),
    }
}
