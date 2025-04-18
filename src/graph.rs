use itertools::Itertools;
use std::f64::consts::PI;
use std::{
    collections::{BinaryHeap, HashMap, HashSet},
    io::Write,
};

use crate::{
    grid_coordinates_between, save_svg, CNodeId, CWayId, Map, Node, Svg, SvgW,
    TILE_BORDER_THICKNESS,
};

#[derive(Debug, Clone, Copy)]
struct GNode {
    id: CNodeId,
    node: Node,
}

impl std::fmt::Display for GNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}, {}]", self.x, self.y)
    }
}

impl<W: Write> Svg<W> for GNode {
    fn write_svg(&self, writer: &mut W, color: &str) -> std::io::Result<()> {
        self.node.write_svg(writer, color)
    }
}

impl std::ops::Deref for GNode {
    type Target = Node;

    fn deref(&self) -> &Self::Target {
        &self.node
    }
}

impl PartialEq for GNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for GNode {}

impl std::hash::Hash for GNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl Map {
    pub fn shortest_path(&self, gps_start: &Node, street: &str) -> Vec<Node> {
        let starting_node = self.find_starting_node(gps_start);
        let end_node = self.find_ending_node(gps_start, street);
        let greedy_path_length = self.greedy_path(&starting_node, &end_node);
        let path = self.low_level_a_star(&starting_node, &end_node, greedy_path_length);
        save_svg(
            "path.svg",
            self.bounding_box(),
            [
                self as SvgW,
                &starting_node as SvgW,
                &end_node as SvgW,
                (&path.as_slice()) as SvgW,
            ],
        )
        .unwrap();
        path
    }

    #[allow(dead_code)]
    fn a_star(&self, start: &GNode, end: &GNode, greedy_path_length: f64) -> Vec<Node> {
        let mut heap = BinaryHeap::new();
        heap.push(HeapEntry {
            predecessor: None,
            travel: [*start, *start],
            distance: 0.,
        });

        let mut seen_nodes = HashSet::new(); // TODO: replace by bitvec
        let mut predecessors = HashMap::new(); // TODO: replace with vec and renumbering of nodes
        while let Some(entry) = heap.pop() {
            if seen_nodes.contains(&entry.travel[1].id) {
                continue;
            }
            seen_nodes.insert(entry.travel[0].id);
            seen_nodes.insert(entry.travel[1].id);
            let current_node = entry.travel[1];
            if let Some(predecessor) = entry.predecessor {
                predecessors.insert(entry.travel[1], predecessor);
            }
            if current_node.is(end) {
                return rebuild_path(&current_node, &predecessors);
            }
            heap.extend(
                self.neighbours(&current_node)
                    .map(|travel| HeapEntry {
                        predecessor: Some(current_node),
                        travel,
                        distance: entry.distance + travel[0].distance_to(&travel[1]),
                    })
                    .filter(|entry| {
                        entry.distance + entry.travel[1].distance_to(end) < greedy_path_length
                    }),
            );
        }
        Vec::new()
    }

    fn low_level_a_star(&self, start: &GNode, end: &GNode, greedy_path_length: f64) -> Vec<Node> {
        let mut heap = BinaryHeap::new();
        heap.push(HeapEntry {
            predecessor: None,
            travel: [*start, *start],
            distance: 0.,
        });

        let mut seen_nodes = vec![0u8; 1 + self.binary_ways.len() / 16];
        let mut predecessors = Vec::new();
        while let Some(entry) = heap.pop() {
            let n1_offset_id = self.node_offset_id(&entry.travel[1].id);
            if (seen_nodes[n1_offset_id / 8] & (1u8 << (n1_offset_id % 8))) != 0 {
                continue;
            }
            let n0_offset_id = self.node_offset_id(&entry.travel[0].id);
            seen_nodes[n0_offset_id / 8] |= 1u8 << (n0_offset_id % 8);
            seen_nodes[n1_offset_id / 8] |= 1u8 << (n1_offset_id % 8);

            let current_node = entry.travel[1];
            if let Some(predecessor) = entry.predecessor {
                predecessors.push((entry.travel[1], predecessor));
            }
            if current_node.is(end) {
                return rebuild_path_vec(&current_node, &predecessors);
            }
            heap.extend(
                self.neighbours(&current_node)
                    .map(|travel| HeapEntry {
                        predecessor: Some(current_node),
                        travel,
                        distance: entry.distance + travel[0].distance_to(&travel[1]),
                    })
                    .filter(|entry| {
                        let d = entry.distance + entry.travel[1].distance_to(end);
                        d <= greedy_path_length + TILE_BORDER_THICKNESS // TODO: should be max path length error
                    }),
            );
        }
        Vec::new()
    }

