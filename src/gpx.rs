use std::{
    collections::HashSet,
    io::{Read, Write},
};

use gpx::{read, Gpx};
use itertools::Itertools;
use tokio::io::AsyncWriteExt;

use crate::{
    map::SIDE,
    request, save_svg,
    simplify::simplify_path,
    svg::{MapTiles, UniColorNodes},
    Map, Node, Svg, SvgW,
};

const LOWER_SHARP_TURN: f64 = 80.0 * std::f64::consts::PI / 180.0;
const UPPER_SHARP_TURN: f64 = std::f64::consts::PI * 2.0 - LOWER_SHARP_TURN;

pub fn parse_gpx_points<R: Read>(reader: R) -> (HashSet<Node>, Vec<Node>) {
    // read takes any io::Read and gives a Result<Gpx, Error>.
    let mut gpx: Gpx = read(reader).unwrap();
    eprintln!("we have {} tracks", gpx.tracks.len());

    let mut waypoints = HashSet::new();

    let points = gpx
        .tracks
        .pop()
        .unwrap()
        .segments
        .into_iter()
        .flat_map(|segment| segment.points.into_iter())
        .map(|p| {
            let is_commented = p.comment.is_some();
            let (x, y) = p.point().x_y();
            let p = Node { x, y };
            if is_commented {
                waypoints.insert(p);
            }
            p
        })
        .collect::<Vec<_>>();
    (waypoints, points)
}

fn detect_sharp_turns(path: &[Node], waypoints: &mut HashSet<Node>) {
    path.iter()
        .tuple_windows()
        .map(|(a, b, c)| {
            let xd1 = b.x - a.x;
            let yd1 = b.y - a.y;
            let angle1 = yd1.atan2(xd1);

            let xd2 = c.x - b.x;
            let yd2 = c.y - b.y;
            let angle2 = yd2.atan2(xd2);
            let adiff = angle2 - angle1;
            let adiff = if adiff < 0.0 {
                adiff + std::f64::consts::PI * 2.0
            } else {
                adiff
            };
            (adiff, b)
        })
        .filter_map(|(adiff, b)| {
            if adiff > LOWER_SHARP_TURN && adiff < UPPER_SHARP_TURN {
                Some(b)
            } else {
                None
            }
        })
        .for_each(|b| {
            waypoints.insert(*b);
        });
}

pub fn load_gpx(path: &str) -> std::io::Result<(HashSet<Node>, Vec<Node>)> {
    let gpx_file = std::fs::File::open(path)?;
    let gpx_reader = std::io::BufReader::new(gpx_file);

    // load all points composing the trace and mark commented points
    // as special waypoints.
    let (mut waypoints, p) = parse_gpx_points(gpx_reader);

    // detect sharp turns before path simplification to keep them
    detect_sharp_turns(&p, &mut waypoints);
    waypoints.insert(p.first().copied().unwrap());
    waypoints.insert(p.last().copied().unwrap());
    println!("we have {} waypoints", waypoints.len());

    println!("initially we had {} points", p.len());

    // simplify path
    let rp = if p.len() < 100 {
        p.clone()
    } else {
        std::iter::successors(Some(0.00015), |precision| Some(precision / 2.))
            .map(|precision| {
                // simplify path
                let mut rp = Vec::new();
                let mut segment = Vec::new();
                for point in &p {
                    segment.push(*point);
                    if waypoints.contains(point) && segment.len() >= 2 {
                        let mut s = simplify_path(&segment, precision);
                        rp.append(&mut s);
                        segment = rp.pop().into_iter().collect();
                    }
                }
                rp.append(&mut segment);
                rp
            })
            .find(|rp| rp.len() > 80)
            .unwrap()
    };
    println!("we now have {} points", rp.len());
    Ok((waypoints, rp))
}

pub async fn request_map_from_path<P: AsRef<std::path::Path>>(
    path: &[Node],
    key_values: &[(String, String)],
    map_name: P,
) -> Result<Map, Box<dyn std::error::Error>> {
    eprintln!("requesting map");
    let path_polygon = build_polygon(path, SIDE * 2.); // two tiles on each side
    let osm_answer = request(&path_polygon).await?;
    eprintln!("we got the map, saving it");
    let mut writer = std::io::BufWriter::new(std::fs::File::create(map_name)?);
    writer.write_all(osm_answer.as_bytes())?;
    eprintln!("we saved the map");
    Ok(Map::from_string(&osm_answer, key_values))
}

