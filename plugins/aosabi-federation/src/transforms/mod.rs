// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the request and result transforms of a delegation.
 *
 * The four transforms of the reference's federation are the modules below.
 *
 * Derived from @graphql-tools/delegate 12.0.9, dist/index.d.ts, MIT.
 * Derived from @graphql-tools/utils 11.0.0, typings/Interfaces.d.ts, MIT.
 */

//! The transforms of a delegation, the interface and the four transforms the
//! reference's federation runs.
//!
//! A delegation builds an [`ExecutionRequest`] for the field it delegates,
//! hands it through the request half of every [`Transform`] between the
//! preparation and the finalization of its document, and hands the result the
//! executor answers through the result half. The [`DelegationContext`] is
//! what each transform reads of the delegation, the target schema and the
//! field name above all.
//!
//! The interface of the reference names a third hook, `transformSchema`,
//! which only a subschema configuration with transforms runs, and the
//! federation configures none, so the trait leaves it out. The third argument
//! of a request and a result hook, the transformation context, is an empty
//! object the `Transformer` makes per transform and that no transform of
//! the gateway reads, so it has no parameter here. The members `signal` and
//! `schemaCoordinateInErrors` of a request and `onLocatedError` and
//! `rootValue` of a delegation context are left out with the branches that
//! read them, which the gateway never takes.

pub mod force_args;
pub mod inject_needs;
pub mod inline_args;
pub mod patch;

use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_core::Thrown;
use aosabi_execute::{
    execute::ResolveInfo,
    value::{
        ContextValue,
        Value, //
    }, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::ast::{
    DefinitionNode,
    DocumentNode,
    FragmentDefinitionNode,
    OperationTypeNode, //
};
use aosabi_type::{
    Schema,
    Type, //
};

use crate::delegate::subschema::{
    SchemaOrSubschemaConfig,
    SubschemaConfig, //
};

/// One request a delegation sends, `ExecutionRequest` of `@graphql-tools/utils`.
///
/// A clone shares the document, as a spread of the reference shares it. The
/// request `createRequest` builds shares the fragments and the variables of
/// the client's request, as the reference shares the objects themselves: the
/// fragments stand in `fragments` until the preparation writes the document
/// anew with them, and the variables spread the client's object,
/// [`Variables`].
#[derive(Clone)]
pub struct ExecutionRequest {
    /// The document of the request.
    pub document: Rc<DocumentNode>,
    /// The fragment definitions that follow the definitions of `document`, shared with the client's request.
    ///
    /// The document `createRequest` builds ends with the fragments of the
    /// client. They stay shared here until the preparation of the delegation
    /// reads them, and every request after it holds them in its document.
    pub fragments: Option<Rc<[Rc<FragmentDefinitionNode>]>>,
    /// The variable values, `variables`.
    pub variables: Option<Variables>,
    /// The name of the operation to run, `operationName`.
    pub operation_name: Option<String>,
    /// The extensions the body carries, `extensions`.
    pub extensions: Option<JsObject>,
    /// The context value of the request, `context`.
    pub context: Option<ContextValue>,
    /// The resolve information of the field the request delegates, `info`.
    pub info: Option<Rc<ResolveInfo>>,
    /// The value the root fields resolve on, `rootValue`.
    pub root_value: Value,
    /// The lifted files of the upload variables, keyed by variable name.
    pub uploads: Vec<(String, Value)>,
    /// The name of the subschema the request goes to, `subgraphName`.
    pub subgraph_name: Option<String>,
    /// The type of the operation, `operationType`.
    pub operation_type: Option<OperationTypeNode>,
}

impl ExecutionRequest {
    /// Returns the request of `document` with every other member absent.
    #[must_use]
    pub fn new(document: Rc<DocumentNode>) -> Self {
        Self {
            document,
            fragments: None,
            variables: None,
            operation_name: None,
            extensions: None,
            context: None,
            info: None,
            root_value: Value::Undefined,
            uploads: Vec::new(),
            subgraph_name: None,
            operation_type: None,
        }
    }
}

impl ExecutionRequest {
    /// Returns the document with the fragments that follow it, the document `createRequest` builds.
    #[must_use]
    pub fn whole_document(&self) -> DocumentNode {
        let mut document = self.document.as_ref().clone();
        document.definitions.extend(
            self.fragments
                .iter()
                .flat_map(|fragments| fragments.iter())
                .map(|fragment| DefinitionNode::FragmentDefinition(fragment.as_ref().clone())),
        );
        document
    }
}

/// The variable values of a request, `variables`, the object a request spreads first and its own members.
///
/// `createRequest` spreads the client's variables into a new object and
/// writes the projected arguments over it, so the values are shared and only
/// the slots are new. The spread here shares the client's object itself
/// behind its `Rc`, a read answers an own member before a member of the
/// spread, and [`Variables::to_object`] writes the object the spread makes:
/// the members of the spread in their order, each with the value of the own
/// member of its name where there is one, and the other own members after
/// them in the order they were written.
#[derive(Clone, Debug, Default)]
pub struct Variables {
    /// The object the variables spread first, shared with the client's request.
    spread: Option<Rc<JsObject>>,
    /// The members written over the spread, in the order they were written.
    own: JsObject,
}

impl Variables {
    /// Returns the variables that spread `object` and have no own member, `{...object}`.
    #[must_use]
    pub fn spreading(object: Rc<JsObject>) -> Self {
        Self {
            spread: Some(object),
            own: JsObject::new(),
        }
    }

    /// Returns `variables[name]`, an own member before a member of the spread.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&JsValue> {
        self.own
            .get(name)
            .or_else(|| self.spread.as_deref()?.get(name))
    }

    /// Sets the member `name`, `variables[name] = value`.
    pub fn insert(&mut self, name: &str, value: JsValue) {
        self.own.insert(name, value);
    }

    /// Returns whether the variables have no member.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.own.is_empty() && self.spread.as_deref().is_none_or(JsObject::is_empty)
    }

    /// Returns the object the spread and the own members make.
    #[must_use]
    pub fn to_object(&self) -> JsObject {
        let Some(spread) = self.spread.as_deref() else {
            return self.own.clone();
        };
        let mut object = JsObject::with_capacity(spread.len() + self.own.len());
        for (key, value) in spread.own_keys() {
            object.insert(key.clone(), value.clone());
        }
        for (key, value) in self.own.own_keys() {
            object.insert(key.clone(), value.clone());
        }
        object
    }

    /// Returns the object the spread and the own members make, taking the own members.
    #[must_use]
    pub fn into_object(self) -> JsObject {
        match self.spread {
            None => self.own,
            Some(_) => self.to_object(),
        }
    }
}

