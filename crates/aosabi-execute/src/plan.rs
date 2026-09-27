// SPDX-License-Identifier: MIT
/*
 * aosabi-execute plan: the field plan of a document under a schema.
 *
 * No analogue in the reference.
 */

//! The field plan of a document under a schema.
//!
//! A [`Plan`] holds one [`PlanNode`] for each selection an execution of the
//! document collects on a runtime object type: the root selection of an
//! operation on its root type, and the subfields of a field on each object
//! type its value completes as. A node holds its [`PlanField`]s in the order
//! [`collect_fields`](crate::collect_fields::collect_fields) collects them. A
//! field holds its response key preencoded as `"key":` in the byte table of
//! its node, its definition as an index into the fields of the parent type,
//! its return type, the [`LeafKind`] it completes as, the [`ResolverKind`]
//! and the [`ArgsKind`] it is resolved with, the [`SubPlan`] of its selection
//! with the memo of the runtime types of an interface or a union, and the
//! shared field nodes of its response key. A node holds the value of
//! `__typename` on its type as a preencoded constant.
//!
//! A node also keeps, for each static shape an object of its selection had,
//! the slot of the data member each of its fields reads in that shape (see
//! `PlanNode::slots`), so the executor reads the data members of a shape
//! source by slot with no lookup of a name.
//!
//! The plan is built by calling
//! [`collect_fields`](crate::collect_fields::collect_fields),
//! [`collect_sub_fields`](crate::collect_fields::collect_sub_fields) and
//! [`get_field_def`](crate::execute::get_field_def) themselves, never by a
//! walk of its own, so it collects what the reference collects. A node is
//! built when an execution first reaches its selection on its type, so a
//! plan holds the nodes the executions of its document reached, and a node
//! read again costs no collection, no lookup of a field definition and no
//! copy of a field node.
//!
//! A collection that reads a variable through an `@skip` or an `@include`
//! answers what the variables of its request ask. Its node is built for its
//! request alone and the field that leads to it is flagged, so each request
//! collects that selection for itself, and a plan holds no node that depends
//! on the variables of a request. Every value a plan keeps is therefore a
//! function of the document and the schema alone, the rule of the request
//! cache that a kept value is a function of its key.
//!
//! A [`KeptDocument`] holds the fragment map of a document and its plan
//! under each schema it ran against, for a [`Keeper`], the request cache,
//! which holds the kept document beside the parsed document of its entry and
//! counts the estimated bytes of the plans and the map against its budget. A
//! plan keeps a node, an entry, the arguments of a field or a copy of a field
//! node only once its keeper admitted their bytes, the copies a collection
//! shares whether the plan keeps the node of the collection or not. A
//! collection whose bytes the keeper refuses, or that fails, drops the copies
//! it added to the cache of the plan again, so the plan holds no copy it does
//! not count, and once its keeper refused bytes a collection shares its field
//! nodes through a cache of its own, so the plan keeps nothing more. A plan
//! whose schema is gone leaves at the next execution of its document, as a
//! verdict of the cache does. Every execution on a thread finds the kept
//! document of its own document by the address of that document, so a
//! request reads the plan of the document it executes, whatever another
//! request parsed in between. An execution of a document no keeper keeps
//! builds the map and a plan of its own, which end with it.
//!
//! The plan is not `Send` and does not need to be.

use core::{
    cell::{
        Cell,
        OnceCell,
        RefCell, //
    },
    mem::{
        self,
        size_of, //
    }, //
};
use std::rc::{
    Rc,
    Weak, //
};

use aosabi_json::{
    JsObject,
    JsValue,
    map::IndexMap,
    value::KeyHasherBuilder, //
};
use aosabi_lang::ast::{
    ArgumentNode,
    DefinitionNode,
    DirectiveNode,
    FieldNode,
    FragmentDefinitionNode,
    ValueNode, //
};
use aosabi_type::{
    DefinitionError,
    Field,
    IntrospectionResolver,
    ObjMap,
    ObjectType,
    Schema,
    SharedType,
    Type, //
};

use crate::{
    collect_fields::{
        CollectedFields,
        FieldNodeCache,
        collect_root_fields,
        collect_sub_fields_of, //
    },
    execute::{
        ExecutionContext,
        FieldNodes,
        FieldResolve,
        FieldResolverFn,
        FieldSite,
        SharedDocument,
        SharedSchema,
        WeakSchema,
        find_field_def, //
    },
    value::{
        MemberSlot,
        NO_MEMBER,
        Shape, //
    }, //
};

/// The fragment definitions of a document by name.
pub type Fragments = ObjMap<Rc<FragmentDefinitionNode>>;

/// The bytes of the two counts at the head of an `Rc` allocation.
const RC_COUNTS: usize = 2 * size_of::<usize>();

/// The fewest entries at which a fill sweeps the table of a thread.
const SWEEP_FLOOR: usize = 2;

/// What keeps a document with the plans of its executions, told of the bytes they add and free.
///
/// The request cache keeps a document for the requests of its text and counts
/// the estimated bytes of everything it keeps against its budget, so a plan
/// asks it to admit the bytes of a node before the plan keeps the node, and
/// frees the bytes of a plan whose schema is gone.
pub trait Keeper {
    /// Answers whether `bytes` more estimated bytes are admitted, which the keeper then counts.
    fn admit(&self, bytes: usize) -> bool;

    /// Takes `bytes` estimated bytes the keeper counted off its count.
    fn free(&self, bytes: usize);
}

/// The account of the bytes a kept document and its plans hold, and the keeper they weigh on.
struct Ledger {
    /// The keeper.
    keeper: Box<dyn Keeper>,
    /// The estimated bytes the keeper admitted and counts.
    weight: Cell<usize>,
    /// Whether the keeper refused bytes, after which nothing more is kept.
    closed: Cell<bool>,
}

impl Ledger {
    /// Answers whether `bytes` are admitted, which the ledger then counts.
    ///
    /// A refusal closes the ledger, so a kept document whose keeper has no
    /// room keeps what it holds and builds everything else per request, as a
    /// text whose entry passes its bound parses every time.
    fn admit(&self, bytes: usize) -> bool {
        if self.closed.get() {
            return false;
        }
        if !self.keeper.admit(bytes) {
            self.closed.set(true);
            return false;
        }
        self.weight.set(self.weight.get().saturating_add(bytes));
        true
    }