    // return if we can discard this crossroad safely.
    // it is the case if by going forward you take the right path
    pub fn obvious_crossroad(
        &self,
        possible_waypoint_node: &Node,
        previous_node: &Node,
        next_node: &Node,
    ) -> bool {
        let ideal_leaving_angle = previous_node.angle_to(possible_waypoint_node);
        let real_leaving_angle = possible_waypoint_node.angle_to(next_node);
        let allowed_angle_diff = angles_sub(ideal_leaving_angle, real_leaving_angle);

        let mut possible_destinations = HashSet::new();
        for n in self
            .node_tiles(possible_waypoint_node)
            .flat_map(|(tile_x, tile_y)| self.tile_edges(tile_x, tile_y))
            .flatten()
            .filter(|n| n.distance_to(possible_waypoint_node) <= 0.0001)
        {
            possible_destinations.extend(self.neighbours(&n).map(|n| n[1].node));
        }

        possible_destinations.len() <= 2 || {
            // check alignment
            // if one and only one destination is roughly aligned
            // then it's good : it's not a waypoint
            let count = possible_destinations
                .iter()
                .filter(|d| {
                    let ia = possible_waypoint_node.angle_to(d);
                    let angle_diff = angles_sub(ideal_leaving_angle, ia);
                    // angle_diff <= allowed_angle_diff * 1.5 + PI / 36.
                    angle_diff <= allowed_angle_diff + PI / 10.
                })
                .count();
            count == 1
        }
    }

