use crate::page::PageId;

#[derive(PartialEq, Debug)]
pub enum BTreeNode {
    Leaf(LeafNode),
    Internal(InternalNode),
}

#[derive(PartialEq, Debug)]
pub struct LeafNode {
    pub keys: Vec<u32>,
    pub right_leaf: Option<PageId>,
}

#[derive(PartialEq, Debug)]
pub struct InternalNode {
    pub keys: Vec<u32>,
    pub child_nodes: Vec<PageId>,
}