    /// Takes `bytes` the ledger counted off its count and its keeper's.
    fn free(&self, bytes: usize) {
        let bytes = bytes.min(self.weight.get());
        self.weight.set(self.weight.get() - bytes);
        self.keeper.free(bytes);
    }
}

/// The plan of a kept document under one schema.
struct SchemaPlan {
    /// The schema, which the plan keeps no hold of, as the cache keeps a verdict.
    schema: WeakSchema,
    /// The plan.
    plan: Rc<Plan>,
    /// The estimated bytes the ledger counts for the plan.
    weight: Rc<Cell<usize>>,
}

/// The fragment map of a document and its plan under each schema, kept for a [`Keeper`].
///
/// The keeper holds the kept document beside the parsed document of its
/// entry, so an eviction of the entry drops both, and the table of the
/// thread holds a weak handle of it by the address of the document, which
/// every execution of the document on the thread reads.
pub struct KeptDocument {
    /// The document, which keeps its address from every other document while it stands.
    document: SharedDocument,
    /// The account of the bytes kept.
    ledger: Rc<Ledger>,
    /// The fragment definitions of the document by name, built on its first execution.
    fragments: OnceCell<Rc<Fragments>>,
    /// The plan under each schema the document ran against.
    plans: RefCell<Vec<SchemaPlan>>,
}

/// The kept documents of one thread, by the address of each document.
#[derive(Default)]
struct KeptTable {
    /// The weak handle of each kept document, by the address of its document.
    documents: IndexMap<usize, Weak<KeptDocument>, KeyHasherBuilder>,
    /// The count of entries at which the next fill first drops every entry
    /// whose kept document is gone.
    sweep_at: usize,
}

impl KeptTable {
    /// Takes every entry whose kept document is gone out of the table, once
    /// the table holds `sweep_at` entries.
    ///
    /// A sweep reads each entry at most twice and moves each entry it keeps
    /// once, into a new map that hashes its key again, and the next sweep
    /// comes once the table holds twice the entries this one keeps, or
    /// [`SWEEP_FLOOR`]. A sweep of `n` entries therefore follows at least
    /// `n / 2` fills, so a fill pays at most four reads and two moves of an
    /// entry on the average, however many documents the table holds.
    fn sweep(&mut self) {
        if self.documents.len() < self.sweep_at {
            return;
        }
        let gone = |kept: &Weak<KeptDocument>| kept.strong_count() == 0;
        if self.documents.values().any(gone) {
            for (address, kept) in mem::take(&mut self.documents) {
                if !gone(&kept) {
                    self.documents.insert(address, kept);
                }
            }
        }
        self.sweep_at = SWEEP_FLOOR.max(2 * self.documents.len());
    }
}

// An `unsafe` cell would skip each borrow flag.
thread_local! {
    /// The kept documents of the thread, by the address of each document.
    static KEPT: RefCell<KeptTable> = RefCell::default();
}

impl KeptDocument {
    /// Returns a kept document of `document` for `keeper`, which every
    /// execution of the document on this thread reads from then on.
    ///
    /// The kept document starts with no fragment map and no plan, and its
    /// first executions build them, each admitted by `keeper` byte for byte.
    /// The fill of the table of the thread first sweeps it where it has grown
    /// to the count of its next sweep.
    // Out of line, as a fill of a miss, so `20_exec_cost` reads its count by
    // its name.
    #[must_use]
    #[inline(never)]
    pub fn keep(document: &SharedDocument, keeper: Box<dyn Keeper>) -> Rc<Self> {
        let kept = Rc::new(Self {
            document: SharedDocument::clone(document),
            ledger: Rc::new(Ledger {
                keeper,
                weight: Cell::new(0),
                closed: Cell::new(false),
            }),
            fragments: OnceCell::new(),
            plans: RefCell::new(Vec::new()),
        });
        let address = SharedDocument::as_ptr(document).addr();
        KEPT.with_borrow_mut(|table| {
            table.sweep();
            table.documents.insert(address, Rc::downgrade(&kept));
        });
        kept
    }

    /// Returns the kept document of `document` on this thread, none where no keeper keeps it.
    #[must_use]
    pub fn of(document: &SharedDocument) -> Option<Rc<Self>> {
        let address = SharedDocument::as_ptr(document).addr();
        let kept =
            KEPT.with_borrow(|table| table.documents.get(&address).and_then(Weak::upgrade))?;
        SharedDocument::ptr_eq(&kept.document, document).then_some(kept)
    }

    /// Returns the bytes a kept document weighs before it keeps a fragment map or a plan.
    #[must_use]
    pub const fn bytes() -> usize {
        RC_COUNTS + size_of::<Self>() + RC_COUNTS + size_of::<Ledger>()
    }

    /// Returns the estimated bytes its keeper counts for the fragment map and the plans.
    #[must_use]
    pub fn weight(&self) -> usize {
        self.ledger.weight.get()
    }

    /// Returns the document.
    #[must_use]
    pub fn document(&self) -> &SharedDocument {
        &self.document
    }

    /// Returns the fragment map, where an execution built and kept it.
    #[must_use]
    pub fn kept_fragments(&self) -> Option<&Rc<Fragments>> {
        self.fragments.get()
    }

    /// Returns the plan under `schema`, where an execution built it.
    #[must_use]
    pub fn plan_of(&self, schema: &SharedSchema) -> Option<Rc<Plan>> {
        let address = SharedSchema::as_ptr(schema).addr();
        self.plans
            .borrow()
            .iter()
            .find(|kept| kept.schema.as_ptr().addr() == address && kept.schema.strong_count() > 0)
            .map(|kept| Rc::clone(&kept.plan))
    }

    /// Returns the fragment map, built and kept on the first call that its keeper admits.
    fn fragments(&self) -> Rc<Fragments> {
        if let Some(fragments) = self.fragments.get() {
            return Rc::clone(fragments);
        }
        let fragments = Rc::new(build_fragments(&self.document));
        if self.ledger.admit(fragments_bytes(&fragments)) {
            let _ = self.fragments.set(Rc::clone(&fragments));
        }
        fragments
    }

