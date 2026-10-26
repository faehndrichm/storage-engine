use core::fmt;
use std::sync::Arc;

use super::node::{BTreeNode, InternalNode, LeafNode};
use crate::buffer_pool::{InMemoryPageStore, PageError, PageStore};
use crate::page::{InternalPage, LeafPage, Page, PageHeader, PageId, PageKind};

impl LeafNode {
    fn new(config: &TreeConfig) -> Self {
        Self {
            keys: Vec::with_capacity(config.max_order - 1),
            right_leaf: None,
        }
    }
}

impl InternalNode {
    fn new(config: &TreeConfig) -> Self {
        Self {
            keys: Vec::with_capacity(config.max_order - 1),
            child_nodes: Vec::with_capacity(config.max_order),
        }
    }
}

pub struct BPlusTree {
    root_id: PageId,
    buffer_pool: Arc<dyn PageStore>,
    config: TreeConfig,
}
impl fmt::Debug for BPlusTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f)?;
        self.fmt_node(f, self.root_id, 0)
    }
}

impl BPlusTree {
    fn fmt_node(&self, f: &mut fmt::Formatter<'_>, node_id: PageId, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        match self.get_node(node_id) {
            Ok(BTreeNode::Leaf(leaf)) => writeln!(f, "{indent}Leaf {:?}", leaf.keys),
            Ok(BTreeNode::Internal(internal)) => {
                writeln!(f, "{indent}Internal {:?}", internal.keys)?;
                for child_id in internal.child_nodes {
                    self.fmt_node(f, child_id, depth + 1)?;
                }
                Ok(())
            }
            Err(e) => writeln!(f, "{indent}<unreadable page {node_id}: {e:?}>"),
        }
    }
}
#[derive(PartialEq, Debug)]
pub struct TreeConfig {
    max_order: usize,
    debug: bool,
}

impl TreeConfig {
    pub fn new(max_order: usize) -> Self {
        Self {
            max_order,
            debug: false,
        }
    }

    pub fn debug(max_order: usize, debug: bool) -> Self {
        Self { max_order, debug }
    }

    pub fn min_keys(&self) -> usize {
        (self.max_order + 1) / 2 - 1
    }

    // TODO: not used
    pub fn max_keys(&self) -> usize {
        self.max_order - 1
    }

    pub fn min_childs(&self) -> usize {
        (self.max_order + 1) / 2
    }

    pub fn max_childs(&self) -> usize {
        self.max_order
    }
}

impl BPlusTree {
    pub fn new(buffer_pool: Arc<dyn PageStore>, config: TreeConfig) -> Self {
        let root = BTreeNode::Leaf(LeafNode::new(&config));
        let root_id = buffer_pool.allocate_page(root.into_page()).unwrap();

        Self {
            root_id,
            buffer_pool,
            config,
        }
    }

    // TODO: dont know what to do with this right now
    // TODO: Maybe pass a Vec here, Arena should be internal only
    //pub fn from_root_with_arena(config: TreeConfig, root_id: PageId, arena: Arena) -> Self {
    //    Self {
    //        config,
    //        arena,
    //        root_id,
    //    }
    //}

    pub fn print(&self) {
        println!("B+ Tree:");
        self.print_node(self.root_id, 0);
        println!("End");
        println!("");
    }

    fn print_node(&self, node_id: PageId, depth: usize) {
        match self.get_node(node_id).unwrap() {
            BTreeNode::Leaf(leaf) => leaf.print(node_id, depth),
            BTreeNode::Internal(internal) => {
                let indent = "  ".repeat(depth);
                println!(
                    "{}Internal {:?}, node_id: {}",
                    indent, internal.keys, node_id
                );

                for child_id in internal.child_nodes {
                    self.print_node(child_id, depth + 1);
                }
            }
        }
    }

    pub fn print_config(&self) {
        println!("Tree Config: ");
        println!("Max Order: {}", self.config.max_order);
        println!("Max Childs: {}", self.config.max_childs());
        println!("Max Keys: {}", self.config.max_keys());
        println!("Min Childs: {}", self.config.min_childs());
        println!("Min Keys: {}", self.config.min_keys());
    }
}

// trait BTreeOperations {
//     fn insert(&mut self, value: u32, max_order: usize) -> Option<InsertSplit>;
//     fn find(&self, value: u32) -> bool;
//     fn delete(&self, value: u32);
//     fn range(&self, from: Option<u32>, to: Option<u32>);
// }

impl BPlusTree {
    fn get_node(&self, node_id: PageId) -> Result<BTreeNode, PageError> {
        let page = self.buffer_pool.get_page(node_id)?;
        let node = BTreeNode::from_page(&page, &self.config)?;
        Ok(node)
    }
    fn create_node(&mut self, node: BTreeNode) -> Result<PageId, PageError> {
        let page = node.into_page();
        let node_id = self.buffer_pool.allocate_page(page)?;
        Ok(node_id)
    }
    fn write_node(&mut self, node_id: PageId, node: &BTreeNode) -> Result<(), PageError> {
        let page = node.into_page();
        self.buffer_pool.write_page(node_id, page)?;
        Ok(())
    }

    fn free_node(&mut self, node_id: PageId) -> Result<(), PageError> {
        self.buffer_pool.free_page(node_id)?;
        Ok(())
    }
    pub fn find(&mut self, value: u32) -> bool {
        self.find_node(self.root_id, value)
    }

    fn find_node(&mut self, node_id: PageId, value: u32) -> bool {
        let node = self.get_node(node_id).unwrap();

        match node {
            BTreeNode::Leaf(leaf) => leaf.keys.contains(&value),
            BTreeNode::Internal(internal) => {
                let child_id = internal.find_child_node_id(value);
                self.find_node(child_id, value)
            }
        }
    }

