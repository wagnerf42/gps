// encode streets to binary

use std::collections::HashMap;

use crate::CWayId;

use unicode_categories::UnicodeCategories;
use unicode_normalization::UnicodeNormalization;

fn remove_accents(s: &str) -> String {
    //TODO: also remove front numbers
    s.nfd().filter(|&c| !c.is_mark_nonspacing()).collect()
}

pub(crate) fn encode_streets(streets: &HashMap<String, Vec<CWayId>>) -> Vec<u8> {
    // sort by alphabetical order
    let mut sorted_streets = streets.iter().collect::<Vec<_>>();
    sorted_streets.sort_unstable_by(|a, b| a.0.cmp(b.0));
    // now, let's do sqrt(n) blocks of sqrt(n) streets
    let block_size = (sorted_streets.len() as f64).sqrt().ceil() as usize;
    let mut blocks_labels = String::new();
    let mut encoded_blocks: Vec<u8> = Vec::new();
    let mut blocks_starts = Vec::new();
    for streets_chunk in sorted_streets.chunks(block_size) {
        let mut names = String::new();
        let mut ways = Vec::new();
        // let mut ways_starts = Vec::new();
        blocks_labels.push_str(&remove_accents(streets_chunk[0].0));
        blocks_labels.push('\n');
        for (street_name, street_ways) in streets_chunk {
            // ways_starts.push(ways.len());
            names.push_str(&remove_accents(street_name));
            names.push('\n');
            ways.extend((street_ways.len() as u16).to_le_bytes());
            for way in *street_ways {
                ways.extend(way.tile_number.to_le_bytes());
                ways.push(way.local_way_id);
            }
        }
        let mut raw_block = Vec::new();
        // raw_block.extend((ways_starts.len() as u16).to_le_bytes());
        // raw_block.extend(ways_starts.iter().flat_map(|s| (*s as u16).to_le_bytes()));
        raw_block.extend((ways.len() as u16).to_le_bytes());
        raw_block.extend(ways);
        raw_block.extend(names.as_bytes().iter().copied());

        let mut encoded_block = vec![0u8; 2 * raw_block.len()];
        let encoded = heatshrink::encode(
            &raw_block,
            &mut encoded_block,
            &heatshrink::Config::new(8, 6).unwrap(),
        )
        .expect("encoding failed");
        blocks_starts.push(encoded_blocks.len());
        encoded_blocks.extend(encoded);
    }
    let mut full_encoding = Vec::new();
    let full_size = 4 // size encoding
        + 2 // number of blocks
        + 2 // size of all blocks labels
        + blocks_labels.len() // label of each block
        + blocks_starts.len() * 4 // start offset of each block in the encoded part
        + encoded_blocks.len(); // binary encoding of each block

    full_encoding.extend((full_size as u32).to_le_bytes());
    full_encoding.extend((blocks_starts.len() as u16).to_le_bytes());
    full_encoding.extend((blocks_labels.len() as u16).to_le_bytes());
    full_encoding.extend(blocks_labels.as_bytes());
    for start in blocks_starts {
        full_encoding.extend((start as u32).to_le_bytes());
    }
    full_encoding.append(&mut encoded_blocks);
    full_encoding
}

pub fn decode_streets(encoded: &[u8]) -> HashMap<String, Vec<CWayId>> {
    let mut streets = HashMap::new();
    let real_size = encoded.len();
    let (size_encoding, encoded) = encoded.split_at(4);
    let stored_size = u32::from_le_bytes([
        size_encoding[0],
        size_encoding[1],
        size_encoding[2],
        size_encoding[3],
    ]);
    assert_eq!(stored_size as usize, real_size);
    let (blocks_number_encoding, encoded) = encoded.split_at(2);
    let blocks_number =
        u16::from_le_bytes([blocks_number_encoding[0], blocks_number_encoding[1]]) as usize;
    let (labels_size_encoding, encoded) = encoded.split_at(2);
    let labels_size = u16::from_le_bytes([labels_size_encoding[0], labels_size_encoding[1]]);
    let (binary_labels, encoded) = encoded.split_at(labels_size as usize);
    let _labels = String::from_utf8(binary_labels.to_owned()).unwrap();
    let (binary_blocks_starts, encoded) = encoded.split_at(blocks_number * 4);
    let blocks_starts = binary_blocks_starts
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize)
        .collect::<Vec<_>>();
    for block_id in 0..blocks_number {
        let start = blocks_starts[block_id];
        let end = blocks_starts
            .get(block_id + 1)
            .copied()
            .unwrap_or(encoded.len());
        let decoded_block = decode_block(&encoded[start..end]);
        streets.extend(decoded_block);
    }
    streets
}

fn decode_block(encoded_binary_block: &[u8]) -> Vec<(String, Vec<CWayId>)> {
    let mut decoded_binary_block = vec![0; encoded_binary_block.len() * 2]; // duh, what a crappy API
    let decoded = heatshrink::decode(
        encoded_binary_block,
        &mut decoded_binary_block,
        &heatshrink::Config::new(8, 6).unwrap(),
    )
    .unwrap();

    // let (binary_streets_number, decoded) = decoded.split_at(2);
    // let streets_number =
    //     u16::from_le_bytes([binary_streets_number[0], binary_streets_number[1]]) as usize;
    // let (binary_starts, decoded) = decoded.split_at(streets_number * 2);
    // let starts = binary_starts // TODO: do we really need them ?
    //     .chunks_exact(2)
    //     .map(|c| u16::from_le_bytes([c[0], c[1]]) as usize)
    //     .collect::<Vec<_>>();
    let (binary_ways_len, decoded) = decoded.split_at(2);
    let ways_len = u16::from_le_bytes([binary_ways_len[0], binary_ways_len[1]]) as usize;
    let (binary_ways, decoded) = decoded.split_at(ways_len);

    let ways = decode_ways(binary_ways);

    let names = String::from_utf8(decoded.to_owned()).unwrap();
    names.split('\n').map(|n| n.to_owned()).zip(ways).collect()
}

fn decode_ways(mut binary_ways: &[u8]) -> Vec<Vec<CWayId>> {
    let mut ways = Vec::new();
    while !binary_ways.is_empty() {
        let (binary_way_len, binary) = binary_ways.split_at(2);
        binary_ways = binary;
        let way_len = u16::from_le_bytes([binary_way_len[0], binary_way_len[1]]);
        let mut way = Vec::new();
        for _ in 0..way_len {
            let (binary_tile_number, binary) = binary_ways.split_at(2);
            binary_ways = binary;
            let tile_number = u16::from_le_bytes([binary_tile_number[0], binary_tile_number[1]]);
            let (local_way_id, binary) = binary_ways.split_first().unwrap();
            binary_ways = binary;
            way.push(CWayId {
                tile_number,
                local_way_id: *local_way_id,
            });
        }
        ways.push(way);
    }
    ways
}