    /// Returns the plan under `schema`, made on the first call that its keeper admits.
    ///
    /// The plans whose schema is gone leave first, with the bytes they
    /// weighed. A plan the keeper does not admit is the plan of the request
    /// alone.
    fn plan(&self, schema: &SharedSchema) -> Rc<Plan> {
        self.prune();
        if let Some(plan) = self.plan_of(schema) {
            return plan;
        }
        if !self.ledger.admit(SchemaPlan::BYTES) {
            return Rc::new(Plan::new(None));
        }
        let weight = Rc::new(Cell::new(SchemaPlan::BYTES));
        let plan = Rc::new(Plan::new(Some(Account {
            ledger: Rc::clone(&self.ledger),
            weight: Rc::clone(&weight),
        })));
        self.plans.borrow_mut().push(SchemaPlan {
            schema: SharedSchema::downgrade(schema),
            plan: Rc::clone(&plan),
            weight,
        });
        plan
    }

    /// Drops the plans whose schema is gone and frees the bytes they weighed.
    fn prune(&self) {
        let mut freed = 0;
        self.plans.borrow_mut().retain(|kept| {
            let alive = kept.schema.strong_count() > 0;
            if !alive {
                freed += kept.weight.get();
            }
            alive
        });
        if freed > 0 {
            self.ledger.free(freed);
        }
    }
}

impl SchemaPlan {
    /// The bytes a plan weighs before it holds a node.
    const BYTES: usize = size_of::<Self>() + RC_COUNTS + size_of::<Plan>() + RC_COUNTS;
}

/// Returns the fragment map and the plan an execution of `document` under `schema` reads.
///
/// A document a keeper keeps answers its kept map and its plan under the
/// schema, and any other document a map and a plan of the request alone.
pub(crate) fn plan_for(
    document: &SharedDocument,
    schema: &SharedSchema,
) -> (Rc<Fragments>, Rc<Plan>) {
    match KeptDocument::of(document) {
        Some(kept) => (kept.fragments(), kept.plan(schema)),
        None => (Rc::new(build_fragments(document)), Rc::new(Plan::new(None))),
    }
}

/// Returns the fragment definitions of `document` by name.
///
/// A later definition of a name replaces an earlier one and keeps its
/// position. A definition is copied with its head alone, and the copy shares
/// the selections of the document.
// Out of line, so `20_exec_cost` counts the builds of a map by their calls.
#[inline(never)]
fn build_fragments(document: &SharedDocument) -> Fragments {
    let mut fragments = ObjMap::default();
    for definition in &document.definitions {
        if let DefinitionNode::FragmentDefinition(definition) = definition {
            fragments.insert(
                String::from(definition.name.value()),
                Rc::new(definition.clone()),
            );
        }
    }
    fragments
}

/// The bytes a plan adds to the account of its kept document.
struct Account {
    /// The ledger of the kept document.
    ledger: Rc<Ledger>,
    /// The bytes the ledger counts for this plan.
    weight: Rc<Cell<usize>>,
}

/// The field plan of a document under one schema.
pub struct Plan {
    /// The account the plan weighs on, none for the plan of one request.
    account: Option<Account>,
    /// The root node of each operation on its root type.
    roots: RefCell<Vec<Root>>,
    /// The field nodes the collections of the plan share, by address.
    field_nodes: FieldNodeCache,
    /// The executions that read the plan.
    executions: Cell<u64>,
}

/// The root node of an operation on a root type.
struct Root {
    /// The address of the operation in the document.
    operation: usize,
    /// The address of the root type.
    ty: usize,
    /// The node.
    slot: Slot,
}

/// Where the node of a selection on a runtime type stands.
#[derive(Clone)]
pub enum Slot {
    /// No execution built the node yet.
    Unbuilt,
    /// The node the plan keeps.
    Kept(Rc<PlanNode>),
    /// The collection reads a variable, so each request builds the node.
    PerRequest,
}

impl Plan {
    /// Returns a plan with no node, which weighs on `account` where it is kept.
    fn new(account: Option<Account>) -> Self {
        Self {
            account,
            roots: RefCell::new(Vec::new()),
            field_nodes: FieldNodeCache::new(),
            executions: Cell::new(0),
        }
    }

    /// Answers whether the plan lives as long as a kept document, rather than one request.
    #[must_use]
    pub fn is_kept(&self) -> bool {
        self.account.is_some()
    }

    /// Returns the executions that read the plan.
    #[must_use]
    pub fn executions(&self) -> u64 {
        self.executions.get()
    }

    /// Returns the estimated bytes the plan weighs on its kept document, 0 for the plan of one request.
    #[must_use]
    pub fn weight(&self) -> usize {
        self.account
            .as_ref()
            .map_or(0, |account| account.weight.get())
    }

    /// Returns the copies of field nodes the collections of the plan share, their count and their estimated bytes.
    ///
    /// A plan of a kept document counts every copy it holds in its weight,
    /// so their bytes never pass [`Plan::weight`].
    #[must_use]
    pub fn copies(&self) -> (usize, usize) {
        self.field_nodes.holds()
    }

    /// Returns the root nodes the plan keeps, each with the address of its operation in the document.
    #[must_use]
    pub fn roots(&self) -> Vec<(usize, Slot)> {
        self.roots
            .borrow()
            .iter()
            .map(|root| (root.operation, root.slot.clone()))
            .collect()
    }

    /// Counts an execution that reads the plan.
    pub(crate) fn enter(&self) {
        self.executions.set(self.executions.get() + 1);
    }

    /// Answers whether the plan keeps nodes whose collection reads a variable.
    ///
    /// The plan of one request keeps them, since every node of it serves
    /// the variables of that request.
    fn keeps_per_request(&self) -> bool {
        self.account.is_none()
    }

    /// Answers whether the plan may keep what its executions build.
    ///
    /// The plan of one request keeps all of it, and a kept plan keeps what
    /// its keeper admits until the keeper refuses bytes once.
    fn is_open(&self) -> bool {
        self.account
            .as_ref()
            .is_none_or(|account| !account.ledger.closed.get())
    }

    /// Answers whether `bytes` are admitted, which the plan then counts.
    fn admit(&self, bytes: usize) -> bool {
        let Some(account) = &self.account else {
            return true;
        };
        if !account.ledger.admit(bytes) {
            return false;
        }
        account
            .weight
            .set(account.weight.get().saturating_add(bytes));
        true
    }

