use std::path::Path;
use std::{collections::HashSet, io::Write};

use itertools::Itertools;

use crate::{grid_coordinates_between, Map, Node};

pub type SvgW<'a> = &'a dyn Svg<std::io::BufWriter<std::fs::File>>;

const COLORS: [&str; 6] = ["black", "red", "green", "blue", "purple", "cyan"];

pub trait Svg<W: Write> {
    fn write_svg(&self, writer: &mut W, color: &str) -> std::io::Result<()>;
}

pub struct UniColorNodes(pub Vec<Node>);

impl<W: Write> Svg<W> for UniColorNodes {
    fn write_svg(&self, writer: &mut W, color: &str) -> std::io::Result<()> {
        self.0.iter().try_for_each(|n| {
            writeln!(
                writer,
                "<circle cx='{}' cy='{}' fill='{color}' r='0.1%'/>",
                n.x, n.y,
            )
        })
    }
}

impl<W: Write> Svg<W> for &[Node] {
    fn write_svg(&self, writer: &mut W, color: &str) -> std::io::Result<()> {
        self.iter().tuple_windows().try_for_each(|(n1, n2)| {
            writeln!(
                writer,
                "<line x1='{}' y1='{}' x2='{}' y2='{}' stroke='{color}' stroke-width='0.1%'/>",
                n1.x, n1.y, n2.x, n2.y
            )
        })
    }
}

impl<W: Write> Svg<W> for Vec<Vec<Node>> {
    fn write_svg(&self, writer: &mut W, color: &str) -> std::io::Result<()> {
        self.iter().try_for_each(|v| {
            v.iter().tuple_windows().try_for_each(|(n1, n2)| {
                writeln!(
                    writer,
                    "<line x1='{}' y1='{}' x2='{}' y2='{}' stroke='{color}' stroke-width='0.1%'/>",
                    n1.x, n1.y, n2.x, n2.y
                )
            })
        })
    }
}

impl<W: Write> Svg<W> for Node {
    fn write_svg(&self, writer: &mut W, color: &str) -> std::io::Result<()> {
        writeln!(
            writer,
            "<circle cx='{}' cy='{}' fill='{color}' r='0.4%'/>",
            self.x, self.y,
        )
    }
}

impl<W: Write> Svg<W> for Map {
    fn write_svg(&self, writer: &mut W, color: &str) -> std::io::Result<()> {
        let (xmin, ymin, xmax, ymax) = self.bounding_box();
        for x in grid_coordinates_between(xmin, xmax, self.side) {
            writeln!(
            writer,
            "<line x1='{x}' y1= '{ymin}' x2='{x}' y2='{ymax}' stroke='grey' stroke-width='0.2%'/>"
        )?;
        }

        for y in grid_coordinates_between(ymin, ymax, self.side) {
            writeln!(
            writer,
            "<line x1='{xmin}' y1= '{y}' x2='{xmax}' y2='{y}' stroke='grey' stroke-width='0.2%'/>"
        )?;
        }

        self.ways()
            .try_for_each(|w| w.as_slice().write_svg(writer, color))
    }
}

pub(crate) struct MapTiles<'a> {
    pub tiles: &'a HashSet<(usize, usize)>,
    pub map: &'a Map,
}

impl<'a, W: Write> Svg<W> for MapTiles<'a> {
    fn write_svg(&self, writer: &mut W, color: &str) -> std::io::Result<()> {
        for (tile_x, tile_y) in self.tiles {
            let x = (self.map.first_tile.0 + tile_x) as f64 * self.map.side;
            let y = (self.map.first_tile.1 + tile_y) as f64 * self.map.side;
            writeln!(
                writer,
                "<rect x='{x}' y='{y}' width='{}' height='{}' opacity='0.5' fill='{color}'/>",
                self.map.side, self.map.side,
            )?;
        }
        Ok(())
    }
}

pub fn save_svg<
    'a,
    P: AsRef<Path>,
    S: IntoIterator<Item = &'a dyn Svg<std::io::BufWriter<std::fs::File>>>,
>(
    path: P,
    bounding_box: (f64, f64, f64, f64),
    content: S,
) -> std::io::Result<()> {
    let mut writer = std::io::BufWriter::new(std::fs::File::create(path)?);
    save_svg_to_writer(&mut writer, bounding_box, content)
}

pub fn save_svg_to_writer<'a, W: Write + 'a, S: IntoIterator<Item = &'a dyn Svg<W>>>(
    writer: &mut W,
    bounding_box: (f64, f64, f64, f64),
    content: S,
) -> std::io::Result<()> {
    let (xmin, ymin, xmax, ymax) = bounding_box;

    writeln!(
        writer,
        "<svg width='800' height='600' viewBox='{} {} {} {}'>",
        xmin,
        ymin,
        xmax - xmin,
        ymax - ymin
    )?;
    writeln!(
        writer,
        "<rect fill='white' x='{}' y='{}' width='{}' height='{}'/>",
        xmin,
        ymin,
        xmax - xmin,
        ymax - ymin
    )?;

    writeln!(
        writer,
        "<g transform='translate(0, {}) scale(1,-1)'>",
        ymin + ymax
    )?;
    content
        .into_iter()
        .zip(COLORS.iter().cycle())
        .try_for_each(|(c, color)| c.write_svg(writer, color))?;

    writeln!(writer, "</g></svg>",)?;

    Ok(())
}