    pub fn range(&mut self, from: Option<u32>, to: Option<u32>) -> Vec<u32> {
        let from = from.unwrap_or(u32::MIN);
        let to = to.unwrap_or(u32::MAX);
        self.range_node(self.root_id, from, to)
    }

    pub fn descend_to_leaf(&mut self, node_id: PageId, from: u32) -> PageId {
        let node = self.get_node(node_id).unwrap();
        match node {
            BTreeNode::Leaf(_) => node_id,
            BTreeNode::Internal(internal) => {
                let child_id = internal.find_child_node_id(from);
                self.descend_to_leaf(child_id, from)
            }
        }
    }

    fn range_node(&mut self, node_id: PageId, from: u32, to: u32) -> Vec<u32> {
        let mut values = Vec::with_capacity(1024);
        let mut current_node_id = Some(self.descend_to_leaf(node_id, from));
        let mut first = true;

        while let Some(cur) = current_node_id {
            let leaf = match self.get_node(cur).unwrap() {
                BTreeNode::Leaf(l) => l,
                _ => unreachable!(),
            };

            let start = if first {
                leaf.keys.partition_point(|&k| k < from)
            } else {
                0
            };
            first = false;

            let end = leaf.keys.partition_point(|&k| k <= to);
            values.extend_from_slice(&leaf.keys[start..end]);

            if end < leaf.keys.len() {
                break;
            }
            current_node_id = leaf.right_leaf;
        }

        values
    }

    pub fn insert(&mut self, value: u32) -> bool {
        if self.config.debug {
            println!("Insert: {}", value);
        }

        match self.insert_node(self.root_id, value) {
            InsertResult::Split(split) => {
                // handle root split, the tree height is increased here

                let old_root_id = self.root_id;
                let mut new_root_node = InternalNode::new(&self.config); // new root is a fresh node

                // Split node always inserted to the right
                new_root_node.child_nodes.push(old_root_id);
                new_root_node.child_nodes.push(split.split_node);
                new_root_node.keys.push(split.seperator_key);

                // assign new root
                let new_root_id = self
                    .create_node(BTreeNode::Internal(new_root_node))
                    .unwrap();
                self.root_id = new_root_id;
                true
            }
            InsertResult::Inserted => true,
            InsertResult::DuplicateKey => false,
        }
    }

    fn insert_node(&mut self, node_id: PageId, value: u32) -> InsertResult {
        let node = self.get_node(node_id).unwrap();
        let index = node.find_child_index(value);

        // handle leaf case first
        let mut internal = match node {
            BTreeNode::Leaf(mut leaf) => {
                if leaf.keys.contains(&value) {
                    return InsertResult::DuplicateKey;
                }

                leaf.keys.insert(index, value);
                if leaf.keys.len() < self.config.max_order {
                    self.write_node(node_id, &BTreeNode::Leaf(leaf)).unwrap();
                    return InsertResult::Inserted;
                }

                // Create a leaf split
                let mut split_node = LeafNode::new(&self.config);
                split_node.keys = leaf.keys.split_off(self.config.max_order / 2);

                // Connect the new node to the previous right neighbour
                split_node.right_leaf = leaf.right_leaf;
                let separator_key = split_node.keys[0];
                let split_node_id = self.create_node(BTreeNode::Leaf(split_node)).unwrap();

                // Link the current leaf to the new one
                leaf.right_leaf = Some(split_node_id);
                self.write_node(node_id, &BTreeNode::Leaf(leaf)).unwrap();

                return InsertResult::Split(InsertSplit {
                    seperator_key: separator_key,
                    split_node: split_node_id,
                });
            }
            BTreeNode::Internal(internal) => internal,
        };

        let split = match self.insert_node(internal.child_nodes[index], value) {
            InsertResult::Split(s) => s,
            other => return other,
        };

        // Insert the split node, TODO: maybe we can do the split logic from the leaf branch here
        internal.child_nodes.insert(index + 1, split.split_node);
        internal.keys.insert(index, split.seperator_key);

        let internal_keys_after_insert = internal.keys.len();

        if internal_keys_after_insert < self.config.max_order {
            self.write_node(node_id, &BTreeNode::Internal(internal))
                .unwrap();
            return InsertResult::Inserted;
        }

        // When splitting another overflow can occur, need another split
        let mut internal_split_node = InternalNode::new(&self.config);
        let internal_split_index = (self.config.max_order / 2) + 1;
        internal_split_node.keys = internal.keys.split_off(internal_split_index);
        internal_split_node.child_nodes = internal.child_nodes.split_off(internal_split_index);

        let seperator_key = internal.keys.pop().unwrap();
        let split_node_id = self
            .create_node(BTreeNode::Internal(internal_split_node))
            .unwrap();
        self.write_node(node_id, &BTreeNode::Internal(internal))
            .unwrap();

        InsertResult::Split(InsertSplit {
            seperator_key,
            split_node: split_node_id,
        })
    }

    pub fn delete(&mut self, value: u32) -> bool {
        if self.config.debug {
            println!("Delete: {}", value);
        }

        match self.delete_node(self.root_id, value) {
            DeleteResult::NotFound => false,
            // a new minimum is dropped at the root, there is no separator that points at it
            DeleteResult::Rebalance | DeleteResult::RebalanceNewMinimum(_) => {
                let root = self.get_node(self.root_id).unwrap();

                if let BTreeNode::Internal(mut root_internal) = root
                    && root_internal.child_nodes.len() == 1
                {
                    // Replace root with its only child
                    let old_root_id = self.root_id;
                    self.root_id = root_internal.child_nodes.remove(0);
                    self.free_node(old_root_id).unwrap();
                }
                true
            }
            _ => true,
        }
    }