pub async fn convert_gpx<W: AsyncWriteExt + std::marker::Unpin>(
    waypoints: &HashSet<Node>,
    gpx_path: &Vec<Node>,
    mut map: Map,
    writer: &mut W,
) -> Result<(), Box<dyn std::error::Error>> {
    let path_map: Map = gpx_path.clone().into();
    let extended_path_tiles = path_map
        .non_empty_tiles()
        .map(|(x, y)| {
            (
                x + path_map.first_tile.0 - map.first_tile.0,
                y + path_map.first_tile.1 - map.first_tile.1,
            )
        })
        .flat_map(|(x, y)| {
            (x.saturating_sub(1)..(x + 2))
                .flat_map(move |nx| (y.saturating_sub(1)..(y + 2)).map(move |ny| (nx, ny)))
        })
        .collect::<HashSet<(usize, usize)>>();

    map.keep_tiles(&extended_path_tiles);

    let interests_nodes = UniColorNodes(
        map.interests
            .iter()
            .map(|(_, n)| n)
            .cloned()
            .collect::<Vec<_>>(),
    );

    let maptiles = MapTiles {
        tiles: &extended_path_tiles,
        map: &map,
    };
    save_svg(
        "map.svg",
        map.bounding_box(),
        [
            &map as SvgW,
            (&gpx_path.as_slice()) as SvgW,
            &interests_nodes as SvgW,
            &maptiles as SvgW,
        ],
    )
    .unwrap();

    map.add_interests(std::iter::repeat(0).zip(waypoints.iter().copied()));
    eprintln!("saving interests");
    map.save_interests(writer).await?;
    eprintln!("saving the path");
    save_path(&gpx_path, &waypoints, writer).await?;
    eprintln!("saving the pathtiles");
    let path: Map = gpx_path.clone().into();
    path.save_tiles(writer, &[255, 0, 0]).await?;
    eprintln!("saving the maptiles");
    map.save_tiles(writer, &[0, 0, 0]).await?;
    eprintln!("all is saved");

    Ok(())
}

pub async fn save_path<W: AsyncWriteExt + std::marker::Unpin>(
    points: &[Node],
    waypoints: &HashSet<Node>,
    writer: &mut W,
) -> std::io::Result<()> {
    writer.write_u8(crate::map::BlockType::Path as u8).await?;
    writer
        .write_all(&(points.len() as u16).to_le_bytes())
        .await?;
    for coordinates in points.iter().flat_map(|p| [p.x, p.y]) {
        writer.write_all(&coordinates.to_le_bytes()).await?;
    }

    let mut waypoints_bits = std::iter::repeat(0u8)
        .take(points.len() / 8 + if points.len() % 8 != 0 { 1 } else { 0 })
        .collect::<Vec<u8>>();
    points.iter().enumerate().for_each(|(i, p)| {
        if waypoints.contains(p) {
            waypoints_bits[i / 8] |= 1 << (i % 8)
        }
    });
    for byte in &waypoints_bits {
        writer.write_all(&byte.to_le_bytes()).await?;
    }
    Ok(())
}

// inflate a polygon around given path
fn build_polygon(path: &[Node], thickness: f64) -> Vec<Node> {
    path.windows(2)
        .filter(|w| w[0] != w[1])
        .flat_map(|segment| parallel_segment(segment.iter(), thickness))
        .chain(
            path.windows(2)
                .rev()
                .filter(|w| w[0] != w[1])
                .flat_map(|segment| parallel_segment(segment.iter().rev(), thickness)),
        )
        .dedup()
        .collect()
}

// return a parallel segment at distance "thickness"
fn parallel_segment<'a, I: Iterator<Item = &'a Node>>(mut segment: I, thickness: f64) -> [Node; 2] {
    let start = segment.next().unwrap();
    let end = segment.next().unwrap();
    let xdiff = end.x - start.x;
    let ydiff = end.y - start.y;
    let d = (xdiff * xdiff + ydiff * ydiff).sqrt();
    let x = (-ydiff / d) * thickness;
    let y = (xdiff / d) * thickness;
    assert!(!x.is_nan());
    assert!(!y.is_nan());
    [
        Node::new(start.x + x, start.y + y),
        Node::new(end.x + x, end.y + y),
    ]
}
