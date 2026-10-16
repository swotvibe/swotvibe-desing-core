//! Consistent retained-allocation estimates for inverse commands, not process RSS.
use crate::{Command, Content, Extensions, Node, NodeId, NodeProps, Page};
use std::mem::size_of;

pub(crate) fn command_heap(command: &Command) -> usize {
    match command {
        Command::CreatePage { name, .. } | Command::CreateAsset { name, .. } => name.capacity(),
        Command::CreateNode { name, .. } | Command::RenameNode { name, .. } => {
            name.as_ref().map_or(0, String::capacity)
        }
        Command::SetNodeProps { props, .. } => {
            size_of::<NodeProps>().saturating_add(props_heap(props))
        }
        Command::ReorderTo { order, .. } => order.capacity().saturating_mul(size_of::<NodeId>()),
        Command::RestoreNode {
            node, extensions, ..
        } => size_of::<Node>()
            .saturating_add(node.name.as_ref().map_or(0, String::capacity))
            .saturating_add(node.children().len().saturating_mul(size_of::<NodeId>()))
            .saturating_add(props_heap(&node.props))
            .saturating_add(extensions_heap(extensions)),
        Command::RestorePage {
            page, extensions, ..
        } => size_of::<Page>()
            .saturating_add(page.name.capacity())
            .saturating_add(page.roots().len().saturating_mul(size_of::<NodeId>()))
            .saturating_add(extensions_heap(extensions)),
        Command::RestoreAsset {
            asset, extensions, ..
        } => asset
            .name
            .capacity()
            .saturating_add(extensions_heap(extensions)),
        Command::DeleteNode { .. }
        | Command::MoveNode { .. }
        | Command::ReorderNode { .. }
        | Command::RemovePage { .. }
        | Command::RemoveAsset { .. } => 0,
    }
}

fn props_heap(props: &NodeProps) -> usize {
    match &props.content {
        Content::Text(text) => text
            .content
            .capacity()
            .saturating_add(text.font_family.capacity()),
        _ => 0,
    }
}

fn extensions_heap(extensions: &Extensions) -> usize {
    // Map entries include their keys/values and a conservative link allowance.
    // Allocator internals and unused B-tree slots are not observable through serde_json.
    let entry = size_of::<String>()
        .saturating_add(size_of::<serde_json::Value>())
        .saturating_add(3 * size_of::<usize>());
    let mut total = extensions.len().saturating_mul(entry);
    let mut pending = Vec::new();
    for (key, value) in extensions {
        total = total.saturating_add(key.capacity());
        pending.push(value);
    }
    while let Some(value) = pending.pop() {
        match value {
            serde_json::Value::String(value) => total = total.saturating_add(value.capacity()),
            serde_json::Value::Array(values) => {
                total = total.saturating_add(
                    values
                        .capacity()
                        .saturating_mul(size_of::<serde_json::Value>()),
                );
                pending.extend(values);
            }
            serde_json::Value::Object(values) => {
                total = total.saturating_add(values.len().saturating_mul(entry));
                for (key, value) in values {
                    total = total.saturating_add(key.capacity());
                    pending.push(value);
                }
            }
            _ => {}
        }
    }
    total
}