impl From<JsObject> for Variables {
    fn from(own: JsObject) -> Self {
        Self { spread: None, own }
    }
}

/// What one delegation knows while its transforms run, `DelegationContext`.
#[derive(Clone)]
pub struct DelegationContext {
    /// The schema or the subschema configuration the delegation targets, `subschema`.
    pub subschema: SchemaOrSubschemaConfig,
    /// The subschema configuration, when the delegation targets one, `subschemaConfig`.
    pub subschema_config: Option<Rc<SubschemaConfig>>,
    /// The schema the request is validated and finalized against, `targetSchema`.
    pub target_schema: Arc<Schema>,
    /// The type of the delegated operation, `operation`.
    pub operation: OperationTypeNode,
    /// The name of the root field of the target, `fieldName`.
    pub field_name: String,
    /// The arguments the delegation passes, `args`.
    pub args: Option<JsObject>,
    /// The context value of the request, `context`.
    pub context: Option<ContextValue>,
    /// The resolve information of the field that delegates, `info`.
    pub info: Option<Rc<ResolveInfo>>,
    /// The type the delegated value completes as, `returnType`.
    pub return_type: Type,
    /// The transforms of the delegation in their array order, `transforms`.
    pub transforms: Vec<Rc<dyn Transform>>,
    /// The schema the document is prepared against, `transformedSchema`.
    pub transformed_schema: Arc<Schema>,
    /// Whether the merging of types is skipped, `skipTypeMerging`.
    pub skip_type_merging: bool,
}

/// A request and result transform of a delegation.
///
/// A hook that throws makes the delegation throw what it threw, as the
/// reference's synchronous hooks do.
pub trait Transform {
    /// Returns the request with this transform applied, `transformRequest`.
    ///
    /// # Errors
    ///
    /// Returns what the transform throws.
    fn transform_request(
        &self,
        request: ExecutionRequest,
        context: &DelegationContext,
    ) -> Result<ExecutionRequest, Thrown>;

    /// Returns the result with this transform applied, `transformResult`.
    ///
    /// The default answers the result unchanged, which is a transform with no
    /// result hook.
    ///
    /// # Errors
    ///
    /// Returns what the transform throws.
    fn transform_result(
        &self,
        result: Value,
        context: &DelegationContext,
    ) -> Result<Value, Thrown> {
        let _ = context;
        Ok(result)
    }
}
