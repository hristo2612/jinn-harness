//! What a caller asks for: the workflow spec — its nodes, its edges and
//! its typed input schema — and the request documents of every operation
//! that takes one.
//!
//! # Node and edge kinds are TYPES, not strings
//!
//! A free-string node kind is a dispatch table nobody can enumerate: a
//! reader cannot tell which kinds exist, a store cannot refuse one it
//! does not implement, and a typo becomes a node that silently does
//! nothing. So both are closed value spaces ([`NodeKind`], [`EdgeKind`]),
//! and a kind this version cannot name is REFUSED at decode.
//!
//! # An actor is declared, never inferred
//!
//! Same law as the todos seam's, and for the same reason: absence records
//! that nobody was declared, and a blank that would render like a
//! principal is refused rather than recorded.

use serde::{Deserialize, Serialize};

use crate::{ErrorCode, Extensions, WorkflowError, API_VERSION};

/// Who asked for a write. `None` is "nobody was declared", and it stays
/// that way all the way to the record.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Attribution {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

impl Attribution {
    /// The declared actor, checked.
    ///
    /// # Errors
    ///
    /// The actor is present and blank — a sentinel, not a principal.
    pub fn check(&self) -> Result<Option<String>, WorkflowError> {
        match self.actor.as_deref() {
            None => Ok(None),
            Some(name) if name.trim().is_empty() => Err(WorkflowError::new(
                ErrorCode::Invalid,
                "an `actor` is a principal's name; omit it to record that none was declared, \
                 rather than sending a blank that would read like one",
            )),
            Some(name) => Ok(Some(name.to_owned())),
        }
    }
}

/// What a node DOES. A CLOSED value space (see the module doc).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeKind {
    /// The node has no work of its own: it ends `done` the moment it
    /// starts. What makes an entry, a join or an exit expressible without
    /// pretending they dispatch anything.
    #[default]
    Checkpoint,
    /// The node dispatches work through the TODOS seam's DEFINITION — the
    /// fourth layer's whole point. Carries a [`TodoBinding`].
    Dispatch,
}

impl NodeKind {
    /// The kind's wire tag.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Checkpoint => "checkpoint",
            Self::Dispatch => "dispatch",
        }
    }
}

/// When an edge is FOLLOWED. A CLOSED value space.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeKind {
    /// Followed whenever the source node ends, however it ended.
    #[default]
    Always,
    /// Followed only when the source node ended `done`.
    OnDone,
    /// Followed only when the source node ended in a state that is not
    /// `done` — the failure lane, named positively rather than as "not
    /// the other one".
    OnNotDone,
}

impl EdgeKind {
    /// The kind's wire tag.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::OnDone => "on-done",
            Self::OnNotDone => "on-not-done",
        }
    }

    /// Whether this edge is followed for a source node that ended in
    /// `state`. A source that has not ended follows nothing — the caller
    /// checks that before asking.
    #[must_use]
    pub fn follows(self, state: crate::NodeState) -> bool {
        match self {
            Self::Always => true,
            Self::OnDone => state == crate::NodeState::Done,
            Self::OnNotDone => state != crate::NodeState::Done,
        }
    }
}

/// What a [`NodeKind::Dispatch`] node dispatches: a TODO store id and the
/// Todo it records there, with the dispatch binding that Todo is sent
/// with. Every one of those is the TODOS seam's own vocabulary reached
/// through its definition — this seam names no Todo provider, no session
/// provider and no engine provider.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TodoBinding {
    /// The Todo store id — the second half of `jinn:todo.<store>`.
    pub store: String,
    /// The Todo this node records. Its title defaults to the node's own
    /// when blank, so a node is never dispatched as an unnamed Todo.
    #[serde(default)]
    pub todo: jinn_todo::TodoSpec,
    /// Where the Todo's work is done: a SESSION store and an engine
    /// binding, both definitions. Changing ONE field here runs the whole
    /// workflow over another engine.
    pub dispatch: jinn_todo::DispatchSpec,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One node of a workflow.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct NodeSpec {
    /// The node id, unique within the workflow. Edges name it.
    pub id: String,
    #[serde(default)]
    pub kind: NodeKind,
    #[serde(default)]
    pub title: String,
    /// Present exactly when `kind` is [`NodeKind::Dispatch`] — checked,
    /// not assumed ([`WorkflowSpec::check`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todo: Option<TodoBinding>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One edge of a workflow.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EdgeSpec {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub kind: EdgeKind,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// What kind of value one input field holds. A CLOSED value space: an
/// input type this version cannot name is refused rather than accepted
/// unchecked.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FieldKind {
    #[default]
    String,
    Number,
    Bool,
}

