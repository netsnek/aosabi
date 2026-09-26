// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the introspection query of graphql-js.
 *
 * The interfaces of :183-349 describe the shape of an introspection result
 * and have no runtime existence, so they have no item here, and the client
 * schema reads that shape from a value of the JSON layer.
 *
 * Derived from graphql-js 16.12.0, src/utilities/getIntrospectionQuery.ts, MIT.
 */

//! The introspection query.
//!
//! The function [`get_introspection_query`] returns the text of the query a
//! client sends to read the schema of a server, the query `pull` posts. The
//! text is a template literal in the reference whose holes are filled from
//! [`IntrospectionOptions`], and a hole an option leaves empty keeps the
//! indent of its line, so the bytes of the query carry lines of spaces alone.
//! The template is one string here, line by line as the reference writes it,
//! because the bytes the server reads are the bytes of that literal.

use alloc::{
    format,
    string::String, //
};

/// The options of the introspection query.
///
/// Every option of the reference is optional, and
/// [`IntrospectionOptions::default`] gives the value each takes when it is
/// absent.
///
/// Derived from graphql-js 16.12.0, src/utilities/getIntrospectionQuery.ts, MIT.
// The six flags are the six options of the reference, one field each.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is one option of the reference"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntrospectionOptions {
    /// Whether to include descriptions in the introspection result.
    pub descriptions: bool,
    /// Whether to include `specifiedByURL` in the introspection result.
    pub specified_by_url: bool,
    /// Whether to include `isRepeatable` flag on directives.
    pub directive_is_repeatable: bool,
    /// Whether to include `description` field on schema.
    pub schema_description: bool,
    /// Whether target GraphQL server support deprecation of input values.
    pub input_value_deprecation: bool,
    /// Whether target GraphQL server supports `@oneOf` input objects.
    pub one_of: bool,
}

impl Default for IntrospectionOptions {
    /// Returns the options the reference fills in for an absent one,
    /// `descriptions` true and every other flag false.
    fn default() -> Self {
        Self {
            descriptions: true,
            specified_by_url: false,
            directive_is_repeatable: false,
            schema_description: false,
            input_value_deprecation: false,
            one_of: false,
        }
    }
}

/// Produces the GraphQL query recommended for a full schema introspection.
///
/// The reference takes the options as an optional object and spreads it over
/// the defaults, which [`IntrospectionOptions::default`] stands for, so a
/// call with no argument there is a call with the default here. The argument
/// `options` chooses which members the query asks for. The text starts with a
/// newline and ends with a newline and two spaces, as the literal does.
///
/// Derived from graphql-js 16.12.0, src/utilities/getIntrospectionQuery.ts, MIT.
// The template literal of the reference runs over more than a hundred lines,
// and the query reads as the reference writes it only as one literal.
#[expect(
    clippy::too_many_lines,
    reason = "the template of the reference is one literal"
)]
#[must_use]
pub fn get_introspection_query(options: IntrospectionOptions) -> String {
    let descriptions = if options.descriptions {
        "description"
    } else {
        ""
    };
    let specified_by_url = if options.specified_by_url {
        "specifiedByURL"
    } else {
        ""
    };
    let directive_is_repeatable = if options.directive_is_repeatable {
        "isRepeatable"
    } else {
        ""
    };
    let schema_description = if options.schema_description {
        descriptions
    } else {
        ""
    };

    let input_deprecation = |str: &'static str| {
        if options.input_value_deprecation {
            str
        } else {
            ""
        }
    };
    let one_of = if options.one_of { "isOneOf" } else { "" };
    let include_deprecated = input_deprecation("(includeDeprecated: true)");
    let is_deprecated = input_deprecation("isDeprecated");
    let deprecation_reason = input_deprecation("deprecationReason");

    format!(
        "
    query IntrospectionQuery {{
      __schema {{
        {schema_description}
        queryType {{ name kind }}
        mutationType {{ name kind }}
        subscriptionType {{ name kind }}
        types {{
          ...FullType
        }}
        directives {{
          name
          {descriptions}
          {directive_is_repeatable}
          locations
          args{include_deprecated} {{
            ...InputValue
          }}
        }}
      }}
    }}

    fragment FullType on __Type {{
      kind
      name
      {descriptions}
      {specified_by_url}
      {one_of}
      fields(includeDeprecated: true) {{
        name
        {descriptions}
        args{include_deprecated} {{
          ...InputValue
        }}
        type {{
          ...TypeRef
        }}
        isDeprecated
        deprecationReason
      }}
      inputFields{include_deprecated} {{
        ...InputValue
      }}
      interfaces {{
        ...TypeRef
      }}
      enumValues(includeDeprecated: true) {{
        name
        {descriptions}
        isDeprecated
        deprecationReason
      }}
      possibleTypes {{
        ...TypeRef
      }}
    }}

    fragment InputValue on __InputValue {{
      name
      {descriptions}
      type {{ ...TypeRef }}
      defaultValue
      {is_deprecated}
      {deprecation_reason}
    }}

    fragment TypeRef on __Type {{
      kind
      name
      ofType {{
        kind
        name
        ofType {{
          kind
          name
          ofType {{
            kind
            name
            ofType {{
              kind
              name
              ofType {{
                kind
                name
                ofType {{
                  kind
                  name
                  ofType {{
                    kind
                    name
                    ofType {{
                      kind
                      name
                      ofType {{
                        kind
                        name
                      }}
                    }}
                  }}
                }}
              }}
            }}
          }}
        }}
      }}
    }}
  "
    )
}