    /// Returns the root node of the operation of `exe_context` on `root_type`.
    ///
    /// A collection shares its field nodes through the cache of the plan
    /// while the plan may keep what it builds. The copies it adds there stay
    /// once the keeper admits their bytes, with the node, or alone for a
    /// root whose collection reads a variable, and the collection drops them
    /// again where the keeper refuses them or the collection fails, so the
    /// cache holds no copy the plan does not count. A root whose collection
    /// reads a variable takes its entry once. Once the keeper refused bytes,
    /// a collection shares its field nodes through a cache of its own, which
    /// ends with the node.
    ///
    /// # Errors
    ///
    /// Returns what [`collect_root_fields`] and
    /// [`get_field_def`](crate::execute::get_field_def) return.
    pub(crate) fn root(
        &self,
        exe_context: &ExecutionContext,
        root_type: &SharedType<ObjectType>,
    ) -> Result<Rc<PlanNode>, DefinitionError> {
        let operation = exe_context.operation_key;
        let ty = SharedType::as_ptr(root_type).addr();
        let key = (operation, ty);
        let unseen = match self.root_slot(operation, ty) {
            Some(Slot::Kept(node)) => return Ok(node),
            slot => slot.is_none(),
        };
        if let Some(node) = exe_context.local_node(key) {
            return Ok(node);
        }
        let open = self.is_open();
        let local;
        let cache = if open {
            &self.field_nodes
        } else {
            local = FieldNodeCache::new();
            &local
        };
        let (collected, reads) = collect_root_fields(
            &exe_context.schema,
            &exe_context.fragments,
            &exe_context.variable_values,
            root_type,
            &exe_context.operation.selection_set,
            cache,
        )
        .inspect_err(|_| cache.drop_uncounted())?;
        let shared = collected.shared;
        let kept = open && (!reads || self.keeps_per_request());
        let Built { node, bytes } =
            PlanNode::build(&exe_context.schema, root_type, collected, kept)
                .inspect_err(|_| cache.drop_uncounted())?;
        if kept && self.admit(bytes + Root::BYTES) {
            self.field_nodes.count_uncounted();
            self.set_root(operation, ty, Slot::Kept(Rc::clone(&node)));
            return Ok(node);
        }
        if open {
            let entry = if unseen { Root::BYTES } else { 0 };
            if reads && (shared + entry == 0 || self.admit(shared + entry)) {
                self.field_nodes.count_uncounted();
                if unseen {
                    self.set_root(operation, ty, Slot::PerRequest);
                }
            } else {
                self.field_nodes.drop_uncounted();
            }
        }
        exe_context.keep_local_node(key, Rc::clone(&node));
        Ok(node)
    }

    /// Returns where the root node of the operation at `operation` on the root type at `ty` stands.
    fn root_slot(&self, operation: usize, ty: usize) -> Option<Slot> {
        self.roots
            .borrow()
            .iter()
            .find(|root| root.operation == operation && root.ty == ty)
            .map(|root| root.slot.clone())
    }

    /// Sets where the root node of the operation at `operation` on the root type at `ty` stands.
    fn set_root(&self, operation: usize, ty: usize, slot: Slot) {
        let mut roots = self.roots.borrow_mut();
        match roots
            .iter_mut()
            .find(|root| root.operation == operation && root.ty == ty)
        {
            Some(root) => root.slot = slot,
            None => roots.push(Root {
                operation,
                ty,
                slot,
            }),
        }
    }

    /// Returns the node of the subfields of `field` on `runtime_type`.
    ///
    /// The field holds its field nodes, so a field and a runtime type name the
    /// node, which the plan keeps for every later execution, and a node the
    /// plan does not keep is kept by the execution for its own later reads.
    /// The collection shares its field nodes as [`Plan::root`] says, the
    /// copies of a node of one request staying where the keeper admits their
    /// bytes, and a field whose selection reads a variable on a runtime type
    /// of an interface or a union takes its entry in the memo once. A kept
    /// node read again costs one count of its `Rc`, the handle the slot
    /// answers, which moves to the caller.
    ///
    /// # Errors
    ///
    /// Returns what [`collect_sub_fields_of`] and
    /// [`get_field_def`](crate::execute::get_field_def) return.
    pub(crate) fn sub(
        &self,
        exe_context: &ExecutionContext,
        field: &Rc<PlanField>,
        runtime_type: &SharedType<ObjectType>,
    ) -> Result<Rc<PlanNode>, DefinitionError> {
        let ty = SharedType::as_ptr(runtime_type).addr();
        let key = (Rc::as_ptr(field).addr(), ty);
        let unbuilt = match field.sub.slot(ty) {
            Slot::Kept(node) => return Ok(node),
            slot => field.kept && matches!(slot, Slot::Unbuilt),
        };
        if field.kept
            && let Some(node) = exe_context.local_node(key)
        {
            return Ok(node);
        }
        let open = self.is_open();
        let local;
        let cache = if open {
            &self.field_nodes
        } else {
            local = FieldNodeCache::new();
            &local
        };
        let (collected, reads) = collect_sub_fields_of(
            &exe_context.schema,
            &exe_context.fragments,
            &exe_context.variable_values,
            runtime_type,
            &field.field_nodes,
            cache,
        )
        .inspect_err(|_| cache.drop_uncounted())?;
        let shared = collected.shared;
        let kept = open && field.kept && (!reads || self.keeps_per_request());
        let Built { node, bytes } =
            PlanNode::build(&exe_context.schema, runtime_type, collected, kept)
                .inspect_err(|_| cache.drop_uncounted())?;
        if kept && self.admit(bytes + field.sub.entry_bytes(ty)) {
            self.field_nodes.count_uncounted();
            field.sub.set(ty, Slot::Kept(Rc::clone(&node)));
            return Ok(node);
        }
        let entry = if reads && unbuilt {
            field.sub.entry_bytes(ty)
        } else {
            0
        };
        let admitted = if open {
            let admitted = shared + entry == 0 || self.admit(shared + entry);
            if admitted {
                self.field_nodes.count_uncounted();
            } else {
                self.field_nodes.drop_uncounted();
            }
            admitted
        } else {
            entry == 0
        };
        if !field.kept {
            // A field of a node of one request holds the nodes of its
            // selection for that request.
            field.sub.set(ty, Slot::Kept(Rc::clone(&node)));
            return Ok(node);
        }
        if reads && unbuilt && admitted {
            field.sub.set(ty, Slot::PerRequest);
        }
        exe_context.keep_local_node(key, Rc::clone(&node));
        Ok(node)
    }
}

