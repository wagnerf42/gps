use gps::Map;
use itertools::Itertools;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gpx_filename = std::env::args()
        .nth(1)
        .unwrap_or("retours_route.gpx".to_owned());
    let gpx_file = std::fs::File::open(gpx_filename)?;
    let gpx_reader = std::io::BufReader::new(gpx_file);

    let key_values: Vec<(String, String)> = std::env::args()
        .skip(3)
        .filter_map(|key_slash_value| {
            key_slash_value
                .split('/')
                .map(|s| s.to_owned())
                .tuples()
                .next()
        })
        .collect();

    let map_data = std::env::args()
        .nth(2)
        .and_then(|map_name| Map::load(map_name, &key_values).ok());
    let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create("out.gps").await?);
    gps::convert_gpx(gpx_reader, map_data, &key_values, &mut writer).await?;
    writer.flush().await?;
    Ok(())
}
