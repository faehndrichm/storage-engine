use super::BTreeNode;

pub type NodeId = u32;

#[derive(PartialEq, Debug)]
pub struct Arena {
    items: Vec<BTreeNode>,
    len: NodeId,
}

impl Arena {
    pub fn new() -> Self {
        Arena {
            items: Vec::with_capacity(256),
            len: 0,
        }
    }

    pub fn push(&mut self, node: BTreeNode) -> NodeId {
        self.items.push(node);

        let len = self.len;
        self.len += 1;
        len
    }

    pub fn get(&self, node_id: NodeId) -> Option<&BTreeNode> {
        self.items.get(node_id as usize)
    }
    pub fn get_mut(&mut self, node_id: NodeId) -> Option<&mut BTreeNode> {
        self.items.get_mut(node_id as usize)
    }
}