impl Root {
    /// The bytes of the entry of a root.
    const BYTES: usize = size_of::<Self>();
}

/// Where the definition of a field stands.
#[derive(Clone, Copy)]
pub enum Definition {
    /// A field of the parent type, by its position among the fields of the type.
    Own(usize),
    /// A meta field, `__schema`, `__type` or `__typename`, borrowed from the static that holds it.
    Meta(&'static Field),
    /// The parent type defines no field of the name, so the key is left out of the result.
    Missing,
}

/// The kind of leaf a field completes as, read from its named type.
///
/// A kind names a scalar by its name alone, since the serializers of a
/// request are no part of the key of a plan, the document and the schema,
/// and an execution reads once which serializer of its request binds the
/// scalar of each kind. Every other scalar is [`LeafKind::Custom`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeafKind {
    /// The specified scalar `String`.
    String,
    /// The specified scalar `ID`.
    Id,
    /// The specified scalar `Boolean`.
    Boolean,
    /// The specified scalar `Int`.
    Int,
    /// The specified scalar `Float`.
    Float,
    /// The scalar `Number`.
    Number,
    /// The scalar `Date`.
    Date,
    /// The scalar `JSON`.
    Json,
    /// The scalar `JSONObject`.
    JsonObject,
    /// An enum type.
    Enum,
    /// Any other scalar, which a serializer of the request or the identity completes.
    Custom,
}

/// What a field is resolved with, read from its definition.
pub enum ResolverKind {
    /// The field holds no resolver of its own, so the resolver of the context runs, `exeContext.fieldResolver`.
    Default,
    /// A resolver function of the field.
    Function(FieldResolverFn),
    /// An introspection resolver of the type layer.
    Introspection(IntrospectionResolver),
}

/// How the arguments of a field are coerced.
pub enum ArgsKind {
    /// The definition holds no argument, so every call takes an empty object.
    None,
    /// No argument value reads a variable, so the arguments are coerced once and kept.
    ///
    /// The cell holds the coerced arguments once the first call coerced
    /// them, or none where that coercion failed or the keeper had no room,
    /// after which each call coerces the arguments.
    Literal(OnceCell<Option<JsObject>>),
    /// An argument value reads a variable, so each call coerces.
    Variables,
}

/// The nodes of the selection of a field.
pub enum SubPlan {
    /// A leaf, which a serializer completes.
    Leaf,
    /// An object type, whose selection has one node.
    Object(RefCell<Slot>),
    /// An interface or a union, whose selection has one node per runtime type, by its address.
    Abstract(RefCell<Vec<(usize, Slot)>>),
}

impl SubPlan {
    /// Returns where the node of the runtime type at `ty` stands.
    fn slot(&self, ty: usize) -> Slot {
        match self {
            Self::Leaf => Slot::Unbuilt,
            Self::Object(slot) => slot.borrow().clone(),
            Self::Abstract(memo) => memo
                .borrow()
                .iter()
                .find(|(at, _)| *at == ty)
                .map_or(Slot::Unbuilt, |(_, slot)| slot.clone()),
        }
    }

    /// Returns the bytes the memo of an abstract type adds for a runtime type it does not hold.
    fn entry_bytes(&self, ty: usize) -> usize {
        match self {
            Self::Abstract(memo) if !memo.borrow().iter().any(|(at, _)| *at == ty) => {
                size_of::<(usize, Slot)>()
            }
            _ => 0,
        }
    }

    /// Sets where the node of the runtime type at `ty` stands.
    fn set(&self, ty: usize, slot: Slot) {
        match self {
            Self::Leaf => {}
            Self::Object(cell) => *cell.borrow_mut() = slot,
            Self::Abstract(memo) => {
                let mut memo = memo.borrow_mut();
                match memo.iter_mut().find(|(at, _)| *at == ty) {
                    Some((_, held)) => *held = slot,
                    None => memo.push((ty, slot)),
                }
            }
        }
    }
}

/// The node of a selection on a runtime object type, its fields in collected order.
pub struct PlanNode {
    /// The object type the selection is collected on.
    runtime_type: SharedType<ObjectType>,
    /// The fields in collected order.
    fields: Box<[Rc<PlanField>]>,
    /// The byte table of the preencoded keys, `"key":` for each field in order.
    keys: Box<str>,
    /// The value of `__typename` on the node, the name of its type as a JSON string.
    typename: Box<str>,
    /// Whether the plan keeps the node, rather than one request.
    kept: bool,
    /// The slots of the fields in each shape an object of the node had, by
    /// the address of the shape (see [`PlanNode::slots`]).
    shapes: RefCell<Vec<ShapeSlots>>,
}

/// The slots of the fields of a node in one [`Shape`].
struct ShapeSlots {
    /// The address of the shape, which names the type of its objects.
    shape: usize,
    /// The slot of the data member of each field in collected order, or
    /// none where no field reads a data member of the shape.
    slots: Option<Rc<[MemberSlot]>>,
}

/// A node as it is built, with the bytes it adds to a plan.
struct Built {
    /// The node.
    node: Rc<PlanNode>,
    /// The estimated bytes of the node and of the field nodes its collection shared first.
    bytes: usize,
}

impl PlanNode {
    /// Builds the node of `collected` on `runtime_type`, whose fields a plan keeps where `kept` holds.
    ///
    /// Each field takes its definition from
    /// [`get_field_def`](crate::execute::get_field_def), through
    /// [`find_field_def`], which answers where the definition stands.
    fn build(
        schema: &Schema,
        runtime_type: &SharedType<ObjectType>,
        collected: Collected,
        kept: bool,
    ) -> Result<Built, DefinitionError> {
        let Collected { fields, shared } = collected;
        let mut keys = String::new();
        let mut planned = Vec::with_capacity(fields.len());
        let mut bytes = shared;
        for (response_name, field_nodes) in fields {
            let start = keys.len();
            keys.push('"');
            keys.push_str(&response_name);
            keys.push_str("\":");
            let field =
                PlanField::build(schema, runtime_type, field_nodes, (start, keys.len()), kept)?;
            bytes = bytes.saturating_add(field.bytes());
            planned.push(Rc::new(field));
        }
        let typename = format!("\"{}\"", runtime_type.name);
        bytes = bytes
            .saturating_add(RC_COUNTS + size_of::<Self>())
            .saturating_add(keys.len())
            .saturating_add(typename.len());
        let node = Rc::new(Self {
            runtime_type: SharedType::clone(runtime_type),
            fields: planned.into_boxed_slice(),
            keys: keys.into_boxed_str(),
            typename: typename.into_boxed_str(),
            kept,
            shapes: RefCell::new(Vec::new()),
        });
        Ok(Built { node, bytes })
    }