    // possible nodes we need to mutate:
    //        [parent]
    //      /    |    \
    // [left] [child] [right]
    fn rebalance(&mut self, parent: &mut InternalNode, index: usize) -> DeleteResult {
        let child_node_id = parent.child_nodes[index];
        let mut child = self.get_node(child_node_id).unwrap();

        let mut left = (index > 0).then(|| {
            let id = parent.child_nodes[index - 1];
            let parent_key = parent.keys[index - 1];
            (id, parent_key, self.get_node(id).unwrap())
        });

        // try to borrow from left sibling
        if let Some((left_id, parent_left_key, ref mut left_sibling)) = left
            && left_sibling.get_keys().len() > self.config.min_keys()
        {
            let borrow_key = child.borrow_from_left(left_sibling, parent_left_key);

            parent.keys[index - 1] = borrow_key;

            // TODO: check when to write which node, also resolve this by working with pages directly -> we write to the page, page is hanlded by buffer_pool
            self.write_node(left_id, left_sibling).unwrap();
            self.write_node(child_node_id, &child).unwrap();

            // we cannot create a new underflow, since the parent keys are only updated
            return DeleteResult::Deleted;
        }

        let mut right = (index + 1 < parent.child_nodes.len()).then(|| {
            let id = parent.child_nodes[index + 1];
            let parent_key = parent.keys[index];
            (id, parent_key, self.get_node(id).unwrap())
        });

        // try to borrow from right sibling
        if let Some((right_id, parent_right_key, ref mut right_sibling)) = right
            && right_sibling.get_keys().len() > self.config.min_keys()
        {
            let update_seperator_key = child.borrow_from_right(right_sibling, parent_right_key);

            parent.keys[index] = update_seperator_key;

            if index > 0
                && let BTreeNode::Leaf(leaf) = &child
            {
                parent.keys[index - 1] = leaf.keys[0];
            }

            // TODO: check when to write which node, also resolve this by working with pages directly -> we write to the page, page is hanlded by buffer_pool
            self.write_node(right_id, right_sibling).unwrap();
            self.write_node(child_node_id, &child).unwrap();

            if index == 0
                && let BTreeNode::Leaf(leaf) = &child
            {
                return DeleteResult::NewMinimum(leaf.keys[0]);
            }

            return DeleteResult::Deleted;
        }

        // try merging with the left sibling
        if let Some((left_sibling_id, parent_left_key, ref mut left_sibling)) = left {
            left_sibling.merge_right_into(&mut child, parent_left_key);

            parent.child_nodes.remove(index);
            parent.keys.remove(index - 1); // remove parent_left_key

            // TODO: check when to write which node, also resolve this by working with pages directly -> we write to the page, page is hanlded by buffer_pool
            self.write_node(left_sibling_id, left_sibling).unwrap();
            self.free_node(child_node_id).unwrap();
            if parent.keys.len() < self.config.min_keys() {
                return DeleteResult::Rebalance;
            }

            return DeleteResult::Deleted;
        }

        // try merging with the right sibling
        if let Some((right_id, parent_right_key, ref mut right_sibling)) = right {
            child.merge_right_into(right_sibling, parent_right_key);

            parent.child_nodes.remove(index + 1);
            parent.keys.remove(index); // remove parent_right_key

            // TODO: check when to write which node, also resolve this by working with pages directly -> we write to the page, page is hanlded by buffer_pool
            self.write_node(child_node_id, &child).unwrap();
            self.free_node(right_id).unwrap();

            let result = if parent.keys.len() < self.config.min_keys() {
                DeleteResult::Rebalance
            } else {
                DeleteResult::Deleted
            };

            if index == 0
                && let BTreeNode::Leaf(leaf) = &child
            {
                return result.with_new_minimum(leaf.keys[0]);
            }

            return result;
        }

        unreachable!("internal node without siblings, tree invariant violated");
    }

    fn delete_node(&mut self, node_id: PageId, value: u32) -> DeleteResult {
        let node = self.get_node(node_id).unwrap();
        let index = node.find_child_index(value);

        let mut internal = match node {
            BTreeNode::Leaf(mut leaf) => {
                let Some(index) = leaf.keys.iter().position(|&k| k == value) else {
                    return DeleteResult::NotFound;
                };

                leaf.keys.remove(index);

                let leaf_keys = leaf.keys.clone();

                self.write_node(node_id, &BTreeNode::Leaf(leaf)).unwrap();

                let min = self.config.min_keys();

                // TODO: check if we should update the parent seperator, when we do not rebalance?
                return match (leaf_keys.len() < min, index == 0) {
                    (true, _) => DeleteResult::Rebalance,
                    (false, true) => {
                        let del_key = leaf_keys[0];
                        DeleteResult::NewMinimum(del_key)
                    }
                    (false, false) => DeleteResult::Deleted,
                };
            }
            BTreeNode::Internal(internal) => internal,
        };

        let (result, dirty) = match self.delete_node(internal.child_nodes[index], value) {
            DeleteResult::RebalanceNewMinimum(new_min) => {
                // apply the minimum first, a merge pulls the separator down
                if index > 0 {
                    internal.keys[index - 1] = new_min;
                    (self.rebalance(&mut internal, index), true)
                } else {
                    let result = self.rebalance(&mut internal, index);
                    (result.with_new_minimum(new_min), true)
                }
            }
            DeleteResult::Rebalance => (self.rebalance(&mut internal, index), true),
            DeleteResult::NewMinimum(new_min) => {
                if index > 0 {
                    internal.keys[index - 1] = new_min;
                    (DeleteResult::Deleted, true)
                } else {
                    (DeleteResult::NewMinimum(new_min), false)
                }
            }
            other => (other, false),
        };

        if dirty {
            self.write_node(node_id, &BTreeNode::Internal(internal))
                .unwrap();
        }

        result
    }
}

