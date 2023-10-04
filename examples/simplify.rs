use gps::parse_gpx_points;
use gps::{optimal_simplification, optimal_simplification2};

fn main() -> std::io::Result<()> {
    let path = std::env::args().nth(1).expect("missing gpx file");
    let gpx_file = std::fs::File::open(path)?;
    let gpx_reader = std::io::BufReader::new(gpx_file);
    let points = parse_gpx_points(gpx_reader).1;

    println!("starting with {} points", points.len());

    let start = std::time::Instant::now();
    let p2 = optimal_simplification(&points, 0.00015);
    let elapsed = start.elapsed();
    println!("opt : down to {} in {:?}", p2.len(), elapsed);

    let start = std::time::Instant::now();
    let p2 = optimal_simplification2(&points, 0.00015);
    let elapsed = start.elapsed();
    println!("opt2 : down to {} in {:?}", p2.len(), elapsed);
    Ok(())
}
