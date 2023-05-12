use std::{
    collections::HashSet,
    io::{Read, Write},
};

use gpx::{read, Gpx};
use itertools::Itertools;

use crate::{map_and_interests_from_string, request, Map, Node};

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

pub fn detect_sharp_turns(path: &[Node], waypoints: &mut HashSet<Node>) {
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

pub async fn request_map_from<P: AsRef<std::path::Path>>(
    polygon: &[Node],
    key_values: &[(String, String)],
    map_name: Option<P>,
) -> Result<(Map, Vec<(usize, Node)>), Box<dyn std::error::Error>> {
    eprintln!("requesting map");
    let osm_answer = request(polygon).await?;
    crate::log("got the request answer");
    eprintln!("we got the map, saving it");
    if let Some(map_name) = map_name {
        let mut writer = std::io::BufWriter::new(std::fs::File::create(map_name)?);
        writer.write_all(osm_answer.as_bytes())?;
        eprintln!("we saved the map");
    }
    Ok(map_and_interests_from_string(&osm_answer, key_values))
}

pub fn save_path<W: Write>(
    points: &[Node],
    waypoints: &HashSet<Node>,
    writer: &mut W,
) -> std::io::Result<()> {
    writer.write_all(&[crate::map::BlockType::Path as u8])?;
    writer.write_all(&(points.len() as u16).to_le_bytes())?;
    for coordinates in points.iter().flat_map(|p| [p.x, p.y]) {
        writer.write_all(&coordinates.to_le_bytes())?;
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
        writer.write_all(&byte.to_le_bytes())?;
    }
    Ok(())
}

// inflate a polygon around given path
pub fn build_polygon(path: &[Node], thickness: f64) -> Vec<Node> {
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
