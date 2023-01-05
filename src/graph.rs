use crate::{CWayId, Map, Node, WayId};

impl Map {
    pub fn shortest_path(&self, gps_start: &Node, street: &str) -> Vec<Node> {
        let (starting_way, starting_node) = self.find_starting_node(gps_start);
        eprintln!("starting at {starting_node:?}");
        todo!()
    }

    fn find_starting_node(&self, gps_start: &Node) -> (CWayId, Node) {
        //TODO: fixme if between tiles
        //TODO: fixme if outside of grid
        //TODO: fixme if empty tile
        let tile_x = ((gps_start.x - self.start_coordinates.0) / self.side).floor() as usize;
        let tile_y = ((gps_start.y - self.start_coordinates.1) / self.side).floor() as usize;
        self.tile_ways(tile_x, tile_y)
            .enumerate()
            .flat_map(move |(way_id, way_nodes)| way_nodes.into_iter().map(move |n| (way_id, n)))
            .min_by(|(_, na), (_, nb)| {
                na.squared_distance_between(gps_start)
                    .partial_cmp(&nb.squared_distance_between(gps_start))
                    .unwrap()
            })
            .map(|(way_id, n)| {
                (
                    (
                        (tile_x + tile_y * self.tiles_per_line) as u16,
                        way_id as u16,
                    ),
                    n,
                )
            })
            .unwrap()
    }
}
