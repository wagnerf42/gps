use gps;

fn main() -> std::io::Result<()> {
    let key_values = [
        ("shop".to_owned(), "bakery".to_string()),
        ("amenity".to_owned(), "drinking_water".to_string()),
        ("amenity".to_owned(), "toilets".to_string()),
        ("tourism".to_owned(), "artwork".to_string()),
    ];
    let gpx_filename = std::env::args().nth(1).unwrap();

    let mut gps = gps::load_gps_from_file(&gpx_filename)?;
    let mut map_name: std::path::PathBuf = (&gpx_filename).into();
    map_name.set_extension("map");
    gps.load_map(&map_name, &key_values)?;

    // gps.detect_crossroads(); // DONE in load_map now

    gps.save_svg("map.svg").expect("failed saving svg file");

    std::process::Command::new("kitty")
        .arg("+kitten")
        .arg("icat")
        .arg("map.svg")
        .status()
        .expect("running kitty failed");

    Ok(())
}
