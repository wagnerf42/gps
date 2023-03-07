use gps::{
    cut_segments_on_tiles, cut_ways_into_edges, group_ways_in_tiles, simplify_ways, Map, Node,
    NodeId, SvgW, WayId,
};
use gps::{sanitize_ways, save_svg};
use std::collections::HashMap;
// use std::io::Write;
use tokio::io::AsyncWriteExt;

use gps::parse_osm_xml;
use gps::rename_nodes;
use gps::request;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::io::BufReader;
use tokio::io::BufWriter;

const SIDE: f64 = 1. / 1000.; // excellent value
                              // with it we have few segments crossing several squares
                              // and what's more we can use 1 byte for each coordinate inside the square
                              // for 1/2 meter precision

#[allow(dead_code)]
async fn load_data_set(
    filename: &str,
) -> std::io::Result<(
    Vec<Node>,
    HashMap<WayId, Vec<NodeId>>,
    HashMap<String, Vec<WayId>>,
    f64,
)> {
    // let answer = request(5.767136, 45.186547, 5.897531, 45.247925) // large
    let mut answer = Vec::new();
    BufReader::new(File::open(filename).await?)
        .read_to_end(&mut answer)
        .await?;
    let (nodes, mut ways, streets) = parse_osm_xml(std::str::from_utf8(&answer).unwrap());
    let renamed_nodes = rename_nodes(nodes, &mut ways);
    Ok((renamed_nodes, ways, streets, SIDE))
}

#[allow(dead_code)]
async fn request_data_set(
    path: &[Node],
    filename: &str,
) -> std::io::Result<(
    Vec<Node>,
    HashMap<WayId, Vec<NodeId>>,
    HashMap<String, Vec<WayId>>,
    f64,
)> {
    let answer = request(path).await.unwrap();
    BufWriter::new(File::create(filename).await?)
        .write_all(answer.as_bytes())
        .await?;
    let (nodes, mut ways, streets) = parse_osm_xml(std::str::from_utf8(answer.as_bytes()).unwrap());
    let renamed_nodes = rename_nodes(nodes, &mut ways);
    Ok((renamed_nodes, ways, streets, SIDE))
}

#[allow(dead_code)]
fn manual_data_set() -> (
    Vec<Node>,
    HashMap<WayId, Vec<NodeId>>,
    HashMap<String, Vec<WayId>>,
    f64,
) {
    let renamed_nodes = vec![
        Node::new(3., 3.),
        Node::new(4., 3.),
        Node::new(4., 2.),
        Node::new(8., 4.),
        Node::new(5., 2.),
    ];
    let ways = [vec![0, 1], vec![1, 2], vec![1, 3], vec![3, 4]]
        .into_iter()
        .enumerate()
        .collect();
    let streets = std::iter::once(("Rue Lavoisier".to_owned(), vec![3])).collect();
    (renamed_nodes, ways, streets, 10.)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gpx_filename = std::env::args()
        .nth(1)
        .unwrap_or("retours_route.gpx".to_owned());
    let gpx_file = std::fs::File::open(gpx_filename)?;
    let gpx_reader = std::io::BufReader::new(gpx_file);
    let map = std::env::args()
        .nth(2)
        .and_then(|map_name| Map::load(map_name, SIDE).ok());
    gps::convert_gpx(gpx_reader, map).await?;
    Ok(())
}

// #[tokio::main]
// async fn main() -> std::io::Result<()> {
//     // let (mut nodes, ways, mut streets, side) =
//     //     request_data_set(5.767136, 45.186547, 5.897531, 45.247925, "large2.set").await?;

//     // let (mut nodes, ways, mut streets, side) = request_data_set(
//     //     5.7860000000000005,
//     //     45.211,
//     //     5.787000000000001,
//     //     45.211999999999996,
//     //     "heavy2.set",
//     // )
//     // .await?;

//     let (mut nodes, ways, mut streets, side) = load_data_set("large.set").await?;
//     // let (mut nodes, ways, mut streets, side) = load_data_set("small.set").await?;
//     // let (mut nodes, ways, mut streets, side) = load_data_set("heavy.set").await?;
//     // let (mut nodes, ways, mut streets, side) = manual_data_set();
//     let mut ways = sanitize_ways(ways, &mut streets);
//     simplify_ways(&mut nodes, &mut ways, &mut streets);
//     eprintln!(
//         "we have {} nodes and {} streets",
//         nodes.len(),
//         streets.len()
//     );
//     cut_segments_on_tiles(&mut nodes, &mut ways, side);
//     eprintln!("after cutting segments we have {} nodes", nodes.len());
//     eprintln!(
//         "we have {} segments and {} ways",
//         ways.iter().map(|w| w.len() - 1).sum::<usize>(),
//         ways.len()
//     );
//     let ways = cut_ways_into_edges(ways, &mut streets);
//     eprintln!(
//         "after cutting ways we have {} segments and {} ways",
//         ways.iter().map(|w| w.len() - 1).sum::<usize>(),
//         ways.len(),
//     );
//     let tiles = group_ways_in_tiles(&nodes, &ways, side);
//     let street_segments = streets
//         .values()
//         .flat_map(|street_ways| {
//             street_ways
//                 .iter()
//                 .map(|w| ways.get(*w).map(|w| w.len() - 1).unwrap_or_default())
//         })
//         .sum::<usize>();
//     eprintln!("we have {street_segments} street segments");

//     let map = Map::new(&nodes, &ways, streets, &tiles, side);
//     let (map_size, tiles_number, max_ways_per_tile) = map.stats();
//     map.save("test.map").await?;
//     eprintln!("map has size {map_size}, with {tiles_number} tiles and at most {max_ways_per_tile} ways per tile");
//     save_svg("dec.svg", map.bounding_box(), std::iter::once(&map as SvgW))?;
//     // let path = map.shortest_path(&Node::new(5.79, 45.22), "Rue Lavoisier");
//     let path = map.shortest_path(&Node::new(5.8275, 45.223), "Faculté de Pharmacie");
//     // let path = map.shortest_path(&Node::new(5.8275, 45.223), "Belle Plaine");
//     // let path = map.shortest_path(&Node::new(5.769, 45.187), "Rue des Universités");
//     // let path = map.shortest_path(&Node::new(2.99, 2.99), "Rue Lavoisier");
//     eprintln!("path is {path:?}");

//     Ok(())
// }