impl FieldKind {
    /// The kind's wire tag.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Bool => "bool",
        }
    }

    /// Whether `value` is one of these.
    #[must_use]
    pub fn admits(self, value: &serde_json::Value) -> bool {
        match self {
            Self::String => value.is_string(),
            Self::Number => value.is_number(),
            Self::Bool => value.is_boolean(),
        }
    }
}

/// One declared input field.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct FieldSpec {
    pub name: String,
    #[serde(default)]
    pub kind: FieldKind,
    /// A field that must be present. An absent optional field is absent —
    /// never a zero value standing in for one that was not given.
    #[serde(default)]
    pub required: bool,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// A workflow's typed input schema.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct InputSchema {
    #[serde(default)]
    pub fields: Vec<FieldSpec>,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl InputSchema {
    /// Checks one input document against this schema.
    ///
    /// A field the schema does not declare is REFUSED rather than carried
    /// through: an input a workflow cannot read is a caller believing
    /// something is being used that is not, which is the same class of
    /// quiet wrongness as a status nothing justifies. (Additivity is a
    /// law about a WIRE type's unknown fields, not about a caller's
    /// arguments — a document `extra` map keeps forward compatibility for
    /// the schema itself, and the arguments it declares stay closed.)
    ///
    /// # Errors
    ///
    /// A required field absent, a field of the wrong kind, or a field
    /// this schema does not declare.
    pub fn check(&self, input: &Extensions) -> Result<(), WorkflowError> {
        for field in &self.fields {
            match input.get(&field.name) {
                None if field.required => {
                    return Err(WorkflowError::new(
                        ErrorCode::Invalid,
                        format!(
                            "this workflow's input requires {:?} ({})",
                            field.name,
                            field.kind.tag()
                        ),
                    ))
                }
                None => {}
                Some(value) if !field.kind.admits(value) => {
                    return Err(WorkflowError::new(
                        ErrorCode::Invalid,
                        format!(
                            "this workflow's input field {:?} is a {}, and this one is {value}",
                            field.name,
                            field.kind.tag()
                        ),
                    ))
                }
                Some(_) => {}
            }
        }
        for name in input.keys() {
            if !self.fields.iter().any(|field| &field.name == name) {
                let declared: Vec<&str> = self
                    .fields
                    .iter()
                    .map(|field| field.name.as_str())
                    .collect();
                return Err(WorkflowError::new(
                    ErrorCode::Invalid,
                    format!(
                        "this workflow declares no input field {name:?}; it declares {}",
                        if declared.is_empty() {
                            "none".to_owned()
                        } else {
                            declared.join(" | ")
                        }
                    ),
                ));
            }
        }
        Ok(())
    }
}

