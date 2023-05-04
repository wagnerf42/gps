use itertools::Itertools;
use std::collections::HashSet;

use gps::Node;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key_values = [
        ("shop".to_owned(), "bakery".to_string()),
        ("amenity".to_owned(), "drinking_water".to_string()),
        ("amenity".to_owned(), "toilets".to_string()),
        ("tourism".to_owned(), "artwork".to_string()),
    ];

    if std::env::args().len() == 2 {
        let gpx_filename = std::env::args()
            .nth(1)
            .unwrap_or("retours_route.gpx".to_owned());

        let (waypoints, path) = gps::load_gpx(&gpx_filename)?;
        let mut map_name: std::path::PathBuf = (&gpx_filename).into();
        map_name.set_extension("map");
        let mut gps_name: std::path::PathBuf = gpx_filename.into();
        gps_name.set_extension("gps");

        let (map, interests) =
            if let Ok(loaded) = gps::load_map_and_interests(&map_name, &key_values) {
                loaded
            } else {
                gps::request_map_from_path(&path, &key_values, &map_name)
                    .await
                    .expect("failed requesting map")
            };

        let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create(gps_name).await?);
        gps::convert_gpx(Some(&waypoints), Some(&path), map, interests, &mut writer).await?;
        writer.flush().await?;
    } else {
        let mut coordinates = std::env::args()
            .skip(1)
            .filter_map(|a| a.parse::<f64>().ok());
        let xmin = coordinates.next().unwrap();
        let ymin = coordinates.next().unwrap();
        let width = coordinates.next().unwrap();
        let height = coordinates.next().unwrap();

        let gpx_filename = format!("area_[{xmin}_{ymin}_{width}_{height}].gpx");

        let mut map_name: std::path::PathBuf = (&gpx_filename).into();
        map_name.set_extension("map");
        let mut gps_name: std::path::PathBuf = gpx_filename.into();
        gps_name.set_extension("gps");

        let (mut map, interests) =
            if let Ok(loaded) = gps::load_map_and_interests(&map_name, &key_values) {
                loaded
            } else {
                let area = vec![
                    Node::new(xmin, ymin),
                    Node::new(xmin + width, ymin),
                    Node::new(xmin + width, ymin + height),
                    Node::new(xmin, ymin + height),
                ];
                gps::request_map_from_path(&area, &key_values, &map_name)
                    .await
                    .expect("failed requesting map")
            };
        let min_x_tile = (xmin / gps::SIDE).floor() as usize;
        let max_x_tile = ((xmin + width) / gps::SIDE).floor() as usize;
        let min_y_tile = (ymin / gps::SIDE).floor() as usize;
        let max_y_tile = ((ymin + height) / gps::SIDE).floor() as usize;
        let tiles_wanted = (min_x_tile..=max_x_tile)
            .cartesian_product(min_y_tile..=max_y_tile)
            .map(|(x, y)| (x - map.first_tile.0, y - map.first_tile.1))
            .collect::<HashSet<_>>();
        map.keep_tiles(&tiles_wanted);

        let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create(gps_name).await?);
        gps::convert_gpx(None, None, map, interests, &mut writer).await?;
        writer.flush().await?;
    }
    Ok(())
}