impl BPlusTree {
    fn nodes_equal(&self, self_id: PageId, other: &Self, other_id: PageId) -> bool {
        let Ok(self_node) = self.get_node(self_id) else {
            return false;
        };
        let Ok(other_node) = other.get_node(other_id) else {
            return false;
        };

        match (self_node, other_node) {
            (BTreeNode::Leaf(self_leaf), BTreeNode::Leaf(other_leaf)) => {
                self_leaf.keys == other_leaf.keys
            }
            (BTreeNode::Internal(self_internal), BTreeNode::Internal(other_internal)) => {
                self_internal.keys == other_internal.keys
                    && self_internal.child_nodes.len() == other_internal.child_nodes.len()
                    && self_internal
                        .child_nodes
                        .iter()
                        .zip(other_internal.child_nodes.iter())
                        .all(|(self_child_id, other_child_id)| {
                            self.nodes_equal(*self_child_id, other, *other_child_id)
                        })
            }
            _ => false,
        }
    }
}

impl PartialEq for BPlusTree {
    fn eq(&self, other: &Self) -> bool {
        self.config == other.config && self.nodes_equal(self.root_id, other, other.root_id)
    }
}

struct InsertSplit {
    seperator_key: u32,
    split_node: PageId,
}

enum DeleteResult {
    NotFound,
    Deleted,
    NewMinimum(u32),
    Rebalance,
    RebalanceNewMinimum(u32),
}

impl DeleteResult {
    fn with_new_minimum(self, new_min: u32) -> Self {
        match self {
            DeleteResult::Deleted => DeleteResult::NewMinimum(new_min),
            DeleteResult::Rebalance => DeleteResult::RebalanceNewMinimum(new_min),
            other => other,
        }
    }
}

enum InsertResult {
    Inserted,
    Split(InsertSplit),
    DuplicateKey,
}

impl BTreeNode {
    pub fn from_page(page: &Page, config: &TreeConfig) -> Result<Self, PageError> {
        let header = PageHeader::decode(page.to_bytes());

        match header.kind {
            PageKind::BtreeInternal => {
                let mut node = InternalNode::new(config);
                let node_view = InternalPage::new(page);

                let key_count = node_view.key_count();
                for i in 0..key_count {
                    node.keys.push(node_view.key(i));
                    node.child_nodes.push(node_view.child_node(i));
                }
                node.child_nodes.push(node_view.child_node(key_count)); // +1 childs

                Ok(BTreeNode::Internal(node))
            }
            PageKind::BtreeLeaf => {
                let mut node = LeafNode::new(config);
                let node_view = LeafPage::new(page);

                let key_count = node_view.key_count();
                node.right_leaf = node_view.right_leaf();

                for i in 0..key_count {
                    node.keys.push(node_view.key(i));
                }
                Ok(BTreeNode::Leaf(node))
            }
            other => Err(PageError::UnexpectedPageKind(other)),
        }
    }

    pub fn into_page(&self) -> Page {
        match self {
            BTreeNode::Leaf(leaf_node) => {
                let mut page = Page::new(PageKind::BtreeLeaf);
                let mut node_view = LeafPage::new(&mut page);

                node_view.set_key_count(leaf_node.keys.len());
                node_view.set_right_leaf(leaf_node.right_leaf);

                for (i, &key) in leaf_node.keys.iter().enumerate() {
                    node_view.set_key(i, key);
                }

                page
            }
            BTreeNode::Internal(internal_node) => {
                let mut page = Page::new(PageKind::BtreeInternal);
                let mut node_view = InternalPage::new(&mut page);

                node_view.set_key_count(internal_node.keys.len());

                for (i, &key) in internal_node.keys.iter().enumerate() {
                    node_view.set_key(i, key);
                }

                for (i, &child_id) in internal_node.child_nodes.iter().enumerate() {
                    node_view.set_child_node(i, child_id);
                }

                page
            }
        }
    }

    pub fn get_keys(&self) -> &Vec<u32> {
        match self {
            BTreeNode::Leaf(node) => node.get_keys(),
            BTreeNode::Internal(node) => node.get_keys(),
        }
    }

    pub fn find_child_index(&self, value: u32) -> usize {
        self.get_keys().partition_point(|&k| k <= value)
    }

    /// For the -> direction, the borrowed key equals the seperator, since it will become the first key of the right child. (Leaf case).
    fn borrow_from_left(&mut self, left_sibling: &mut BTreeNode, parent_left_key: u32) -> u32 {
        match (left_sibling, self) {
            (BTreeNode::Leaf(left), BTreeNode::Leaf(current)) => {
                let borrow_key = left.keys.pop().unwrap();
                current.keys.insert(0, borrow_key);
                borrow_key
            }
            (BTreeNode::Internal(left), BTreeNode::Internal(current)) => {
                let borrow_key = left.keys.pop().unwrap();
                let borrow_child = left.child_nodes.pop().unwrap();

                // when we do internal borrows, we need to rotate the seperator keys
                current.keys.insert(0, parent_left_key);
                current.child_nodes.insert(0, borrow_child);
                borrow_key
            }
            _ => unreachable!("Leaf/Internal mismatch is impossible here"),
        }
    }

    /// For the <- direction, the borrowed key is different from the seperator to update (Leaf case).
    fn borrow_from_right(&mut self, right_sibling: &mut BTreeNode, parent_right_key: u32) -> u32 {
        match (self, right_sibling) {
            (BTreeNode::Leaf(current), BTreeNode::Leaf(right)) => {
                let borrow_key = right.keys.remove(0);
                let update_seperator_key = right.keys[0];

                current.keys.push(borrow_key);
                update_seperator_key
            }
            (BTreeNode::Internal(current), BTreeNode::Internal(right)) => {
                let update_seperator_key = right.keys.remove(0);
                let borrow_child = right.child_nodes.remove(0);

                current.keys.push(parent_right_key);
                current.child_nodes.push(borrow_child);
                update_seperator_key
            }
            _ => unreachable!("Leaf/Internal mismatch is impossible here"),
        }
    }

