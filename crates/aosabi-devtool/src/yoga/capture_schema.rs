// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool yoga: the two schemas of the in process capture and their
 * resolvers.
 *
 * Records the rows C04 to C10 and C12 to C19, and, which records the rows C23
 * to C25 and C37 to C39. Each script builds its schema with createSchema of
 * Yoga, the second with the scalars aosabi-core binds.
 */

//! The capture schemas.
//!
//! The first schema answers the requests of the five instances of the first
//! half, and its resolvers throw the values the rows ask about: a plain
//! `Error`, a string, a `ServiceError` with its code and status, a
//! `GraphQLError` with `extensions.http`, and a bare `GraphQLError`. The
//! second schema binds `Date`, `JSON`, `JSONObject` and `Void` as aosabi-core
//! binds them, and answers the rows of the second half.

use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_core::{
    ResolverMap,
    make_executable_schema, //
    scalars::{
        date,
        json,
        json_object,
        void, //
    },
};
use aosabi_execute::{
    Completion,
    ErrorObject,
    FieldResolverFn,
    ObjectLike as _,
    Value,
    value::HostValue, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions, //
};
use aosabi_scalars::JsDate;
use aosabi_type::{
    ObjMap,
    Schema, //
};
use graphql_over_http::{
    AgentSetup,
    FileBody, //
};

/// The type definitions of the first half, `first-half-in-process.mjs:61-78`.
pub const FIRST_TYPE_DEFS: &str = "
  scalar Upload

  type Query {
    hello: String!
    other: String!
    nullable: String
    thrownError: String
    thrownString: String
    serviceError: String
    httpStatus(status: Int!): String
    bareGraphQLError: String
    echo(value: String!): String!
    upload(file: Upload!): String!
  }

  type Mutation {
    touch: String!
  }
";

/// The type definitions of the second half, `second-half-in-process.mjs:86-104`.
pub const SECOND_TYPE_DEFS: &str = "
  scalar Date
  scalar JSON
  scalar JSONObject
  scalar Void

  type Query {
    hello: String!
    echoDate(value: Date!): Date!
    echoJson(value: JSON!): JSON!
    echoJsonObject(value: JSONObject!): JSONObject!
    echoVoid(value: Void): Void
    stamp(year: Int!): Date!
  }

  type Mutation {
    touch: String!
  }
";

/// Returns the schema of the first half, built by `makeExecutableSchema`
/// with no scalar bound, so `Upload` parses and serializes a value as it is,
/// as the scalar of the script does.
///
/// # Errors
///
/// Returns the message of the build, which the SDL above never meets.
pub fn first_schema() -> Result<Arc<Schema>, String> {
    make_executable_schema(FIRST_TYPE_DEFS, &ResolverMap::default())
        .map_err(|error| String::from_utf8_lossy(error.message().as_wtf8()).into_owned())
}

/// Returns the schema of the second half, with `Date`, `JSON`, `JSONObject`
/// and `Void` bound as aosabi-core binds them, the four the script binds.
///
/// # Errors
///
/// Returns the message of the build, which the SDL above never meets.
pub fn second_schema() -> Result<Arc<Schema>, String> {
    let mut resolvers = ResolverMap::default();
    resolvers
        .scalars
        .insert(date::NAME.to_owned(), date::binding());
    resolvers
        .scalars
        .insert(json::NAME.to_owned(), json::binding());
    resolvers
        .scalars
        .insert(json_object::NAME.to_owned(), json_object::binding());
    resolvers
        .scalars
        .insert(void::NAME.to_owned(), void::binding());
    make_executable_schema(SECOND_TYPE_DEFS, &resolvers)
        .map_err(|error| String::from_utf8_lossy(error.message().as_wtf8()).into_owned())
}

/// Returns the agent of the first half, whose field resolver is the resolver
/// map of `first-half-in-process.mjs:80-110`.
pub fn first_agent() -> AgentSetup {
    AgentSetup {
        field_resolver: Some(first_resolver()),
        ..AgentSetup::default()
    }
}

/// Returns the agent of the second half, whose field resolver is the resolver
/// map of `second-half-in-process.mjs:106-122` and whose leaf serializers
/// write a `Date` as aosabi-core writes it.
pub fn second_agent() -> AgentSetup {
    AgentSetup {
        field_resolver: Some(second_resolver()),
        leaf_serializers: Some(Arc::new(second_leaf_serializers())),
        ..AgentSetup::default()
    }
}

