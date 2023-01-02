use gps::cut_ways_at_squares;
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

const COLORS: [&str; 4] = ["red", "green", "blue", "purple"];

fn save_svg<P: AsRef<Path>>(
    path: P,
    nodes: &[(f64, f64)],
    ways: &HashMap<usize, Vec<usize>>,
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

    for (x, y) in nodes {
        let color = (((y - ymin) / side).floor() as usize + ((x - xmin) / side).floor() as usize)
            % COLORS.len();
        writeln!(
            &mut writer,
            "<circle cx='{}' cy='{}' fill='{}' r='0.8%'/>",
            x, y, COLORS[color]
        )?;
    }
    for way_points in ways.values() {
        way_points.iter().tuple_windows().try_for_each(|(i1, i2)| {
            let (x1, y1) = nodes[*i1];
            let (x2, y2) = nodes[*i2];
            writeln!(
                &mut writer,
                "<line x1='{x1}' y1='{y1}' x2='{x2}' y2='{y2}' stroke='black' stroke-width='0.2%'/>",
            )
        })?;
    }

    writeln!(&mut writer, "</g></svg>",)?;

    Ok(())
}

const SIDE: f64 = 1. / 1000.;

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
    let (mut nodes, mut ways, streets) = parse_osm_xml(std::str::from_utf8(&answer).unwrap());
    let mut renamed_nodes = rename_nodes(nodes, &mut ways);
    eprintln!(
        "we have {} nodes and {} streets",
        renamed_nodes.len(),
        streets.len()
    );
    cut_ways_at_squares(&mut renamed_nodes, &mut ways, (bbox.0, bbox.1), SIDE);
    eprintln!("after cutting we have {} nodes", renamed_nodes.len());
    let (squared_nodes, indices, starts, squares_per_line) =
        group_nodes_in_squares(&renamed_nodes, bbox, SIDE);
    eprintln!(
        "we still have {} nodes in {} squares",
        squared_nodes.len(),
        starts.len()
    );
    save_svg("test.svg", &renamed_nodes, &ways, bbox, SIDE)?;
    Ok(())
}