    /// Merges the right node into the current (current <- right)
    fn merge_right_into(&mut self, right_sibling: &mut BTreeNode, parent_right_key: u32) {
        match (self, right_sibling) {
            (BTreeNode::Leaf(current), BTreeNode::Leaf(right)) => {
                current.keys.append(&mut right.keys);
                current.right_leaf = right.right_leaf; // leaf chain stays intact
            }
            (BTreeNode::Internal(current), BTreeNode::Internal(right)) => {
                current.keys.push(parent_right_key);
                current.keys.append(&mut right.keys);

                current.child_nodes.append(&mut right.child_nodes);
            }
            _ => unreachable!("Leaf/Internal mismatch is impossible here"),
        };
    }

    // fn print(&self, node_id: PageId, depth: usize, arena: &Arena) {
    //     match self {
    //         BTreeNode::Leaf(node) => node.print(node_id, depth),
    //         BTreeNode::Internal(node) => node.print(depth, arena),
    //     }
    // }
}

impl InternalNode {
    pub fn get_keys(&self) -> &Vec<u32> {
        &self.keys
    }

    pub fn find_child_node_id(&self, value: u32) -> PageId {
        let index = self.keys.partition_point(|&k| k <= value);
        self.child_nodes[index]
    }
    pub fn find_child_index(&self, value: u32) -> usize {
        self.keys.partition_point(|&k| k <= value)
    }

    // fn print(&self, depth: usize, arena: &Arena) {
    //     let indent = "  ".repeat(depth);
    //     println!("{}Internal {:?}", indent, self.keys);

    //     for child_id in self.child_nodes.iter().copied() {
    //         let child = arena.get(child_id).unwrap();
    //         child.print(child_id, depth + 1, arena);
    //     }
    // }
}

impl LeafNode {
    pub fn get_keys(&self) -> &Vec<u32> {
        &self.keys
    }
    pub fn find_child_index(&self, value: u32) -> usize {
        self.keys.partition_point(|&k| k <= value)
    }

    fn print(&self, node_id: PageId, depth: usize) {
        let indent = "  ".repeat(depth);
        println!(
            "{}Leaf {:?}, node_id: {},  pointer: {:?}",
            indent, self.keys, node_id, self.right_leaf
        );
    }
}

#[derive(Clone, Copy)]
enum NodeKind {
    Leaf,
    Internal,
}

struct NodeBuilder {
    kind: NodeKind,
    keys: Vec<u32>,
    children: Vec<NodeBuilder>,
}

impl NodeBuilder {
    fn leaf(keys: impl IntoIterator<Item = u32>) -> Self {
        Self {
            kind: NodeKind::Leaf,
            keys: keys.into_iter().collect(),
            children: Vec::new(),
        }
    }

    fn internal(keys: impl IntoIterator<Item = u32>) -> Self {
        Self {
            kind: NodeKind::Internal,
            keys: keys.into_iter().collect(),
            children: Vec::new(),
        }
    }

    fn child(mut self, child: Self) -> Self {
        self.children.push(child);
        self
    }

    fn build(self, config: TreeConfig) -> BPlusTree {
        let store: Arc<dyn PageStore> = Arc::new(InMemoryPageStore::new());
        let mut leaves = Vec::new();
        let root_id = self.build_into(store.as_ref(), &mut leaves);

        for pair in leaves.windows(2) {
            let (left_id, right_id) = (pair[0], pair[1]);
            let page = store.get_page(left_id).unwrap();
            let BTreeNode::Leaf(mut leaf) = BTreeNode::from_page(&page, &config).unwrap() else {
                unreachable!("collected ids are leaves");
            };
            leaf.right_leaf = Some(right_id);
            store
                .write_page(left_id, BTreeNode::Leaf(leaf).into_page())
                .unwrap();
        }

        BPlusTree {
            root_id,
            buffer_pool: store,
            config,
        }
    }

    fn build_into(&self, store: &dyn PageStore, leaves: &mut Vec<PageId>) -> PageId {
        let node = match self.kind {
            NodeKind::Leaf => BTreeNode::Leaf(LeafNode {
                keys: self.keys.clone(),
                right_leaf: None,
            }),
            NodeKind::Internal => {
                let child_nodes = self
                    .children
                    .iter()
                    .map(|child| child.build_into(store, leaves))
                    .collect();
                BTreeNode::Internal(InternalNode {
                    keys: self.keys.clone(),
                    child_nodes,
                })
            }
        };

        let id = store.allocate_page(node.into_page()).unwrap();
        if let NodeKind::Leaf = self.kind {
            leaves.push(id);
        }
        id
    }

    fn from_list(values: Vec<u32>, config: TreeConfig) -> BPlusTree {
        let page_store = InMemoryPageStore::new();
        let mut btree = BPlusTree::new(Arc::new(page_store), config);

        for v in values {
            btree.insert(v);
        }
        btree
    }

    // fn build_into(&self, arena: &mut Arena) -> PageId {
    //     match self.kind {
    //         NodeKind::Leaf => arena.push(BTreeNode::Leaf(LeafNode {
    //             keys: self.keys.clone(),
    //             right_leaf: None,
    //         })),
    //         NodeKind::Internal => {
    //             let child_nodes = self
    //                 .children
    //                 .iter()
    //                 .map(|child| child.build_into(arena))
    //                 .collect();

    //             arena.push(BTreeNode::Internal(InternalNode {
    //                 keys: self.keys.clone(),
    //                 child_nodes,
    //             }))
    //         }
    //     }
    //                 }
}

fn build_main_tree() -> BPlusTree {
    let v = vec![3, 33, 45, 12, 75, 85, 32, 1, 23, 30, 34];
    let config = TreeConfig::new(4);
    let btree = NodeBuilder::from_list(v, config);

    btree
}
fn build_tree_example_2() -> BPlusTree {
    let v = vec![
        3, 33, 45, 12, 75, 85, 32, 1, 23, 30, 44, 41, 83, 25, 52, 76, 35, 62, 203, 63, 40, 24,
    ];
    let config = TreeConfig::new(4);
    let btree = NodeBuilder::from_list(v, config);

    btree
}

