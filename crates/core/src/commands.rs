//! Typed commands and pre-application validation.
//!
//! Ownership: the commands and the validation performed before they apply.
//! Not owned: the user interface or prompt text.
//!
//! Every change — whether from a user, an agent, or an importer — must go
//! through a command that applies the same validation and policy.
//!
//! ## Contract (§6.1)
//!
//! Each command declares:
//!
//! - inputs validated by type, so a malformed request cannot be constructed;
//! - the scope of the elements it touches;
//! - preconditions, such as the target existing;
//! - a success result carrying any created identities;
//! - a stable error when it cannot be applied;
//! - enough information to build a safe inverse.
//!
//! ## Request versus effect
//!
//! A [`Command`] is a *request*: it names a target and new values. It is not a
//! record of what happened. When the engine applies a command it produces an
//! *effect* recording the values observed at apply time (see §6.4: the inverse
//! is generated from the actual state, never from the agent's requested
//! values).
//!
//! Keeping the two apart is what lets a command be replayed on a different
//! document and produce a *different but correct* inverse.

use std::fmt;

use crate::ids::{AssetId, NodeId, PageId};
use crate::model::NodeKind;

/// The stable, machine-readable code for a command failure.
///
/// Callers branch on this, never on the message. Codes are part of the public
/// contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CommandErrorCode {
    /// The target node does not exist.
    NodeNotFound,
    /// The target page does not exist.
    PageNotFound,
    /// The target asset does not exist.
    AssetNotFound,
    /// The target already exists where a fresh one is required.
    NodeAlreadyExists,
    /// The requested position is outside the parent's child range.
    IndexOutOfRange,
    /// The new parent would make the node its own ancestor.
    WouldCycle,
    /// The node kind does not permit children.
    KindCannotOwnChildren,
    /// The node is not a page root, or is already one.
    NotARoot,
    /// The requested identity is already taken.
    IdCollision,
    /// The command is well-formed but not legal in the current state.
    PreconditionFailed,
}

impl CommandErrorCode {
    /// The stable text form of the code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NodeNotFound => "node-not-found",
            Self::PageNotFound => "page-not-found",
            Self::AssetNotFound => "asset-not-found",
            Self::NodeAlreadyExists => "node-already-exists",
            Self::IndexOutOfRange => "index-out-of-range",
            Self::WouldCycle => "would-cycle",
            Self::KindCannotOwnChildren => "kind-cannot-own-children",
            Self::NotARoot => "not-a-root",
            Self::IdCollision => "id-collision",
            Self::PreconditionFailed => "precondition-failed",
        }
    }
}

impl fmt::Display for CommandErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A command failure with a stable code and the element it concerns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandError {
    /// The stable code the caller branches on.
    pub code: CommandErrorCode,
    /// The node the error is about, when the failure concerns a node.
    pub node: Option<NodeId>,
    /// The page the error is about, when the failure concerns a page.
    pub page: Option<PageId>,
    /// The asset the error is about, when the failure concerns an asset.
    pub asset: Option<AssetId>,
    /// A human-readable explanation. Not a programmatic key.
    pub message: String,
}

impl CommandError {
    /// Builds an error about a node.
    #[must_use]
    pub fn on_node(code: CommandErrorCode, id: NodeId, message: impl Into<String>) -> Self {
        Self {
            code,
            node: Some(id),
            page: None,
            asset: None,
            message: message.into(),
        }
    }

    /// Builds an error about a page.
    #[must_use]
    pub fn on_page(code: CommandErrorCode, id: PageId, message: impl Into<String>) -> Self {
        Self {
            code,
            node: None,
            page: Some(id),
            asset: None,
            message: message.into(),
        }
    }

    /// Builds an error about an asset.
    #[must_use]
    pub fn on_asset(code: CommandErrorCode, id: AssetId, message: impl Into<String>) -> Self {
        Self {
            code,
            node: None,
            page: None,
            asset: Some(id),
            message: message.into(),
        }
    }

