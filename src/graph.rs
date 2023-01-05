use crate::{CompressedMap, Node, WayId};

impl CompressedMap {
    pub fn shortest_path(&self, start_pos: (f64, f64), end_ways: &[WayId]) -> Vec<Node> {
        let starting_node = self.find_starting_node(start_pos);
        todo!()
    }

    fn find_starting_node(&self, start_pos: (f64, f64)) -> (WayId, Node) {
        todo!()
    }
}