/// Internal[33]
/// ├── Internal[12, 30]
/// │   ├── Leaf[1, 3]
/// │   ├── Leaf[12, 23]
/// │   └── Leaf[30, 32]
/// └── Internal[75]
///     ├── Leaf[33, 34, 45]
///     └── Leaf[75, 85]
#[test]
fn test_insert_example_1() {
    let expected = NodeBuilder::internal([33])
        .child(
            NodeBuilder::internal([12, 30])
                .child(NodeBuilder::leaf([1, 3]))
                .child(NodeBuilder::leaf([12, 23]))
                .child(NodeBuilder::leaf([30, 32])),
        )
        .child(
            NodeBuilder::internal([75])
                .child(NodeBuilder::leaf([33, 34, 45]))
                .child(NodeBuilder::leaf([75, 85])),
        )
        .build(TreeConfig::new(4));

    assert_eq!(build_main_tree(), expected);
}

#[test]
fn test_find_inserted_values() {
    let mut btree = build_main_tree();
    for &val in &[3u32, 33, 45, 12, 75, 85, 32, 1, 23, 30, 34] {
        assert!(btree.find(val), "Expected to find {val} in tree");
    }
}

// B+ Tree:
// Internal [6]
//   Internal [5]
//     Leaf [4]
//     Leaf [5]
//   Internal [8]
//     Leaf [6, 7]
//     Leaf [8, 9]
// End
#[test]
fn test_root_split() {
    let mut tree = NodeBuilder::internal([6])
        .child(
            NodeBuilder::internal([5])
                .child(NodeBuilder::leaf([4]))
                .child(NodeBuilder::leaf([5])),
        )
        .child(
            NodeBuilder::internal([8])
                .child(NodeBuilder::leaf([6, 7]))
                .child(NodeBuilder::leaf([8, 9])),
        )
        .build(TreeConfig::new(4));

    let expected = NodeBuilder::internal([6, 8])
        .child(NodeBuilder::leaf([5]))
        .child(NodeBuilder::leaf([6, 7]))
        .child(NodeBuilder::leaf([8, 9]))
        .build(TreeConfig::new(4));

    tree.delete(4);

    assert_eq!(tree, expected);
}

// Merging two internal nodes, should pull down the parent key.
//
// B+ Tree:
// Internal [20, 50]
//   Internal [10]
//     Leaf [5]
//     Leaf [10]
//   Internal [30]
//     Leaf [20, 25]
//     Leaf [30, 35]
//   Internal [60]
//     Leaf [50, 55]
//     Leaf [60, 65]
// End
#[test]
fn test_delete_internal_merge_preserves_separator_key() {
    let mut tree = NodeBuilder::internal([20, 50])
        .child(
            NodeBuilder::internal([10])
                .child(NodeBuilder::leaf([5]))
                .child(NodeBuilder::leaf([10])),
        )
        .child(
            NodeBuilder::internal([30])
                .child(NodeBuilder::leaf([20, 25]))
                .child(NodeBuilder::leaf([30, 35])),
        )
        .child(
            NodeBuilder::internal([60])
                .child(NodeBuilder::leaf([50, 55]))
                .child(NodeBuilder::leaf([60, 65])),
        )
        .build(TreeConfig::new(4));

    tree.delete(5);

    let expected = NodeBuilder::internal([50])
        .child(
            NodeBuilder::internal([20, 30])
                .child(NodeBuilder::leaf([10]))
                .child(NodeBuilder::leaf([20, 25]))
                .child(NodeBuilder::leaf([30, 35])),
        )
        .child(
            NodeBuilder::internal([60])
                .child(NodeBuilder::leaf([50, 55]))
                .child(NodeBuilder::leaf([60, 65])),
        )
        .build(TreeConfig::new(4));

    assert_eq!(tree, expected);
    for &val in &[10u32, 20, 25, 30, 35, 50, 55, 60, 65] {
        assert!(tree.find(val), "lost key {val} after internal merge");
    }
}

// Merging two internal nodes, should pull down the parent key, when merging left to right.
//
// B+ Tree:
// Internal [20, 45]
//   Internal [5]
//     Leaf [1, 2]
//     Leaf [5, 6]
//   Internal [30]
//     Leaf [20, 25]
//     Leaf [30, 35]
//   Internal [50]
//     Leaf [45]
//     Leaf [50]
// End
#[test]
fn test_delete_internal_left_merge_preserves_separator_key() {
    let mut tree = NodeBuilder::internal([20, 45])
        .child(
            NodeBuilder::internal([5])
                .child(NodeBuilder::leaf([1, 2]))
                .child(NodeBuilder::leaf([5, 6])),
        )
        .child(
            NodeBuilder::internal([30])
                .child(NodeBuilder::leaf([20, 25]))
                .child(NodeBuilder::leaf([30, 35])),
        )
        .child(
            NodeBuilder::internal([50])
                .child(NodeBuilder::leaf([45]))
                .child(NodeBuilder::leaf([50])),
        )
        .build(TreeConfig::new(4));

    tree.delete(45);

    let expected = NodeBuilder::internal([20])
        .child(
            NodeBuilder::internal([5])
                .child(NodeBuilder::leaf([1, 2]))
                .child(NodeBuilder::leaf([5, 6])),
        )
        .child(
            NodeBuilder::internal([30, 50])
                .child(NodeBuilder::leaf([20, 25]))
                .child(NodeBuilder::leaf([30, 35]))
                .child(NodeBuilder::leaf([50])),
        )
        .build(TreeConfig::new(4));

    assert_eq!(tree, expected);
    for &val in &[1u32, 2, 5, 6, 20, 25, 30, 35, 50] {
        assert!(tree.find(val), "lost key {val} after internal left-merge");
    }
}