/// One workflow as it is asked for: the reusable HOW.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkflowSpec {
    #[serde(default)]
    pub api_version: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub nodes: Vec<NodeSpec>,
    #[serde(default)]
    pub edges: Vec<EdgeSpec>,
    #[serde(default)]
    pub input: InputSchema,
    #[serde(default, flatten)]
    pub attribution: Attribution,
    /// Operator metadata, carried verbatim and never interpreted.
    #[serde(default)]
    pub metadata: Extensions,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl WorkflowSpec {
    /// The spec as a peer receives it back, with this version stamped on.
    #[must_use]
    pub fn versioned(mut self) -> Self {
        self.api_version = API_VERSION.to_owned();
        self
    }

    /// One node by id.
    #[must_use]
    pub fn node(&self, id: &str) -> Option<&NodeSpec> {
        self.nodes.iter().find(|node| node.id == id)
    }

    /// The nodes with no inbound edge — where a run starts. A graph with
    /// none is refused by [`Self::check`], so a started run always has
    /// something to start.
    #[must_use]
    pub fn entry_nodes(&self) -> Vec<&NodeSpec> {
        self.nodes
            .iter()
            .filter(|node| !self.edges.iter().any(|edge| edge.to == node.id))
            .collect()
    }

    /// A spec a store will accept.
    ///
    /// The acyclicity check is the one that earns its keep: a cyclic
    /// workflow would drive its own nodes forever, and the kernel's own
    /// cycle refusal (jinnd M2-K10) does not reach here — nothing in this
    /// seam DISPATCHES around the loop, the loop would be inside one
    /// store's own bookkeeping. So the graph is proven acyclic where it
    /// is defined, once, rather than discovered at run time.
    ///
    /// # Errors
    ///
    /// A blank name, no nodes, a duplicate or blank node id, an edge
    /// naming a node that is not here, a self-edge, a cycle, no entry
    /// node, a `dispatch` node with no binding, a `checkpoint` node with
    /// one, a dispatch node whose Todo store is blank, or a blank actor.
    pub fn check(&self) -> Result<(), WorkflowError> {
        let invalid = |message: String| WorkflowError::new(ErrorCode::Invalid, message);
        if self.name.trim().is_empty() {
            return Err(invalid(
                "a workflow's `name` is what the company reads it by; it cannot be blank".into(),
            ));
        }
        if self.nodes.is_empty() {
            return Err(invalid(
                "a workflow with no nodes is not a procedure; it records nothing to do".into(),
            ));
        }
        for (index, node) in self.nodes.iter().enumerate() {
            if node.id.trim().is_empty() {
                return Err(invalid(
                    "a node's `id` cannot be blank; edges name it".into(),
                ));
            }
            if self.nodes[..index].iter().any(|other| other.id == node.id) {
                return Err(invalid(format!(
                    "two nodes share the id {:?}; an edge naming it would be ambiguous",
                    node.id
                )));
            }
            match (node.kind, &node.todo) {
                (NodeKind::Dispatch, None) => {
                    return Err(invalid(format!(
                        "node {:?} is a `dispatch` and carries no `todo` binding, \
                         so there is nothing for it to dispatch",
                        node.id
                    )))
                }
                (NodeKind::Dispatch, Some(binding)) if binding.store.trim().is_empty() => {
                    return Err(invalid(format!(
                        "node {:?} names no Todo store, and a store id is half of the \
                         contract name it would be dispatched through",
                        node.id
                    )))
                }
                (NodeKind::Checkpoint, Some(_)) => {
                    return Err(invalid(format!(
                        "node {:?} is a `checkpoint` and carries a `todo` binding that \
                         nothing would ever dispatch",
                        node.id
                    )))
                }
                _ => {}
            }
        }
        for edge in &self.edges {
            if self.node(&edge.from).is_none() {
                return Err(invalid(format!(
                    "an edge leaves node {:?}, which is not in this workflow",
                    edge.from
                )));
            }
            if self.node(&edge.to).is_none() {
                return Err(invalid(format!(
                    "an edge reaches node {:?}, which is not in this workflow",
                    edge.to
                )));
            }
            if edge.from == edge.to {
                return Err(invalid(format!(
                    "node {:?} has an edge to itself, which is a node that waits for \
                     itself to finish",
                    edge.from
                )));
            }
        }
        // Acyclicity first: a cycle is ALSO a graph with no entry node,
        // and "there is a cycle, here it is" is the answer that tells an
        // author what to fix.
        self.check_acyclic()?;
        if self.entry_nodes().is_empty() {
            return Err(invalid(
                "every node in this workflow has an inbound edge, so a run would have \
                 nothing to start with"
                    .into(),
            ));
        }
        self.attribution.check().map(|_| ())
    }

    /// Kahn's algorithm: a graph whose every node can be removed in
    /// dependency order has no cycle, and one that stalls names what is
    /// left.
    fn check_acyclic(&self) -> Result<(), WorkflowError> {
        let mut remaining: Vec<&str> = self.nodes.iter().map(|node| node.id.as_str()).collect();
        loop {
            let ready: Vec<&str> = remaining
                .iter()
                .copied()
                .filter(|id| {
                    !self
                        .edges
                        .iter()
                        .any(|edge| edge.to == *id && remaining.contains(&edge.from.as_str()))
                })
                .collect();
            if ready.is_empty() {
                break;
            }
            remaining.retain(|id| !ready.contains(id));
            if remaining.is_empty() {
                return Ok(());
            }
        }
        Err(WorkflowError::new(
            ErrorCode::Invalid,
            format!(
                "these nodes are in a cycle and no run could ever finish them: {}",
                remaining.join(", ")
            ),
        ))
    }
}