    pub fn detect_crossroads(&self, path: &mut Vec<Node>, waypoints: &mut HashSet<Node>) {
        eprintln!("detecting crossroads");
        // let rp = crate::gps::simplify_path_around_waypoints(&path, &waypoints);

        let crossroads = self
            .ways()
            .flatten()
            .unique()
            .filter(|n| {
                self.node_tiles(n)
                    .flat_map(|(tx, ty)| self.tile_edges(tx, ty))
                    .filter_map(|[n1, n2]| {
                        if n1.node == *n {
                            Some(n2.node)
                        } else if n2.node == *n {
                            Some(n1.node)
                        } else {
                            None
                        }
                    })
                    .unique()
                    .count()
                    > 2
            })
            .collect::<HashSet<_>>();

        let mut current_distance = 0.;
        let mut previous_waypoint_distance = None;
        for (previous_node, node, next_node) in path.iter().tuple_windows() {
            current_distance += previous_node.distance_to(node);
            if self
                .node_tiles(node)
                .flat_map(|(tile_x, tile_y)| self.tile_edges(tile_x, tile_y))
                .flatten()
                .filter(|n| n.distance_to(node) <= 0.0001)
                .any(|n| crossroads.contains(&n))
            {
                if !(self.obvious_crossroad(node, previous_node, next_node)
                    && self.obvious_crossroad(node, next_node, previous_node))
                {
                    if previous_waypoint_distance
                        .map(|pd| current_distance - pd > 0.0003)
                        .unwrap_or(true)
                    {
                        waypoints.insert(*node);
                        previous_waypoint_distance = Some(current_distance);
                    }
                }
            }
        }
        let final_path = crate::gps::simplify_path_around_waypoints(path, waypoints);
        *path = final_path;
        return;

        // let tiled_segments = self.hash_segments_on_tiles(&rp);
        // for possible_waypoint in crossroads {
        //     let impacted_segments = self
        //         .node_tiles(&possible_waypoint)
        //         .filter_map(|tile| tiled_segments.get(&tile))
        //         .flatten()
        //         .unique();
        //     for &segment_id in impacted_segments {
        //         let start = &rp[segment_id];
        //         let end = &rp[segment_id + 1];
        //         if possible_waypoint.distance_to(start) <= 0.0001 {
        //             if segment_id > 0 {
        //                 let previous_node = &rp[segment_id - 1];
        //                 let next_node = end;
        //                 if !(self.obvious_crossroad(start, previous_node, next_node)
        //                     && self.obvious_crossroad(start, next_node, previous_node))
        //                 {
        //                     waypoints.insert(*start);
        //                 }
        //             }
        //         } else if possible_waypoint.distance_to(end) <= 0.0001 {
        //             if segment_id < rp.len() - 1 {
        //                 let previous_node = start;
        //                 let next_node = end;
        //                 if !(self.obvious_crossroad(end, previous_node, next_node)
        //                     && self.obvious_crossroad(end, next_node, previous_node))
        //                 {
        //                     waypoints.insert(*end);
        //                 }
        //             }
        //         } else if possible_waypoint.distance_to_segment(start, end) <= 0.0001 {
        //             if !(self.obvious_crossroad(&possible_waypoint, start, end)
        //                 && self.obvious_crossroad(&possible_waypoint, end, start))
        //             {
        //                 waypoints.insert(possible_waypoint);
        //             }
        //         }
        //     }
        // }
        // let final_path = crate::gps::simplify_path_around_waypoints(path, waypoints);
        // for (previous_node, node, next_node) in rp.iter().tuple_windows() {
        //     if self
        //         .node_tiles(node)
        //         .flat_map(|(tile_x, tile_y)| self.tile_edges(tile_x, tile_y))
        //         .flatten()
        //         .filter(|n| n.distance_to(node) <= 0.0001)
        //         .any(|n| crossroads.contains(&n))
        //     {
        //         // waypoints.insert(*node);
        //         // continue;
        //         if self.obvious_crossroad(node, previous_node, next_node)
        //             && self.obvious_crossroad(node, next_node, previous_node)
        //         {
        //             eprintln!("discarding");
        //         } else {
        //             eprintln!("keeping");
        //             waypoints.insert(*node);
        //         }
        //     }
        // }
        // eprintln!("nodes: {} waypoints: {}", final_path.len(), waypoints.len());
        // *path = final_path;

        // return;

        // for node in path {
        //     if self.nearby_high_degree_node(node, 0.0001) {
        //         waypoints.insert(node.clone());
        //     }
        // }
    }

    fn hash_segments_on_tiles(&self, path: &[Node]) -> HashMap<(usize, usize), HashSet<usize>> {
        // figure out for each tile which path segments are on it
        let mut segments_tiles: HashMap<(usize, usize), HashSet<usize>> = HashMap::new();
        for (segment_id, (n1, n2)) in path.iter().tuple_windows().enumerate() {
            let mut new_nodes = std::iter::once(*n1)
                .chain(
                    grid_coordinates_between(n1.x, n2.x, self.side)
                        .map(|x| n1.vertical_segment_intersection(&n2, x))
                        .chain(
                            grid_coordinates_between(n1.y, n2.y, self.side)
                                .map(|y| n1.horizontal_segment_intersection(&n2, y)),
                        ),
                )
                .chain(std::iter::once(*n2))
                .collect::<Vec<_>>();
            new_nodes.sort_unstable_by(|na, nb| {
                let da = na.squared_distance_to(&n1);
                let db = nb.squared_distance_to(&n1);
                da.partial_cmp(&db).unwrap()
            });
            new_nodes
                .iter()
                .flat_map(|n| self.node_tiles(n))
                .for_each(|tile| {
                    segments_tiles.entry(tile).or_default().insert(segment_id);
                });
        }
        segments_tiles
    }

    fn nearby_high_degree_node(&self, node: &Node, treshold: f64) -> bool {
        self.node_tiles(node)
            .flat_map(|(tile_x, tile_y)| self.tile_edges(tile_x, tile_y))
            .flatten()
            .filter(|n| n.distance_to(node) <= treshold)
            .any(|n| self.neighbours(&n).nth(2).is_some())
    }

