use std::{
    collections::HashSet,
    io::{Read, Write},
};

use gpx::{read, Gpx};
use itertools::Itertools;
use tokio::io::AsyncWriteExt;

use crate::{request, save_svg, simplify::simplify_path, Map, Node, Svg};

const LOWER_SHARP_TURN: f64 = 80.0 * std::f64::consts::PI / 180.0;
const UPPER_SHARP_TURN: f64 = std::f64::consts::PI * 2.0 - LOWER_SHARP_TURN;

fn points<R: Read>(reader: R) -> (HashSet<Node>, Vec<Node>) {
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

pub async fn convert_gpx<R: Read, W: AsyncWriteExt + std::marker::Unpin>(
    input_reader: R,
    map: Option<Map>,
    writer: &mut W,
) -> Result<(), Box<dyn std::error::Error>> {
    // load all points composing the trace and mark commented points
    // as special waypoints.
    let (mut waypoints, p) = points(input_reader);

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

    let map = if let Some(map) = map {
        map
    } else {
        let path_polygon = build_polygon(&rp, 0.001); // 100m each side
        let osm_answer = request(&path_polygon).await?;
        let mut writer = std::io::BufWriter::new(std::fs::File::create("testpathosm.txt")?);
        writer.write_all(osm_answer.as_bytes())?;
        Map::load(osm_answer)?
    };

    let path: Map = rp.into();
    path.save_tiles(writer, &[255, 0, 0]).await?;
    map.save_tiles(writer, &[0, 0, 0]).await?;

    Ok(())
}

// inflate a polygon around given path
fn build_polygon(path: &[Node], thickness: f64) -> Vec<Node> {
    path.windows(2)
        .flat_map(|segment| parallel_segment(segment.iter(), thickness))
        .chain(
            path.windows(2)
                .rev()
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
    [
        Node::new(start.x + x, start.y + y),
        Node::new(end.x + x, end.y + y),
    ]
}