    /// Returns the slot of the data member each field of the node reads in
    /// `shape`, the shape of an object of `parent_type`, the type of the node,
    /// or none where no field reads a data member of the shape.
    ///
    /// A field reads the data member of the name of its definition where it
    /// is a data member of the selection (see [`PlanField::is_data_member`])
    /// and the shape holds a data member of that name, and every other field
    /// takes [`NO_MEMBER`]. The slots of a shape are a function of the node
    /// and of the shape, the static shape of a type, so the node keeps them
    /// by the address of the shape, and a node the plan keeps keeps them
    /// where `plan` admits their bytes. So every object of a shape after the
    /// first reads its fields by their slots with no lookup of a name. A kept
    /// node whose keeper refused bytes once answers none for a shape it does
    /// not hold, so its objects read their fields by name, as the loop of
    /// the executor reads a field of any object.
    pub(crate) fn slots(
        &self,
        plan: &Plan,
        parent_type: &ObjectType,
        shape: &'static Shape,
    ) -> Option<Rc<[MemberSlot]>> {
        let address = core::ptr::from_ref(shape).addr();
        if let Some(kept) = self
            .shapes
            .borrow()
            .iter()
            .find(|kept| kept.shape == address)
        {
            return kept.slots.clone();
        }
        if self.kept && !plan.is_open() {
            return None;
        }
        let slots: Vec<MemberSlot> = self
            .fields
            .iter()
            .map(|field| {
                if !field.is_data_member() {
                    return NO_MEMBER;
                }
                match field.field_def(parent_type) {
                    Ok(Some(definition)) => shape.slot(&definition.name).unwrap_or(NO_MEMBER),
                    _ => NO_MEMBER,
                }
            })
            .collect();
        let slots: Option<Rc<[MemberSlot]>> = slots
            .iter()
            .any(|slot| *slot != NO_MEMBER)
            .then(|| Rc::from(slots));
        let bytes = size_of::<ShapeSlots>()
            + slots
                .as_ref()
                .map_or(0, |slots| RC_COUNTS + slots.len() * size_of::<MemberSlot>());
        if !self.kept || plan.admit(bytes) {
            self.shapes.borrow_mut().push(ShapeSlots {
                shape: address,
                slots: slots.clone(),
            });
        }
        slots
    }

    /// Returns the object type the selection is collected on.
    #[must_use]
    pub fn runtime_type(&self) -> &SharedType<ObjectType> {
        &self.runtime_type
    }

    /// Returns the fields in collected order.
    #[must_use]
    pub fn fields(&self) -> &[Rc<PlanField>] {
        &self.fields
    }

    /// Returns the byte table of the preencoded keys.
    #[must_use]
    pub fn keys(&self) -> &str {
        &self.keys
    }

    /// Returns the preencoded key of `field`, `"key":`.
    #[must_use]
    pub fn encoded_key(&self, field: &PlanField) -> &str {
        self.keys.get(field.key.0..field.key.1).unwrap_or_default()
    }

    /// Returns the response key of `field`, its preencoded key without the quotes and the colon.
    #[must_use]
    pub fn key(&self, field: &PlanField) -> &str {
        self.keys
            .get(field.key.0 + 1..field.key.1 - 2)
            .unwrap_or_default()
    }

    /// Returns the value of `__typename` on the node, the name of its type as a JSON string.
    ///
    /// The constructor of an object type asserts its name, so the name holds
    /// ASCII letters, digits and the underscore alone, and its literal is the
    /// one the escape of `aosabi-json` writes of it, with no escape inside.
    /// The executor writes this literal into the answer for a `__typename`
    /// selected on an object of the node.
    #[must_use]
    pub fn typename(&self) -> &str {
        &self.typename
    }
}

/// The fields of a collection with the bytes of the field nodes it shared first.
pub(crate) struct Collected {
    /// The field nodes by response key, in collected order.
    pub(crate) fields: CollectedFields,
    /// The estimated bytes of the copies of field nodes the collection added to the cache.
    pub(crate) shared: usize,
}

/// One field of a node, what an execution reads of it on every object.
pub struct PlanField {
    /// The range of the preencoded key, `"key":`, in the byte table of the node.
    key: (usize, usize),
    /// Where the definition of the field stands.
    definition: Definition,
    /// The type of the definition, none where the parent type defines no field of the name.
    return_type: Option<Type>,
    /// The kind of leaf the field completes as, none for an object, an interface or a union.
    leaf: Option<LeafKind>,
    /// What the field is resolved with.
    resolver: ResolverKind,
    /// How the arguments of the field are coerced.
    args: ArgsKind,
    /// The nodes of the selection of the field.
    sub: SubPlan,
    /// The field nodes of the response key, which every execution shares.
    field_nodes: FieldNodes,
    /// Whether the plan keeps the node of the field, rather than one request.
    kept: bool,
}