    fn greedy_path(&self, start: &GNode, end: &GNode) -> f64 {
        let mut heap = BinaryHeap::new();
        heap.push(HeapEntry {
            predecessor: None,
            travel: [*start, *start],
            distance: start.squared_distance_to(end),
        });
        let mut seen_nodes = vec![0u8; 1 + self.binary_ways.len() / 16];
        let mut predecessors = Vec::new();
        let mut loop_count = 0;
        while let Some(entry) = heap.pop() {
            loop_count += 1;
            if loop_count == 300 {
                break;
            };

            let n1_offset_id = self.node_offset_id(&entry.travel[1].id);
            if (seen_nodes[n1_offset_id / 8] & (1u8 << (n1_offset_id % 8))) != 0 {
                eprintln!("skipping {:?} {}", entry.travel[1].id, n1_offset_id);
                continue;
            }
            let n0_offset_id = self.node_offset_id(&entry.travel[0].id);
            eprintln!(
                "loop {loop_count}, we are at {} {:?} {}, from {:?} {}",
                entry.distance, entry.travel[1].id, n1_offset_id, entry.travel[0].id, n0_offset_id
            );
            seen_nodes[n0_offset_id / 8] |= 1u8 << (n0_offset_id % 8);
            seen_nodes[n1_offset_id / 8] |= 1u8 << (n1_offset_id % 8);

            let current_node = entry.travel[1];
            if let Some(predecessor) = entry.predecessor {
                predecessors.push((entry.travel[1], predecessor));
            }
            if current_node.is(end) {
                eprintln!("we found it in {}", loop_count);
                return path_length_vec(&current_node, &predecessors);
            }

            heap.extend(self.neighbours(&current_node).map(|travel| HeapEntry {
                predecessor: Some(current_node),
                travel,
                distance: travel[1].squared_distance_to(end),
            }));
        }
        let seen_nodes = predecessors
            .into_iter()
            .flat_map(|(a, b)| [a.node, b.node])
            .collect::<Vec<_>>();
        let min_distance = seen_nodes
            .iter()
            .map(|n| n.distance_to(&end.node))
            .min_by(|d1, d2| d1.partial_cmp(d2).unwrap());
        eprintln!(
            "min distance is {:?} and we want < {}",
            min_distance, TILE_BORDER_THICKNESS
        );

        save_svg(
            "fail.svg",
            self.bounding_box(),
            [
                self as SvgW,
                &crate::svg::UniColorNodes(seen_nodes) as SvgW,
                start as SvgW,
                end as SvgW,
            ],
        )
        .unwrap();

        0.
    }

    #[allow(dead_code)]
    fn connected_component(&self, start: &GNode) -> Vec<Vec<Node>> {
        let mut stack = vec![[*start, *start]];
        let mut component = Vec::new();
        let mut seen_nodes = HashSet::new(); // NOTE: this will be a BitVec and not a hashset
        while let Some(travel) = stack.pop() {
            if seen_nodes.contains(&travel[1].id) {
                continue;
            }
            seen_nodes.insert(travel[0].id);
            seen_nodes.insert(travel[1].id);
            let current_node = travel[1];
            let edge = travel.iter().map(|n| n.node).collect::<Vec<_>>();
            if edge[0] != edge[1] {
                component.push(edge);
            }
            stack.extend(self.neighbours(&current_node));
        }
        component
    }

    // go to nearest node in the street
    fn find_ending_node(&self, gps_start: &Node, street: &str) -> GNode {
        self.streets
            .get(street)
            .into_iter()
            .flatten()
            .flat_map(|&way_id| self.way(way_id))
            .min_by(|na, nb| {
                na.squared_distance_to(gps_start)
                    .partial_cmp(&nb.squared_distance_to(gps_start))
                    .unwrap()
            })
            .unwrap()
    }