/// `define`: record a workflow, or a NEW REVISION of one already here.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DefineRequest {
    pub spec: WorkflowSpec,
    /// The workflow this defines a new revision OF. Absent records a new
    /// workflow. A revision never replaces the one before it — see
    /// `crate::revision`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `define` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkflowDefined {
    #[serde(default)]
    pub api_version: String,
    pub workflow_id: String,
    pub store: String,
    /// The revision this definition became. Monotone from 1.
    pub revision: u64,
    /// The revision's digest — see `crate::revision`.
    pub spec_digest: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `{ "workflow-id": ... }` document.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkflowRequest {
    pub workflow_id: String,
    /// Which revision to read. Absent reads the LATEST — the answer says
    /// which one that was, so a reader is never left guessing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// `start`: open a run of one workflow. The run is PINNED to the
/// revision resolved here, and to nothing else, for its whole life.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StartRequest {
    pub workflow_id: String,
    /// Which revision this run executes. Absent pins the LATEST AT THIS
    /// MOMENT — and the run records which one that was, so "latest" is
    /// resolved once, at start, and never again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    /// The run's input, checked against the pinned revision's schema.
    #[serde(default)]
    pub input: Extensions,
    #[serde(default, flatten)]
    pub attribution: Attribution,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `{ "run-id": ... }` document. ONE shape names a run, and every
/// operation that takes one reads it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RunRequest {
    pub run_id: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// `cancel`: stop a run, on the record.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CancelRequest {
    pub run_id: String,
    /// Why. A cancellation with no reason is an ending nobody can
    /// explain, so one is required — blank is refused.
    #[serde(default)]
    pub reason: String,
    #[serde(default, flatten)]
    pub attribution: Attribution,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// `list-runs`. Every filter absent lists every run this store holds.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ListRunsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<crate::RunStatus>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// `events`: one page of a run's feed.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EventsRequest {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(flatten)]
    pub extra: Extensions,
}

jinn_settings::closed_value_space!(NodeKind, "a node's `kind`", {
    "checkpoint" => Self::Checkpoint,
    "dispatch" => Self::Dispatch,
});

jinn_settings::closed_value_space!(EdgeKind, "an edge's `kind`", {
    "always" => Self::Always,
    "on-done" => Self::OnDone,
    "on-not-done" => Self::OnNotDone,
});

jinn_settings::closed_value_space!(FieldKind, "an input field's `kind`", {
    "string" => Self::String,
    "number" => Self::Number,
    "bool" => Self::Bool,
});