    /// Builds an error with no specific target.
    #[must_use]
    pub fn general(code: CommandErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            node: None,
            page: None,
            asset: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]", self.code)?;
        if let Some(node) = self.node {
            write!(f, " node {node}")?;
        }
        if let Some(page) = self.page {
            write!(f, " page {page}")?;
        }
        if let Some(asset) = self.asset {
            write!(f, " asset {asset}")?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for CommandError {}

/// Where a node should sit relative to its siblings.
///
/// A position is always resolved against the parent's *current* children at
/// apply time, never stored as a raw offset that could drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// Insert before all existing siblings.
    First,
    /// Insert after all existing siblings.
    Last,
    /// Insert immediately before the given sibling.
    Before(NodeId),
    /// Insert immediately after the given sibling.
    After(NodeId),
    /// Insert at a raw index, clamped to the valid range.
    At(usize),
}

/// Where a node should live in the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodePlacement {
    /// As a root of the given page.
    PageRoot {
        /// The page.
        page: PageId,
        /// Where among the page's roots.
        position: Position,
    },
    /// As a child of the given node.
    Child {
        /// The parent node.
        parent: NodeId,
        /// Where among the parent's children.
        position: Position,
    },
}

impl NodePlacement {
    /// The position requested within the destination container.
    #[must_use]
    pub const fn position(self) -> Position {
        match self {
            Self::PageRoot { position, .. } | Self::Child { position, .. } => position,
        }
    }
}

/// A typed request to change the document.
///
/// `#[non_exhaustive]` so new command categories can be added without breaking
/// downstream matches; every consumer must therefore have a fallback arm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// Creates a page at the end of the page list.
    CreatePage {
        /// The identity to assign. The creator chooses it so the result is
        /// reproducible and so a caller can refer to it before applying.
        id: PageId,
        /// A human-readable name.
        name: String,
    },
    /// Registers a document-scoped asset.
    ///
    /// Assets carry only an identity and a name in M0; the file container and
    /// binary payload live above the kernel (`CORE-FORMAT-01`). The importer
    /// uses this so every path that grows a document still goes through one
    /// validated write surface.
    CreateAsset {
        /// The identity to assign.
        id: AssetId,
        /// A human-readable name.
        name: String,
    },
    /// Creates a node and places it under a parent or as a page root.
    CreateNode {
        /// The identity to assign.
        id: NodeId,
        /// The kind of node.
        kind: NodeKind,
        /// An optional human-readable name.
        name: Option<String>,
        /// Where the new node goes.
        parent: NodePlacement,
    },
    /// Removes a node and its whole subtree.
    DeleteNode {
        /// The node to remove.
        id: NodeId,
    },
    /// Moves a node to a new parent or position without changing its identity.
    MoveNode {
        /// The node to move.
        id: NodeId,
        /// The destination.
        parent: NodePlacement,
    },
    /// Renames a node. A name is not an identity and never affects ordering.
    RenameNode {
        /// The node to rename.
        id: NodeId,
        /// The new name, or `None` to clear it.
        name: Option<String>,
    },
    /// Reorders an existing child within its current parent.
    ReorderNode {
        /// The node to move among its siblings.
        id: NodeId,
        /// Where it should end up.
        position: Position,
    },
    /// Replaces a container's child order with an exact sequence.
    ///
    /// Exists for undo: inverting a reorder by position would be ambiguous when
    /// two siblings are interchangeable, whereas the recorded order restores
    /// the container exactly. The sequence must be a permutation of the
    /// container's current members.
    ReorderTo {
        /// The parent whose order is replaced, or `None` for a page's roots.
        parent: Option<NodeId>,
        /// The page that owns the container. Required when `parent` is `None`,
        /// and must match the parent's page otherwise.
        page: PageId,
        /// The exact order to install.
        order: Vec<NodeId>,
    },
    /// Removes a page that holds no nodes.
    ///
    /// Refuses a non-empty page rather than cascading, so an undo cannot
    /// silently discard content: the inverse of a page creation removes its
    /// nodes first and the page last.
    RemovePage {
        /// The page to remove.
        id: PageId,
    },
}

