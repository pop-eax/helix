use ark_bls12_381::Fr;
use ir::lir::WireId;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BgwNodeId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BgwOp {
    Input { wire: WireId },
    Const { value: Fr },
    Add { a: BgwNodeId, b: BgwNodeId },
    Sub { a: BgwNodeId, b: BgwNodeId },
    Mul { a: BgwNodeId, b: BgwNodeId },
}

#[derive(Debug, Default, Clone)]
pub struct BgwProgram {
    pub nodes: Vec<BgwOp>,
    pub wire_to_node: HashMap<WireId, BgwNodeId>,
}

impl BgwProgram {
    pub fn push_node(&mut self, op: BgwOp) -> BgwNodeId {
        let id = BgwNodeId(self.nodes.len());
        self.nodes.push(op);
        id
    }

    pub fn set_wire_node(&mut self, wire: WireId, node: BgwNodeId) {
        self.wire_to_node.insert(wire, node);
    }

    pub fn get_wire_node(&self, wire: WireId) -> Option<BgwNodeId> {
        self.wire_to_node.get(&wire).copied()
    }
}