// B+ Tree:
// Internal [20]
//   Internal [10]
//     Leaf [5]
//     Leaf [10]
//   Internal [30, 40]
//     Leaf [20, 25]
//     Leaf [30, 35]
//     Leaf [40, 45]
// End
#[test]
fn test_delete_internal_borrow_from_right_uses_correct_separator() {
    let mut tree = NodeBuilder::internal([20])
        .child(
            NodeBuilder::internal([10])
                .child(NodeBuilder::leaf([5]))
                .child(NodeBuilder::leaf([10])),
        )
        .child(
            NodeBuilder::internal([30, 40])
                .child(NodeBuilder::leaf([20, 25]))
                .child(NodeBuilder::leaf([30, 35]))
                .child(NodeBuilder::leaf([40, 45])),
        )
        .build(TreeConfig::new(4));

    tree.delete(5);

    let expected = NodeBuilder::internal([30])
        .child(
            NodeBuilder::internal([20])
                .child(NodeBuilder::leaf([10]))
                .child(NodeBuilder::leaf([20, 25])),
        )
        .child(
            NodeBuilder::internal([40])
                .child(NodeBuilder::leaf([30, 35]))
                .child(NodeBuilder::leaf([40, 45])),
        )
        .build(TreeConfig::new(4));

    assert_eq!(tree, expected);
    for &val in &[10u32, 20, 25, 30, 35, 40, 45] {
        assert!(tree.find(val), "lost key {val} after internal borrow");
    }
}

// When the root collapses, we recompute the keys manually, we have to descend the whole tree,
// to get the leftmost key.
//
// B+ Tree:
// Internal [20]
//   Internal [5]
//     Leaf [1]
//     Leaf [5]
//   Internal [35]
//     Internal [25]
//       Leaf [20]
//       Leaf [25, 26]
//     Internal [40]
//       Leaf [35]
//       Leaf [40]
// End
#[test]
fn test_delete_root_collapse_recomputes_keys_from_leftmost_leaves() {
    let mut tree = NodeBuilder::internal([20])
        .child(
            NodeBuilder::internal([5])
                .child(NodeBuilder::leaf([1]))
                .child(NodeBuilder::leaf([5])),
        )
        .child(
            NodeBuilder::internal([35])
                .child(
                    NodeBuilder::internal([25])
                        .child(NodeBuilder::leaf([20]))
                        .child(NodeBuilder::leaf([25, 26])),
                )
                .child(
                    NodeBuilder::internal([40])
                        .child(NodeBuilder::leaf([35]))
                        .child(NodeBuilder::leaf([40])),
                ),
        )
        .build(TreeConfig::new(4));

    tree.delete(1);

    let expected = NodeBuilder::internal([20, 35])
        .child(NodeBuilder::leaf([5]))
        .child(
            NodeBuilder::internal([25])
                .child(NodeBuilder::leaf([20]))
                .child(NodeBuilder::leaf([25, 26])),
        )
        .child(
            NodeBuilder::internal([40])
                .child(NodeBuilder::leaf([35]))
                .child(NodeBuilder::leaf([40])),
        )
        .build(TreeConfig::new(4));

    assert_eq!(tree, expected);
    for &val in &[5u32, 20, 25, 26, 35, 40] {
        assert!(tree.find(val), "lost key {val} after root collapse");
    }
}

#[test]
fn test_range_full_scan_returns_sorted_all_keys() {
    let mut tree = build_tree_example_2();
    tree.print();
    let range = tree.range(None, None);
    let mut expected = vec![
        1, 3, 12, 23, 24, 25, 30, 32, 33, 35, 40, 41, 44, 45, 52, 62, 63, 75, 76, 83, 85, 203,
    ];
    expected.sort_unstable();
    assert_eq!(range, expected);
}

#[test]
fn test_range_bounded_returns_subset() {
    let mut tree = build_tree_example_2();
    // Keys in [12, 34]: 12, 23, 30, 32, 33, 34 (assuming inclusive bounds;
    // flip to exclusive per your confirmed range() semantics if needed).
    let range = tree.range(Some(12), Some(34));
    assert_eq!(range, vec![12, 23, 24, 25, 30, 32, 33]);
}

#[test]
fn test_range_no_matches_returns_empty() {
    let mut tree = build_tree_example_2();
    let range = tree.range(Some(1000), Some(2000));
    assert!(range.is_empty());
}

#[test]
fn test_range_single_key_bounds() {
    let mut tree = build_tree_example_2();
    let range = tree.range(Some(33), Some(33));
    assert_eq!(range, vec![33]);
}

#[test]
fn test_range_open_lower_bound() {
    let mut tree = build_tree_example_2();
    // Everything up to and including 12: 1, 3, 12
    let range = tree.range(None, Some(12));
    assert_eq!(range, vec![1, 3, 12]);
}

#[test]
fn test_range_open_upper_bound() {
    let mut tree = build_tree_example_2();
    // Everything from 75 onward: 75, 85
    let range = tree.range(Some(75), None);
    assert_eq!(range, vec![75, 76, 83, 85, 203]);
}

