use gps::{cut_segments_on_tiles, cut_ways_on_tiles, simplify_ways, Node};
use gps::{grid_coordinates_between, sanitize_ways};
use itertools::Itertools;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use tokio::io::AsyncWriteExt;

use gps::group_nodes_in_squares;
use gps::parse_osm_xml;
use gps::rename_nodes;
use gps::request;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::io::BufReader;
use tokio::io::BufWriter;

const COLORS: [&str; 5] = ["red", "green", "blue", "purple", "cyan"];

fn save_svg<P: AsRef<Path>>(
    path: P,
    nodes: &[Node],
    ways: &[Vec<usize>],
    bbox: (f64, f64, f64, f64),
    side: f64,
) -> std::io::Result<()> {
    let mut writer = std::io::BufWriter::new(std::fs::File::create(path)?);
    let (xmin, ymin, xmax, ymax) = bbox;

    writeln!(
        &mut writer,
        "<svg width='800' height='600' viewBox='{} {} {} {}'>",
        xmin,
        ymin,
        xmax - xmin,
        ymax - ymin
    )?;
    writeln!(
        &mut writer,
        "<rect fill='white' x='{}' y='{}' width='{}' height='{}'/>",
        xmin,
        ymin,
        xmax - xmin,
        ymax - ymin
    )?;

    writeln!(
        &mut writer,
        "<g transform='translate(0, {}) scale(1,-1)'>",
        ymin + ymax
    )?;

    for x in grid_coordinates_between(xmin, xmax, SIDE) {
        writeln!(
            &mut writer,
            "<line x1='{x}' y1= '{ymin}' x2='{x}' y2='{ymax}' stroke='grey' stroke-width='0.2%'/>"
        )?;
    }

    for y in grid_coordinates_between(ymin, ymax, SIDE) {
        writeln!(
            &mut writer,
            "<line x1='{xmin}' y1= '{y}' x2='{xmax}' y2='{y}' stroke='grey' stroke-width='0.2%'/>"
        )?;
    }

    for n in nodes {
        let color = ((n.y / side).floor() as usize + (n.x / side).floor() as usize) % COLORS.len();
        writeln!(
            &mut writer,
            "<circle cx='{}' cy='{}' fill='{}' r='0.8%'/>",
            n.x, n.y, COLORS[color]
        )?;
    }
    for way_points in ways {
        way_points.iter().tuple_windows().try_for_each(|(i1, i2)| {
            let n1 = nodes[*i1];
            let n2 = nodes[*i2];
            writeln!(
                &mut writer,
                "<line x1='{}' y1='{}' x2='{}' y2='{}' stroke='black' stroke-width='0.2%'/>",
                n1.x, n1.y, n2.x, n2.y
            )
        })?;
    }

    writeln!(&mut writer, "</g></svg>",)?;

    Ok(())
}

const SIDE: f64 = 1. / 1000.; // excellent value
                              // with it we have few segments crossing several squares
                              // and what's more we can use 1 byte for each coordinate inside the square
                              // for 1/2 meter precision

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // let bbox = (5.767136, 45.186547, 5.77, 45.19);
    // let answer = request(bbox.0, bbox.1, bbox.2, bbox.3).await.unwrap();
    // let mut log = BufWriter::new(File::create("small_log").await?);
    // log.write_all(answer.as_bytes()).await?;
    // let (mut nodes, mut ways, streets) = parse_osm_xml(&answer);

    let bbox = (5.767136, 45.186547, 5.897531, 45.247925);
    // let answer = request(5.767136, 45.186547, 5.897531, 45.247925)
    //     .await
    //     .unwrap();
    let mut answer = Vec::new();
    BufReader::new(File::open("log").await?)
        // BufReader::new(File::open("small_log").await?)
        .read_to_end(&mut answer)
        .await?;
    let (nodes, mut ways, mut streets) = parse_osm_xml(std::str::from_utf8(&answer).unwrap());
    let mut renamed_nodes = rename_nodes(nodes, &mut ways);
    let mut ways = sanitize_ways(ways, &mut streets);
    // save_svg("not_simpl_test.svg", &renamed_nodes, &ways, bbox, SIDE)?;
    simplify_ways(&mut renamed_nodes, &mut ways, &mut streets);
    // save_svg("simpl_test.svg", &renamed_nodes, &ways, bbox, SIDE)?;
    eprintln!(
        "we have {} nodes and {} streets",
        renamed_nodes.len(),
        streets.len()
    );
    cut_segments_on_tiles(&mut renamed_nodes, &mut ways, SIDE);
    // save_svg("cut_test.svg", &renamed_nodes, &ways, bbox, SIDE)?;
    eprintln!(
        "after cutting segments we have {} nodes",
        renamed_nodes.len()
    );

    eprintln!(
        "we have {} segments and {} ways",
        ways.iter().map(|w| w.len() - 1).sum::<usize>(),
        ways.len()
    );
    let (ways, tiles) = cut_ways_on_tiles(&renamed_nodes, ways, &mut streets, SIDE);
    eprintln!(
        "after cutting ways we have {} segments and {} ways and {} tiles",
        ways.iter().map(|w| w.len() - 1).sum::<usize>(),
        ways.len(),
        tiles.len()
    );
    let street_segments = streets
        .values()
        .flat_map(|street_ways| {
            street_ways
                .iter()
                .map(|w| ways.get(*w).map(|w| w.len()).unwrap_or_default())
        })
        .sum::<usize>();
    eprintln!("we have {street_segments} street segments");
    save_svg("test.svg", &renamed_nodes, &ways, bbox, SIDE)?;
    Ok(())
}