impl PlanField {
    /// Builds the field of `field_nodes` on `parent_type`, whose key stands at `key` in the table of its node.
    fn build(
        schema: &Schema,
        parent_type: &SharedType<ObjectType>,
        field_nodes: FieldNodes,
        key: (usize, usize),
        kept: bool,
    ) -> Result<Self, DefinitionError> {
        let site = match field_nodes.first() {
            Some(first) => find_field_def(schema, parent_type, first)?,
            None => FieldSite::Missing,
        };
        let (definition, field) = match site {
            FieldSite::Own(index, field) => (Definition::Own(index), Some(field)),
            FieldSite::Meta(field) => (Definition::Meta(field), Some(field)),
            FieldSite::Missing => (Definition::Missing, None),
        };
        let return_type = field.map(|field| field.r#type.clone());
        let named = return_type.as_ref().map(named_of);
        let leaf = named.and_then(leaf_kind);
        let sub = match named {
            Some(Type::Object(_)) => SubPlan::Object(RefCell::new(Slot::Unbuilt)),
            Some(Type::Interface(_) | Type::Union(_)) => {
                SubPlan::Abstract(RefCell::new(Vec::new()))
            }
            _ => SubPlan::Leaf,
        };
        let resolver = field.map_or(ResolverKind::Default, resolver_kind);
        let args = match (field, field_nodes.first()) {
            (Some(field), _) if field.args.is_empty() => ArgsKind::None,
            (Some(_), Some(first)) if reads_variable(first.arguments.as_deref()) => {
                ArgsKind::Variables
            }
            _ => ArgsKind::Literal(OnceCell::new()),
        };
        Ok(Self {
            key,
            definition,
            return_type,
            leaf,
            resolver,
            args,
            sub,
            field_nodes,
            kept,
        })
    }

    /// Returns the estimated bytes of the field, its list of field nodes and its allocation.
    fn bytes(&self) -> usize {
        size_of::<Rc<Self>>()
            + RC_COUNTS
            + size_of::<Self>()
            + RC_COUNTS
            + self.field_nodes.len() * size_of::<Rc<FieldNode>>()
    }

    /// Returns where the definition of the field stands.
    #[must_use]
    pub fn definition(&self) -> Definition {
        self.definition
    }

    /// Returns the definition of the field on `parent_type`, the type of its node.
    ///
    /// # Errors
    ///
    /// Returns what `getFields` of `parent_type` throws.
    pub fn field_def<'a>(
        &self,
        parent_type: &'a ObjectType,
    ) -> Result<Option<&'a Field>, DefinitionError> {
        Ok(match self.definition {
            Definition::Own(index) => parent_type
                .get_fields()?
                .get_index(index)
                .map(|(_, field)| field),
            Definition::Meta(field) => Some(field),
            Definition::Missing => None,
        })
    }

    /// Returns the type of the definition, none where the parent type defines no field of the name.
    #[must_use]
    pub fn return_type(&self) -> Option<&Type> {
        self.return_type.as_ref()
    }

    /// Returns the kind of leaf the field completes as, none for an object, an interface or a union.
    #[must_use]
    pub fn leaf(&self) -> Option<LeafKind> {
        self.leaf
    }

    /// Returns what the field is resolved with.
    #[must_use]
    pub fn resolver(&self) -> &ResolverKind {
        &self.resolver
    }

    /// Returns how the arguments of the field are coerced.
    #[must_use]
    pub fn args(&self) -> &ArgsKind {
        &self.args
    }

    /// Returns the nodes of the selection of the field.
    #[must_use]
    pub fn sub(&self) -> &SubPlan {
        &self.sub
    }

    /// Returns the field nodes of the response key.
    #[must_use]
    pub fn field_nodes(&self) -> &FieldNodes {
        &self.field_nodes
    }

    /// Answers whether the collection of the selection of the field reads a variable on some type.
    #[must_use]
    pub fn is_per_request(&self) -> bool {
        let per_request = |slot: &Slot| matches!(slot, Slot::PerRequest);
        match &self.sub {
            SubPlan::Leaf => false,
            SubPlan::Object(slot) => per_request(&slot.borrow()),
            SubPlan::Abstract(memo) => memo.borrow().iter().any(|(_, slot)| per_request(slot)),
        }
    }

    /// Answers whether the plan keeps the field, rather than one request.
    #[must_use]
    pub fn is_kept(&self) -> bool {
        self.kept
    }

    /// Answers whether the field is a data member of its selection, a field
    /// of the default resolver with no argument that its parent type defines.
    ///
    /// Where the resolver of the context reads members, the executor reads
    /// such a field from its object as a member, by name or by the slot of
    /// the shape of the object (see `PlanNode::slots`).
    #[must_use]
    pub fn is_data_member(&self) -> bool {
        matches!(self.resolver, ResolverKind::Default)
            && matches!(self.args, ArgsKind::None)
            && matches!(self.definition, Definition::Own(_))
            && !self.field_nodes.is_empty()
    }

    /// Answers whether the field is the meta field `__typename`, whose value
    /// on an object of its node is the name of the type of the node (see
    /// [`PlanNode::typename`]).
    ///
    /// The definition of a meta field is the static the name of the first
    /// field node finds, so such a field has its field nodes, and of the
    /// three meta fields `__typename` alone holds the resolver
    /// `TypeNameMetaField`.
    #[must_use]
    pub fn is_type_name(&self) -> bool {
        matches!(self.definition, Definition::Meta(_))
            && matches!(
                self.resolver,
                ResolverKind::Introspection(IntrospectionResolver::TypeNameMetaField)
            )
    }

    /// Returns what a call of the field reads of its kept arguments.
    pub(crate) fn kept_args(&self) -> KeptArgs<'_> {
        match &self.args {
            ArgsKind::None => KeptArgs::None,
            ArgsKind::Literal(cell) => match cell.get() {
                Some(Some(args)) => KeptArgs::Kept(args),
                Some(None) => KeptArgs::Coerce,
                None => KeptArgs::First,
            },
            ArgsKind::Variables => KeptArgs::Coerce,
        }
    }

    /// Keeps the arguments the first call coerced, `args`, none where that coercion failed.
    ///
    /// A field the plan keeps keeps them where `plan` admits their bytes,
    /// and every later call coerces where it does not.
    pub(crate) fn keep_args(&self, plan: &Plan, args: Option<&JsObject>) {
        let ArgsKind::Literal(cell) = &self.args else {
            return;
        };
        let kept = args.filter(|args| !self.kept || plan.admit(object_bytes(args)));
        let _ = cell.set(kept.cloned());
    }
}

/// What a call of a field reads of the arguments its plan keeps.
pub(crate) enum KeptArgs<'a> {
    /// The definition holds no argument.
    None,
    /// The kept arguments, which a call copies where it hands them on.
    Kept(&'a JsObject),
    /// The first call, which coerces the arguments and keeps them.
    First,
    /// A call that coerces the arguments.
    Coerce,
}

