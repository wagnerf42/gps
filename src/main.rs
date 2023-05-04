use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gpx_filename = std::env::args()
        .nth(1)
        .unwrap_or("retours_route.gpx".to_owned());

    let (waypoints, path) = gps::load_gpx(&gpx_filename)?;
    let mut map_name: std::path::PathBuf = (&gpx_filename).into();
    map_name.set_extension("map");
    let mut gps_name: std::path::PathBuf = gpx_filename.into();
    gps_name.set_extension("gps");

    let key_values = [
        ("shop".to_owned(), "bakery".to_string()),
        ("amenity".to_owned(), "drinking_water".to_string()),
        ("amenity".to_owned(), "toilets".to_string()),
        ("tourism".to_owned(), "artwork".to_string()),
    ];

    let (map, interests) = if let Ok(loaded) = gps::load_map_and_interests(&map_name, &key_values) {
        loaded
    } else {
        gps::request_map_from_path(&path, &key_values, &map_name)
            .await
            .expect("failed requesting map")
    };

    let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create(gps_name).await?);
    gps::convert_gpx(&waypoints, &path, map, interests, &mut writer).await?;
    writer.flush().await?;
    Ok(())
}
