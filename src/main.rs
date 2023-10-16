use gps::{disable_elevation, Node};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key_values = [
        ("shop".to_owned(), "bakery".to_string()),
        ("amenity".to_owned(), "drinking_water".to_string()),
        ("amenity".to_owned(), "toilets".to_string()),
        ("tourism".to_owned(), "artwork".to_string()),
    ];

    let (mut gps, map_name) = if std::env::args().len() == 2 {
        let gpx_filename = std::env::args().nth(1).unwrap();

        let gps = gps::load_gps_from_file(&gpx_filename, true)?;
        let mut map_name: std::path::PathBuf = (&gpx_filename).into();
        map_name.set_extension("map");
        (gps, map_name)
    } else {
        let mut coordinates = std::env::args()
            .skip(1)
            .filter_map(|a| a.parse::<f64>().ok());
        let xmin = coordinates.next().unwrap();
        let ymin = coordinates.next().unwrap();
        let width = coordinates.next().unwrap();
        let height = coordinates.next().unwrap();

        let map_name: std::path::PathBuf =
            format!("area_[{xmin}_{ymin}_{width}_{height}].map").into();

        let gps = gps::Gps::from_area(vec![
            Node::new(xmin, ymin),
            Node::new(xmin + width, ymin),
            Node::new(xmin + width, ymin + height),
            Node::new(xmin, ymin + height),
        ]);
        (gps, map_name)
    };
    let mut gps_name: std::path::PathBuf = map_name.clone();
    gps_name.set_extension("gps");

    if gps.load_map(&map_name, &key_values).is_err() {
        gps.request_map(&key_values, Some(map_name)).await
    }
    // disable_elevation(&mut gps);
    gps.save_svg("map.svg").expect("failed saving svg file");

    let mut writer = std::io::BufWriter::new(std::fs::File::create(gps_name)?);
    gps.write_gps(&mut writer)?;
    Ok(())
}