/// Returns the type under every wrapper of `ty`.
fn named_of(ty: &Type) -> &Type {
    match ty {
        Type::List(list) => named_of(&list.of_type),
        Type::NonNull(non_null) => named_of(non_null.of_type()),
        named => named,
    }
}

/// Returns the kind of leaf the named type `named` is, none for a type that is no leaf.
fn leaf_kind(named: &Type) -> Option<LeafKind> {
    match named {
        Type::Scalar(scalar) => Some(match scalar.name.as_str() {
            "String" => LeafKind::String,
            "ID" => LeafKind::Id,
            "Boolean" => LeafKind::Boolean,
            "Int" => LeafKind::Int,
            "Float" => LeafKind::Float,
            "Number" => LeafKind::Number,
            "Date" => LeafKind::Date,
            "JSON" => LeafKind::Json,
            "JSONObject" => LeafKind::JsonObject,
            _ => LeafKind::Custom,
        }),
        Type::Enum(_) => Some(LeafKind::Enum),
        _ => None,
    }
}

/// Returns what `field` is resolved with, `fieldDef.resolve ?? exeContext.fieldResolver` at `execute.js:352`.
///
/// A handle that holds neither a resolver function nor an introspection
/// resolver reads as no resolver.
fn resolver_kind(field: &Field) -> ResolverKind {
    let Some(handle) = &field.resolve else {
        return ResolverKind::Default;
    };
    if let Some(resolve) = handle.downcast_ref::<FieldResolve>() {
        return ResolverKind::Function(FieldResolverFn::clone(&resolve.0));
    }
    match handle.downcast_ref::<IntrospectionResolver>() {
        Some(resolver) => ResolverKind::Introspection(*resolver),
        None => ResolverKind::Default,
    }
}

/// Answers whether the value of an argument of `arguments` reads a variable.
pub(crate) fn reads_variable(arguments: Option<&[ArgumentNode]>) -> bool {
    arguments
        .unwrap_or_default()
        .iter()
        .any(|argument| value_reads_variable(&argument.value))
}

/// Answers whether `value` reads a variable, at its top or inside a list or an object.
fn value_reads_variable(value: &ValueNode) -> bool {
    match value {
        ValueNode::Variable(_) => true,
        ValueNode::ListValue(list) => list.values.iter().any(value_reads_variable),
        ValueNode::ObjectValue(object) => object
            .fields
            .iter()
            .any(|field| value_reads_variable(&field.value)),
        _ => false,
    }
}

/// Answers whether an `@skip` or an `@include` of `directives` reads a variable.
pub(crate) fn condition_reads_variable(directives: Option<&[DirectiveNode]>) -> bool {
    directives.unwrap_or_default().iter().any(|directive| {
        matches!(directive.name.value(), "skip" | "include")
            && reads_variable(directive.arguments.as_deref())
    })
}

/// Returns the estimated bytes a copy of the head of `node` holds on the heap.
///
/// The copy holds its arguments and its directives, and shares its
/// selections and the source of its location.
pub(crate) fn head_bytes(node: &FieldNode) -> usize {
    let names =
        node.alias.as_ref().map_or(0, |alias| alias.value().len()) + node.name.value().len();
    names
        + arguments_bytes(node.arguments.as_deref())
        + directives_bytes(node.directives.as_deref())
}

/// Returns the estimated bytes `arguments` hold on the heap.
fn arguments_bytes(arguments: Option<&[ArgumentNode]>) -> usize {
    arguments.map_or(0, |arguments| {
        arguments
            .iter()
            .map(|argument| {
                size_of::<ArgumentNode>()
                    + argument.name.value().len()
                    + value_bytes(&argument.value)
            })
            .sum()
    })
}

/// Returns the estimated bytes `directives` hold on the heap.
fn directives_bytes(directives: Option<&[DirectiveNode]>) -> usize {
    directives.map_or(0, |directives| {
        directives
            .iter()
            .map(|directive| {
                size_of::<DirectiveNode>()
                    + directive.name.value().len()
                    + arguments_bytes(directive.arguments.as_deref())
            })
            .sum()
    })
}

/// Returns the estimated bytes `value` holds on the heap.
fn value_bytes(value: &ValueNode) -> usize {
    match value {
        ValueNode::Variable(variable) => variable.name.value().len(),
        ValueNode::IntValue(int) => int.value.len(),
        ValueNode::FloatValue(float) => float.value.len(),
        ValueNode::StringValue(string) => string.value.len(),
        ValueNode::EnumValue(enum_value) => enum_value.value.len(),
        ValueNode::ListValue(list) => list
            .values
            .iter()
            .map(|item| size_of::<ValueNode>() + value_bytes(item))
            .sum(),
        ValueNode::ObjectValue(object) => object
            .fields
            .iter()
            .map(|field| {
                size_of::<aosabi_lang::ast::ObjectFieldNode>()
                    + field.name.value().len()
                    + value_bytes(&field.value)
            })
            .sum(),
        ValueNode::BooleanValue(_) | ValueNode::NullValue(_) => 0,
    }
}

/// Returns the estimated bytes of a fragment map and its copies of the heads of the definitions.
fn fragments_bytes(fragments: &Fragments) -> usize {
    let entries: usize = fragments
        .iter()
        .map(|(name, definition)| {
            2 * size_of::<(String, Rc<FragmentDefinitionNode>)>()
                + name.len()
                + RC_COUNTS
                + size_of::<FragmentDefinitionNode>()
                + definition.name.value().len()
                + directives_bytes(definition.directives.as_deref())
        })
        .sum();
    RC_COUNTS + size_of::<Fragments>() + entries
}

/// Returns the estimated bytes a coerced object holds, its map and its values.
fn object_bytes(object: &JsObject) -> usize {
    size_of::<JsObject>()
        + object
            .iter()
            .map(|(key, value)| {
                2 * size_of::<(aosabi_json::JsString, JsValue)>() + key.len() + js_bytes(value)
            })
            .sum::<usize>()
}

/// Returns the estimated bytes `value` holds on the heap.
fn js_bytes(value: &JsValue) -> usize {
    match value {
        JsValue::String(string) => string.len(),
        JsValue::Array(items) => items
            .iter()
            .map(|item| size_of::<JsValue>() + js_bytes(item))
            .sum(),
        JsValue::Object(object) => object_bytes(object),
        _ => 0,
    }
}