// Deleting the smallest key of a subtree that is the first child of a non-first child must
// update the separator in the higher ancestor, the update cannot stop at index 0.
//
// B+ Tree:
// Internal [20, 50]
//   Internal [10]
//     Leaf [5, 6]
//     Leaf [10, 11]
//   Internal [30]
//     Leaf [20, 25]   <- delete 20
//     Leaf [30, 35]
//   Internal [60]
//     Leaf [50, 55]
//     Leaf [60, 65]
// End
#[test]
fn test_delete_min_of_first_child_updates_ancestor_separator() {
    let mut tree = NodeBuilder::internal([20, 50])
        .child(
            NodeBuilder::internal([10])
                .child(NodeBuilder::leaf([5, 6]))
                .child(NodeBuilder::leaf([10, 11])),
        )
        .child(
            NodeBuilder::internal([30])
                .child(NodeBuilder::leaf([20, 25]))
                .child(NodeBuilder::leaf([30, 35])),
        )
        .child(
            NodeBuilder::internal([60])
                .child(NodeBuilder::leaf([50, 55]))
                .child(NodeBuilder::leaf([60, 65])),
        )
        .build(TreeConfig::new(4));

    assert!(tree.delete(20));

    let expected = NodeBuilder::internal([25, 50])
        .child(
            NodeBuilder::internal([10])
                .child(NodeBuilder::leaf([5, 6]))
                .child(NodeBuilder::leaf([10, 11])),
        )
        .child(
            NodeBuilder::internal([30])
                .child(NodeBuilder::leaf([25]))
                .child(NodeBuilder::leaf([30, 35])),
        )
        .child(
            NodeBuilder::internal([60])
                .child(NodeBuilder::leaf([50, 55]))
                .child(NodeBuilder::leaf([60, 65])),
        )
        .build(TreeConfig::new(4));

    assert_eq!(tree, expected);
}

// A leaf at index 0 underflows and borrows from its right sibling. Its minimum changes from
// the deleted key to the borrowed one, so the separator in the higher ancestor must follow.
//
// B+ Tree:
// Internal [20, 50]
//   Internal [10]
//     Leaf [5, 6]
//     Leaf [10, 11]
//   Internal [30]
//     Leaf [20]       <- delete 20, underflows and borrows 30 from the right
//     Leaf [30, 35]
//   Internal [60]
//     Leaf [50, 55]
//     Leaf [60, 65]
// End
#[test]
fn test_delete_borrow_from_right_at_index_0_updates_ancestor_separator() {
    let mut tree = NodeBuilder::internal([20, 50])
        .child(
            NodeBuilder::internal([10])
                .child(NodeBuilder::leaf([5, 6]))
                .child(NodeBuilder::leaf([10, 11])),
        )
        .child(
            NodeBuilder::internal([30])
                .child(NodeBuilder::leaf([20]))
                .child(NodeBuilder::leaf([30, 35])),
        )
        .child(
            NodeBuilder::internal([60])
                .child(NodeBuilder::leaf([50, 55]))
                .child(NodeBuilder::leaf([60, 65])),
        )
        .build(TreeConfig::new(4));

    assert!(tree.delete(20));

    let expected = NodeBuilder::internal([30, 50])
        .child(
            NodeBuilder::internal([10])
                .child(NodeBuilder::leaf([5, 6]))
                .child(NodeBuilder::leaf([10, 11])),
        )
        .child(
            NodeBuilder::internal([35])
                .child(NodeBuilder::leaf([30]))
                .child(NodeBuilder::leaf([35])),
        )
        .child(
            NodeBuilder::internal([60])
                .child(NodeBuilder::leaf([50, 55]))
                .child(NodeBuilder::leaf([60, 65])),
        )
        .build(TreeConfig::new(4));

    assert_eq!(tree, expected);
}

// An internal node underflows while its subtree minimum changes at the same time. The parent
// must apply the new minimum before it merges, since the merge pulls the separator down.
//
// Deleting 20 empties Leaf [20], it merges with Leaf [30] (the minimum of Internal [30] becomes
// 30) and Internal [30] is left with a single child, so it merges into its left sibling.
// The separator pulled down from the root must be 30, not the stale 20.
//
// B+ Tree:
// Internal [20, 50]
//   Internal [10]
//     Leaf [5, 6]
//     Leaf [10, 11]
//   Internal [30]
//     Leaf [20]       <- delete 20
//     Leaf [30]
//   Internal [60]
//     Leaf [50, 55]
//     Leaf [60, 65]
// End
#[test]
fn test_delete_rebalance_and_min_update_at_same_time() {
    let mut tree = NodeBuilder::internal([20, 50])
        .child(
            NodeBuilder::internal([10])
                .child(NodeBuilder::leaf([5, 6]))
                .child(NodeBuilder::leaf([10, 11])),
        )
        .child(
            NodeBuilder::internal([30])
                .child(NodeBuilder::leaf([20]))
                .child(NodeBuilder::leaf([30])),
        )
        .child(
            NodeBuilder::internal([60])
                .child(NodeBuilder::leaf([50, 55]))
                .child(NodeBuilder::leaf([60, 65])),
        )
        .build(TreeConfig::new(4));

    assert!(tree.delete(20));

    let expected = NodeBuilder::internal([50])
        .child(
            NodeBuilder::internal([10, 30])
                .child(NodeBuilder::leaf([5, 6]))
                .child(NodeBuilder::leaf([10, 11]))
                .child(NodeBuilder::leaf([30])),
        )
        .child(
            NodeBuilder::internal([60])
                .child(NodeBuilder::leaf([50, 55]))
                .child(NodeBuilder::leaf([60, 65])),
        )
        .build(TreeConfig::new(4));

    assert_eq!(tree, expected);
}

// A leaf at index > 0 underflows and borrows from its right sibling. Its first key changes from
// the deleted key to the borrowed one, so the parent separator in front of it must follow.
//
// B+ Tree:
// Internal [10, 30]
//   Leaf [5]
//   Leaf [10]         <- delete 10, underflows and borrows 30 from the right
//   Leaf [30, 35]
// End
#[test]
fn test_delete_borrow_from_right_at_index_gt_0_updates_own_separator() {
    let mut tree = NodeBuilder::internal([10, 30])
        .child(NodeBuilder::leaf([5]))
        .child(NodeBuilder::leaf([10]))
        .child(NodeBuilder::leaf([30, 35]))
        .build(TreeConfig::new(4));

    assert!(tree.delete(10));

    let expected = NodeBuilder::internal([30, 35])
        .child(NodeBuilder::leaf([5]))
        .child(NodeBuilder::leaf([30]))
        .child(NodeBuilder::leaf([35]))
        .build(TreeConfig::new(4));

    assert_eq!(tree, expected);
}