impl Command {
    /// A short, stable name for the command category.
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::CreatePage { .. } => "create-page",
            Self::CreateAsset { .. } => "create-asset",
            Self::CreateNode { .. } => "create-node",
            Self::DeleteNode { .. } => "delete-node",
            Self::MoveNode { .. } => "move-node",
            Self::RenameNode { .. } => "rename-node",
            Self::ReorderNode { .. } => "reorder-node",
            Self::ReorderTo { .. } => "reorder-to",
            Self::RemovePage { .. } => "remove-page",
        }
    }

    /// The identity of the primary node target, when the command has one.
    ///
    /// Used for diagnostics and for checks that need the target without
    /// matching on every variant.
    #[must_use]
    pub const fn target_node(&self) -> Option<NodeId> {
        match self {
            Self::CreatePage { .. } => None,
            Self::CreateAsset { .. } => None,
            Self::CreateNode { id, .. } => Some(*id),
            Self::DeleteNode { id }
            | Self::MoveNode { id, .. }
            | Self::RenameNode { id, .. }
            | Self::ReorderNode { id, .. } => Some(*id),
            Self::ReorderTo { .. } | Self::RemovePage { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_have_stable_names() {
        assert_eq!(CommandErrorCode::NodeNotFound.as_str(), "node-not-found");
        assert_eq!(CommandErrorCode::WouldCycle.as_str(), "would-cycle");
        assert_eq!(
            CommandErrorCode::KindCannotOwnChildren.as_str(),
            "kind-cannot-own-children"
        );
        assert_eq!(
            CommandErrorCode::IndexOutOfRange.to_string(),
            "index-out-of-range"
        );
    }

    #[test]
    fn a_command_error_displays_its_code_and_subject() {
        let node = NodeId::new();
        let error = CommandError::on_node(CommandErrorCode::NodeNotFound, node, "gone");
        let text = error.to_string();
        assert!(text.contains("node-not-found"), "got: {text}");
        assert!(text.contains(&node.to_string()), "got: {text}");
        assert!(text.contains("gone"), "got: {text}");
    }

    #[test]
    fn a_page_error_carries_the_page_not_a_node() {
        let page = PageId::new();
        let error = CommandError::on_page(CommandErrorCode::PageNotFound, page, "gone");
        assert_eq!(error.page, Some(page));
        assert_eq!(error.node, None);
    }

    #[test]
    fn a_general_error_carries_no_target() {
        let error = CommandError::general(CommandErrorCode::PreconditionFailed, "nope");
        assert!(error.node.is_none());
        assert!(error.page.is_none());
    }

    #[test]
    fn command_kind_names_are_stable() {
        let commands = [
            (
                Command::CreatePage {
                    id: PageId::new(),
                    name: "p".to_owned(),
                },
                "create-page",
            ),
            (
                Command::CreateNode {
                    id: NodeId::new(),
                    kind: NodeKind::Frame,
                    name: None,
                    parent: NodePlacement::PageRoot {
                        page: PageId::new(),
                        position: Position::Last,
                    },
                },
                "create-node",
            ),
            (Command::DeleteNode { id: NodeId::new() }, "delete-node"),
            (
                Command::MoveNode {
                    id: NodeId::new(),
                    parent: NodePlacement::PageRoot {
                        page: PageId::new(),
                        position: Position::First,
                    },
                },
                "move-node",
            ),
            (
                Command::RenameNode {
                    id: NodeId::new(),
                    name: Some("x".to_owned()),
                },
                "rename-node",
            ),
            (
                Command::ReorderNode {
                    id: NodeId::new(),
                    position: Position::Last,
                },
                "reorder-node",
            ),
        ];

        for (command, expected) in commands {
            assert_eq!(command.kind_name(), expected);
        }
    }

    #[test]
    fn create_page_has_no_node_target() {
        let command = Command::CreatePage {
            id: PageId::new(),
            name: "p".to_owned(),
        };
        assert_eq!(command.target_node(), None);
    }

    #[test]
    fn node_commands_expose_their_target() {
        let id = NodeId::new();
        assert_eq!(Command::DeleteNode { id }.target_node(), Some(id));
        assert_eq!(
            Command::RenameNode { id, name: None }.target_node(),
            Some(id)
        );
        assert_eq!(
            Command::ReorderNode {
                id,
                position: Position::First
            }
            .target_node(),
            Some(id)
        );
    }

    #[test]
    fn positions_compare_structurally() {
        let sibling = NodeId::new();
        assert_eq!(Position::First, Position::First);
        assert_ne!(Position::First, Position::Last);
        assert_eq!(Position::Before(sibling), Position::Before(sibling));
        assert_ne!(Position::Before(sibling), Position::After(sibling));
        assert_eq!(Position::At(3), Position::At(3));
    }
}