/// Returns the leaf serializers of the four scalars of the second half.
fn second_leaf_serializers() -> ObjMap<aosabi_execute::LeafSerializer> {
    let mut serializers = ObjMap::default();
    serializers.insert(date::NAME.to_owned(), date::leaf_serializer());
    serializers.insert(json::NAME.to_owned(), json::leaf_serializer());
    serializers.insert(json_object::NAME.to_owned(), json_object::leaf_serializer());
    serializers.insert(void::NAME.to_owned(), void::leaf_serializer());
    serializers
}

/// Returns a `GraphQLError` with `message` and `extensions` as a thrown value.
fn graphql_error(message: &str, extensions: Option<JsObject>) -> Value {
    Value::from(ErrorObject::GraphQL(GraphQLError::new(
        message,
        GraphQLErrorOptions {
            extensions,
            ..GraphQLErrorOptions::default()
        },
    )))
}

/// Returns the resolvers of the first half as one field resolver.
fn first_resolver() -> FieldResolverFn {
    Arc::new(|_agent, _source, args, _context, info| {
        let arg = |name: &str| Rc::clone(args).get(name);
        match info.field_name.as_str() {
            "hello" => Completion::Normal(Value::String("world".into())),
            "other" => Completion::Normal(Value::String("other".into())),
            "nullable" => Completion::Normal(Value::Null),
            "thrownError" => Completion::Throw(Value::from(ErrorObject::error("resolver blew up"))),
            "thrownString" => Completion::Throw(Value::String("a thrown string".into())),
            "serviceError" => {
                // ServiceError, a GraphQLError whose
                // extensions are set after super().
                let mut extensions = JsObject::new();
                extensions.insert("code", JsValue::from("FORBIDDEN"));
                extensions.insert("statusCode", JsValue::from(403_u32));
                Completion::Throw(graphql_error("service said no", Some(extensions)))
            }
            "httpStatus" => {
                let status = match arg("status") {
                    Value::Number(number) => number,
                    _ => 0.0,
                };
                let mut http = JsObject::new();
                http.insert("status", JsValue::Number(status));
                let mut extensions = JsObject::new();
                extensions.insert("code", JsValue::from("BAD_REQUEST"));
                extensions.insert("http", JsValue::Object(http));
                let message = format!("http status {}", aosabi_json::number_to_string(status));
                Completion::Throw(graphql_error(&message, Some(extensions)))
            }
            "bareGraphQLError" => Completion::Throw(graphql_error("a bare graphql error", None)),
            "echo" => Completion::Normal(arg("value")),
            "upload" => Completion::Normal(Value::String(upload_text(&arg("file")).into())),
            "touch" => Completion::Normal(Value::String("touched".into())),
            _ => Completion::Normal(Value::Undefined),
        }
    })
}

/// Returns `` `${file.name}:${file.type}:${file.size}` `` of the file a
/// multipart request carries.
fn upload_text(file: &Value) -> String {
    let body = match file {
        Value::Host(HostValue::Opaque(object)) => object.downcast_ref::<FileBody>(),
        _ => None,
    };
    match body {
        Some(body) => format!("{}:{}:{}", body.name, body.r#type, body.bytes.len()),
        None => String::from("undefined:undefined:undefined"),
    }
}

/// Returns the resolvers of the second half as one field resolver.
fn second_resolver() -> FieldResolverFn {
    Arc::new(|_agent, _source, args, _context, info| {
        let arg = |name: &str| Rc::clone(args).get(name);
        match info.field_name.as_str() {
            "hello" => Completion::Normal(Value::String("hello".into())),
            "echoDate" | "echoJson" | "echoJsonObject" => Completion::Normal(arg("value")),
            "echoVoid" => Completion::Normal(Value::Null),
            "stamp" => {
                let year = match arg("year") {
                    Value::Number(number) => number,
                    _ => f64::NAN,
                };
                Completion::Normal(Value::Date(JsDate::from_time_value(utc_new_year(year))))
            }
            "touch" => Completion::Normal(Value::String("touched".into())),
            _ => Completion::Normal(Value::Undefined),
        }
    })
}

/// Returns the time value of `Date.UTC(year, 0, 1)`.
///
/// The function `Date.UTC` reads a year from 0 to 99 as 1900 to 1999, and the
/// day is the one of `MakeDay`, section 21.4.1.28, counted from the epoch.
fn utc_new_year(year: f64) -> f64 {
    if !year.is_finite() {
        return f64::NAN;
    }
    let year = year.trunc();
    let year = if (0.0..=99.0).contains(&year) {
        1900.0 + year
    } else {
        year
    };
    let day = 365.0 * (year - 1970.0) + ((year - 1969.0) / 4.0).floor()
        - ((year - 1901.0) / 100.0).floor()
        + ((year - 1601.0) / 400.0).floor();
    day * 86_400_000.0
}