jinn_settings::additive!(
    TodoBinding,
    NodeSpec,
    EdgeSpec,
    FieldSpec,
    InputSchema,
    WorkflowSpec,
    DefineRequest,
    WorkflowDefined,
    WorkflowRequest,
    StartRequest,
    RunRequest,
    CancelRequest,
    ListRunsRequest,
    EventsRequest,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeState;

    fn checkpoint(id: &str) -> NodeSpec {
        NodeSpec {
            id: id.to_owned(),
            kind: NodeKind::Checkpoint,
            ..NodeSpec::default()
        }
    }

    fn spec(nodes: Vec<NodeSpec>, edges: Vec<EdgeSpec>) -> WorkflowSpec {
        WorkflowSpec {
            name: "a procedure".to_owned(),
            nodes,
            edges,
            ..WorkflowSpec::default()
        }
    }

    fn edge(from: &str, to: &str) -> EdgeSpec {
        EdgeSpec {
            from: from.to_owned(),
            to: to.to_owned(),
            ..EdgeSpec::default()
        }
    }

    #[test]
    fn a_cycle_is_refused_where_the_workflow_is_defined() {
        let cyclic = spec(
            vec![checkpoint("a"), checkpoint("b")],
            vec![edge("a", "b"), edge("b", "a")],
        );
        let refused = cyclic.check().expect_err("a cycle");
        assert_eq!(refused.code, ErrorCode::Invalid);
        assert!(refused.message.contains("cycle"), "{}", refused.message);
        // And a chain of the same nodes is fine.
        assert!(
            spec(vec![checkpoint("a"), checkpoint("b")], vec![edge("a", "b")])
                .check()
                .is_ok()
        );
    }

    #[test]
    fn a_dispatch_node_with_nothing_to_dispatch_is_refused() {
        let bare = spec(
            vec![NodeSpec {
                id: "run-it".to_owned(),
                kind: NodeKind::Dispatch,
                ..NodeSpec::default()
            }],
            vec![],
        );
        let refused = bare.check().expect_err("no binding");
        assert!(refused.message.contains("dispatch"), "{}", refused.message);
        // And a checkpoint that carries one is refused too: a binding
        // nothing dispatches is a caller believing work is scheduled.
        let confused = spec(
            vec![NodeSpec {
                id: "gate".to_owned(),
                kind: NodeKind::Checkpoint,
                todo: Some(TodoBinding {
                    store: "default".to_owned(),
                    ..TodoBinding::default()
                }),
                ..NodeSpec::default()
            }],
            vec![],
        );
        assert!(confused.check().is_err());
    }

    #[test]
    fn an_edge_to_a_node_that_is_not_here_is_refused() {
        let dangling = spec(vec![checkpoint("a")], vec![edge("a", "b")]);
        let refused = dangling.check().expect_err("a dangling edge");
        assert!(refused.message.contains("\"b\""), "{}", refused.message);
    }

    #[test]
    fn a_graph_with_no_entry_node_could_never_start() {
        // Not a cycle, and still unstartable: every node has an inbound
        // edge only if there IS a cycle, so this is caught by the cycle
        // check. The entry check earns its keep on the empty-edge case.
        let none = spec(vec![], vec![]);
        assert!(none.check().is_err());
    }

    #[test]
    fn an_input_the_schema_does_not_declare_is_refused_not_carried() {
        let schema = InputSchema {
            fields: vec![FieldSpec {
                name: "ticket".to_owned(),
                kind: FieldKind::String,
                required: true,
                ..FieldSpec::default()
            }],
            ..InputSchema::default()
        };
        let mut input = Extensions::new();
        assert!(schema.check(&input).is_err(), "a required field is absent");
        input.insert("ticket".to_owned(), serde_json::json!(7));
        let wrong = schema.check(&input).expect_err("a number is not a string");
        assert!(wrong.message.contains("ticket"), "{}", wrong.message);
        input.insert("ticket".to_owned(), serde_json::json!("PLA-1"));
        assert!(schema.check(&input).is_ok());
        input.insert("extra".to_owned(), serde_json::json!("?"));
        let undeclared = schema.check(&input).expect_err("undeclared");
        assert!(
            undeclared.message.contains("extra"),
            "{}",
            undeclared.message
        );
    }

    #[test]
    fn an_edge_kind_decides_whether_a_lane_is_followed() {
        assert!(EdgeKind::Always.follows(NodeState::Done));
        assert!(EdgeKind::Always.follows(NodeState::Failed));
        assert!(EdgeKind::OnDone.follows(NodeState::Done));
        assert!(!EdgeKind::OnDone.follows(NodeState::Failed));
        assert!(!EdgeKind::OnDone.follows(NodeState::Interrupted));
        assert!(EdgeKind::OnNotDone.follows(NodeState::Failed));
        assert!(EdgeKind::OnNotDone.follows(NodeState::Interrupted));
        assert!(!EdgeKind::OnNotDone.follows(NodeState::Done));
    }

    #[test]
    fn a_node_kind_this_version_cannot_name_is_refused() {
        let refused = serde_json::from_value::<NodeKind>(serde_json::json!("subworkflow"))
            .expect_err("closed");
        assert!(refused.to_string().contains("subworkflow"), "{refused}");
    }

    #[test]
    fn a_blank_actor_is_refused_because_it_would_read_like_a_principal() {
        for blank in ["", "   "] {
            let attribution = Attribution {
                actor: Some(blank.to_owned()),
            };
            assert_eq!(
                attribution.check().expect_err("blank").code,
                ErrorCode::Invalid
            );
        }
        assert_eq!(Attribution::default().check().expect("absent"), None);
    }
}