    fn tile_edges(&self, tile_x: usize, tile_y: usize) -> impl Iterator<Item = [GNode; 2]> + '_ {
        let tile_number = (tile_x + tile_y * self.grid_size.0 as usize) as u16;
        (0..(self.tile_ways_number(tile_number))).map(move |local_way_id| {
            self.way(CWayId {
                tile_number,
                local_way_id,
            })
        })
    }

    fn find_starting_node(&self, gps_start: &Node) -> GNode {
        //TODO: fixme if between tiles
        //TODO: fixme if outside of grid
        //TODO: fixme if empty tile
        let (tile_x, tile_y) = self.node_tiles(gps_start).next().unwrap();
        //TODO: tile_ways
        self.tile_edges(tile_x, tile_y)
            .flatten()
            .min_by(|na, nb| {
                na.squared_distance_to(gps_start)
                    .partial_cmp(&nb.squared_distance_to(gps_start))
                    .unwrap()
            })
            .unwrap()
    }

    // this is tough.
    // if we have two ways connecting, let's say w1 = (s1, e1) and w2 = (s2, e2) :
    // such that e1 is s2.
    // if we are located at e1 : we can go at s1
    // but we can also go at e2 : however doing so we need to mark both s2 and e2 as visited
    // and not only e2.
    // that's why i cannot loop on the neighbours only, i also need the intermediate points.
    fn neighbours<'a>(&'a self, node: &'a GNode) -> impl Iterator<Item = [GNode; 2]> + 'a {
        self.node_tiles(node) // TODO: rewrite me now that we have node ids in the tile
            .flat_map(|(tile_x, tile_y)| self.tile_edges(tile_x, tile_y))
            .filter_map(|nodes| {
                let is_0 = nodes[0].is(node);
                let is_1 = nodes[1].is(node);
                if is_0 && is_1 {
                    None // if both endpoints are neighbours then we'll leave through one of them
                         // on another way
                } else if is_0 {
                    Some([nodes[0], nodes[1]])
                } else if is_1 {
                    Some([nodes[1], nodes[0]])
                } else {
                    None
                }
            })
    }

    fn way(&self, way_id: CWayId) -> [GNode; 2] {
        let id1 = CNodeId {
            tile_number: way_id.tile_number,
            local_node_id: 2 * way_id.local_way_id as u16,
        };

        let id2 = CNodeId {
            tile_number: way_id.tile_number,
            local_node_id: 2 * way_id.local_way_id as u16 + 1,
        };
        [
            GNode {
                node: self.decode_node(id1),
                id: id1,
            },
            GNode {
                node: self.decode_node(id2),
                id: id2,
            },
        ]
    }
}

fn path_length_vec(end: &GNode, predecessors: &[(GNode, GNode)]) -> f64 {
    std::iter::once(end)
        .chain(
            predecessors
                .iter()
                .rev()
                .scan(end, |current_node, (na, prec_na)| {
                    if na.is(current_node) {
                        *current_node = prec_na;
                        Some(Some(prec_na))
                    } else {
                        Some(None)
                    }
                })
                .flatten(),
        )
        .tuple_windows()
        .map(|(n1, n2)| n1.distance_to(n2))
        .sum::<f64>()
}

fn rebuild_path(end: &GNode, predecessors: &HashMap<GNode, GNode>) -> Vec<Node> {
    std::iter::successors(Some(*end), |current_node| {
        predecessors.get(current_node).copied()
    })
    .map(|n| n.node)
    .collect()
}

fn rebuild_path_vec(end: &GNode, predecessors: &[(GNode, GNode)]) -> Vec<Node> {
    predecessors
        .iter()
        .rev()
        .fold(vec![end.node], |mut path, (na, prec_na)| {
            let current_node = path.last().unwrap();
            if na.is(current_node) {
                path.push(prec_na.node)
            }
            path
        })
}

#[derive(Debug)]
struct HeapEntry {
    predecessor: Option<GNode>,
    travel: [GNode; 2],
    distance: f64,
}
impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}
impl Eq for HeapEntry {}
impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.distance
            .partial_cmp(&other.distance)
            .map(std::cmp::Ordering::reverse)
    }
}
impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

fn angles_sub(angle1: f64, angle2: f64) -> f64 {
    // pre-conditions : angles are both between 0 and 2pi
    let diff1 = angle2 - angle1;
    let diff2 = angle1 - angle2;
    let normalized_diff1 = (diff1 + (2. * PI)) % (2. * PI);
    let normalized_diff2 = (diff2 + (2. * PI)) % (2. * PI);
    normalized_diff1.min(normalized_diff2)
}
