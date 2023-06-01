use itertools::Itertools;
use std::{
    collections::{HashMap, HashSet},
    f64::consts::{FRAC_PI_2, PI},
    io::Write,
};

use crate::Node;

use std::ops::{Add, Mul, Sub};

pub struct Vector([f64; 2]);

impl Vector {
    pub fn x(&self) -> f64 {
        self.0[0]
    }
    pub fn y(&self) -> f64 {
        self.0[1]
    }
    /// Compute angle between vector and x axis (will be strictly less than PI).
    pub fn angle(&self) -> f64 {
        self.y().atan2(self.x())
    }
}

impl Add<Vector> for &Node {
    type Output = Node;

    fn add(self, rhs: Vector) -> Self::Output {
        Node::new(self.x + rhs.x(), self.y + rhs.y())
    }
}

impl Mul<f64> for Vector {
    type Output = Vector;

    fn mul(self, rhs: f64) -> Self::Output {
        Vector([self.x() * rhs, self.y() * rhs])
    }
}

impl Sub<Node> for Node {
    type Output = Vector;

    fn sub(self, rhs: Node) -> Self::Output {
        Vector([self.x - rhs.x, self.y - rhs.y])
    }
}

impl Sub<Node> for &Node {
    type Output = Vector;

    fn sub(self, rhs: Node) -> Self::Output {
        Vector([self.x - rhs.x, self.y - rhs.y])
    }
}

impl Sub<&Node> for Node {
    type Output = Vector;

    fn sub(self, rhs: &Node) -> Self::Output {
        Vector([self.x - rhs.x, self.y - rhs.y])
    }
}

impl Sub<&Node> for &Node {
    type Output = Vector;

    fn sub(self, rhs: &Node) -> Self::Output {
        Vector([self.x - rhs.x, self.y - rhs.y])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Segment([Node; 2]);

impl<W: Write> crate::Svg<W> for &[Segment] {
    fn write_svg(&self, writer: &mut W, color: &str) -> std::io::Result<()> {
        self.iter().try_for_each(|s| {
            writeln!(
                writer,
                "<line x1='{}' y1='{}' x2='{}' y2='{}' stroke='{color}' stroke-width='0.1%'/>",
                s.start().x,
                s.start().y,
                s.end().x,
                s.end().y
            )
        })
    }
}

impl Segment {
    pub fn new(start: Node, end: Node) -> Self {
        Segment([start, end])
    }
    pub fn reverse(&self) -> Self {
        Segment([*self.end(), *self.start()])
    }
    pub fn start(&self) -> &Node {
        &self.0[0]
    }
    pub fn end(&self) -> &Node {
        &self.0[1]
    }

    /// return a parallel segment at distance "thickness"
    pub fn parallel_segment(&self, thickness: f64) -> Segment {
        let xdiff = self.end().x - self.start().x;
        let ydiff = self.end().y - self.start().y;
        let d = (xdiff * xdiff + ydiff * ydiff).sqrt();
        let x = (-ydiff / d) * thickness;
        let y = (xdiff / d) * thickness;
        assert!(!x.is_nan());
        assert!(!y.is_nan());
        Segment::new(
            Node::new(self.start().x + x, self.start().y + y),
            Node::new(self.end().x + x, self.end().y + y),
        )
    }

    /// Intersect with horizontal line at given y.
    /// Returns only x coordinate of intersection.
    /// Precondition: we are not a quasi-horizontal segment.
    pub fn horizontal_line_intersection(&self, y: f64) -> f64 {
        let alpha = (y - self.start().y) / (self.end().y - self.start().y);
        alpha.mul_add(self.end().x - self.start().x, self.start().x)
    }

    /// Intersects two segments.
    pub fn intersection_with(&self, other: &Segment) -> Option<Node> {
        // we solve system obtained by considering the point is inside both segments.
        // p = self.start + alpha * self.direction_vector()
        // p = other.start + beta * self.direction_vector()
        let d = self.end() - self.start();
        let d2 = other.end() - other.start();
        let denominator = d2.x() * d.y() - d.x() * d2.y();
        if is_almost(denominator, 0.0) {
            None // almost parallel lines
        } else {
            let alpha = (d2.x() * (other.start().y - self.start().y)
                + d2.y() * (self.start().x - other.start().x))
                / denominator;
            let beta = (d.x() * (other.start().y - self.start().y)
                + d.y() * (self.start().x - other.start().x))
                / denominator;
            if (is_almost(0.0, alpha) || is_almost(1.0, alpha) || (0.0 < alpha && alpha < 1.0))
                && (is_almost(0.0, beta) || is_almost(1.0, beta) || (0.0 < beta && beta < 1.0))
            {
                Some(self.start() + d * alpha)
            } else {
                None
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Polygon(Vec<Node>);

impl<W: Write> crate::Svg<W> for Polygon {
    fn write_svg(&self, writer: &mut W, color: &str) -> std::io::Result<()> {
        write!(writer, "<polygon points=\"")?;

        self.0
            .iter()
            .try_for_each(|n| write!(writer, " {},{}", n.x, n.y,))?;
        writeln!(writer, "\" fill=\"{}\" opacity=\"0.5\"/>", color)
    }
}

impl Polygon {
    fn new(points: Vec<Node>) -> Self {
        Polygon(points)
    }

    /// Return area (SIGNED) of polygon.
    /// pre-condition: at least 3 points
    fn area(&self) -> f64 {
        assert!(self.0.len() >= 3);
        self.0
            .windows(2)
            .chain(std::iter::once(
                vec![
                    self.0.last().cloned().unwrap(),
                    self.0.first().cloned().unwrap(),
                ]
                .as_slice(),
            ))
            .map(|p| p[0].x * p[1].y - p[0].y * p[1].x)
            .sum::<f64>()
            / 2.0
    }
}

/// Intersect all segments and return all smaller segments.
pub fn intersect_segments(segments: &[Segment]) -> Vec<Segment> {
    let intersections = find_segments_intersections(segments);
    segments
        .iter()
        .flat_map(|s| {
            std::iter::Iterator::flatten(intersections.get(s).into_iter())
                .chain(std::iter::once(s.start()))
                .chain(std::iter::once(s.end()))
                .copied()
                .sorted_by(|p1, p2| {
                    if s.start().partial_cmp(s.end()) == Some(std::cmp::Ordering::Less) {
                        p1.partial_cmp(p2)
                    } else {
                        p2.partial_cmp(p1)
                    }
                    .unwrap()
                })
                .dedup()
                .tuple_windows()
                .map(|(p1, p2)| Segment::new(p1, p2))
        })
        .collect()
}

fn find_segments_intersections(segments: &[Segment]) -> HashMap<&Segment, Vec<Node>> {
    // let's go brute force for now
    let mut intersections: HashMap<&Segment, Vec<Node>> = HashMap::new();
    segments
        .iter()
        .tuple_combinations()
        .filter_map(|(s1, s2)| s1.intersection_with(s2).map(|i| (s1, s2, i)))
        .for_each(|(s1, s2, i)| {
            if !(i.is_almost(s1.start()) || i.is_almost(s1.end())) {
                intersections.entry(s1).or_default().push(i);
            }
            if !(i.is_almost(s2.start()) || i.is_almost(s2.end())) {
                intersections.entry(s2).or_default().push(i);
            }
        });
    intersections
}

/// Are the two given floats almost equals ?
pub fn is_almost(f1: f64, f2: f64) -> bool {
    (f1 - f2).abs() < 10.0_f64.powi(-6)
}

/// Converts segment into oriented polygons (clockwise) by following edges.
/// Flat polygons are discarded in the process.
pub fn build_polygons(segments: &[Segment]) -> Vec<Polygon> {
    let mut points = HashMap::new();
    let mut remaining_segments = HashSet::new();
    for segment in segments {
        points
            .entry(*segment.start())
            .or_insert_with(Vec::new)
            .push((*segment.end(), true));

        points
            .entry(*segment.end())
            .or_insert_with(Vec::new)
            .push((*segment.start(), false));
        remaining_segments.insert(segment);
    }
    for (point, neighbours) in &mut points {
        neighbours.sort_by(|(p1, _), (p2, _)| {
            (p1 - point)
                .angle()
                .partial_cmp(&(p2 - point).angle())
                .unwrap()
        })
    }

    let mut polygons = Vec::new();
    while !remaining_segments.is_empty() {
        let next_start_segment = *remaining_segments.iter().next().unwrap();
        remaining_segments.remove(&next_start_segment);
        if let Some(polygon) = build_polygon(next_start_segment, &points, &mut remaining_segments) {
            polygons.push(polygon);
        }
    }
    polygons
}

/// Builds polygon obtained when following segment. Might return None if obtained polygon is flat.
fn build_polygon(
    start_segment: &Segment,
    points: &HashMap<Node, Vec<(Node, bool)>>,
    remaining_segments: &mut HashSet<&Segment>,
) -> Option<Polygon> {
    // let mut seen_points = HashSet::new();
    let starting_point = *start_segment.start();
    let mut previous_point = starting_point;
    let mut current_point = *start_segment.end();
    let mut polygon_points = vec![starting_point];
    remaining_segments.remove(start_segment);
    //follow edge until we come back to our starting point
    while current_point != starting_point {
        polygon_points.push(current_point);
        let next_point = find_next_point(&points[&current_point], &current_point, &previous_point);
        remaining_segments.remove(&Segment::new(current_point, next_point));
        previous_point = current_point;
        current_point = next_point;
    }
    let polygon = Polygon::new(polygon_points);
    Some(polygon)
}

fn find_next_point(
    neighbours: &[(Node, bool)],
    current_point: &Node,
    previous_point: &Node,
) -> Node {
    let incoming_angle = (previous_point - current_point).angle();
    let index = neighbours
        .binary_search_by(|(p, _)| {
            (p - current_point)
                .angle()
                .partial_cmp(&incoming_angle)
                .unwrap()
        })
        .unwrap();
    // let's find the 'first' leaving segment
    neighbours[index..]
        .iter()
        .skip(1)
        .chain(neighbours.iter())
        .scan(0, |leaving_count, (p, leaving)| {
            *leaving_count += if *leaving { 1 } else { -1 };
            Some((p, *leaving_count))
        })
        .find_map(|(p, count)| (count == 1).then_some(p))
        .copied()
        .unwrap()
}

pub fn inflate_polyline(path: &[Node], thickness: f64) -> Vec<Node> {
    eprintln!("inflating");
    let (xmin, xmax) = path.iter().map(|p| p.x).minmax().into_option().unwrap();

    let (ymin, ymax) = path.iter().map(|p| p.y).minmax().into_option().unwrap();

    crate::save_svg("path.svg", (xmin, ymin, xmax, ymax), [&path as crate::SvgW]).unwrap();
    eprintln!("nodes are {path:?}");

    let segments = path
        .iter()
        .tuple_windows()
        .map(|(p1, p2)| Segment::new(*p1, *p2))
        .collect::<Vec<_>>();

    let mut around_path: Vec<Segment> = path
        .iter()
        .chain(path.iter().rev().skip(1))
        .chain(std::iter::once(&path[1]))
        .dedup()
        .tuple_windows()
        .flat_map(|(p1, p2, p3)| {
            let s1 = Segment::new(*p1, *p2).parallel_segment(thickness);
            let s2 = Segment::new(*p2, *p3).parallel_segment(thickness);
            // if let Some(i) = s1.intersection_with(&s2) {
            //     let s = vec![Segment::new(s1.start, i), Segment::new(i, s2.end)];
            //     tycat!(s1, s2, p1, p2, p3, s);
            //     return s;
            // } // TODO: we could uncomment it but not as is because intersections impact next triplet
            let v1 = s1.end() - p2;
            let v2 = s2.start() - p2;
            let mut a1 = v1.y().atan2(v1.x());
            let mut a2 = v2.y().atan2(v2.x());

            let opposite_v = p3 - p2;
            let opposite_angle = opposite_v.y().atan2(opposite_v.x());

            if if p1 == p3 {
                if is_almost(FRAC_PI_2, opposite_angle) {
                    a1 = 0.;
                    a2 = -PI;
                    false
                } else if is_almost(-FRAC_PI_2, opposite_angle) {
                    a1 = PI;
                    a2 = 0.;
                    false
                } else {
                    (-FRAC_PI_2 < opposite_angle) && (opposite_angle < FRAC_PI_2)
                }
            } else {
                (a2 - a1).abs() > PI
            } {
                a1 = (a1 + 2. * PI) % (2. * PI);
                a2 = (a2 + 2. * PI) % (2. * PI);
            }

            assert!((a2 - a1).abs() <= PI + 0.00001);
            let points = (1..10)
                .map(|c| (a2 * c as f64 + a1 * (10. - c as f64)) / 10.)
                .map(|a| Node::new(a.cos() * thickness + p2.x, a.sin() * thickness + p2.y));
            let s = std::iter::once(*s1.end())
                .chain(points)
                .chain(std::iter::once(*s2.start()))
                .dedup()
                .tuple_windows()
                .map(|(p1, p2)| Segment::new(p1, p2));

            let full_s = std::iter::once(s1).chain(s).collect::<Vec<_>>();
            // tycat!(full_s, p1, p2, p3, s1, s2);
            full_s
        })
        .collect();
    eprintln!(
        "we have {} segments, computing small ones",
        around_path.len()
    );

    let small_segments = intersect_segments(&around_path);
    crate::save_svg(
        "small.svg",
        (xmin, ymin, xmax, ymax),
        [&small_segments.as_slice() as crate::SvgW],
    )
    .unwrap();
    eprintln!("building polygons");

    let mut polygons = build_polygons(&small_segments);

    polygons.retain(|p| p.area() > 0.);
    let outer_poly_index = polygons
        .iter()
        .enumerate()
        .min_by(|(_, p1), (_, p2)| p1.area().partial_cmp(&p2.area()).unwrap())
        .map(|(i, _)| i)
        .unwrap();
    let mut outer_poly = polygons.swap_remove(outer_poly_index);
    // eprintln!("remaining polygons: {}", polygons.len());

    // crate::save_svg(
    //     "outer.svg",
    //     (xmin, ymin, xmax, ymax),
    //     [
    //         &path as crate::SvgW,
    //         &outer_poly as crate::SvgW,
    //         &polygons[0] as crate::SvgW,
    //         &polygons[1] as crate::SvgW,
    //         &polygons[2] as crate::SvgW,
    //         &polygons[3] as crate::SvgW,
    //     ],
    // )
    // .unwrap();

    destroy_holes(&mut outer_poly, polygons);
    outer_poly.0
}

fn destroy_holes(outer_poly: &mut Polygon, mut holes: Vec<Polygon>) {
    holes.iter_mut().for_each(start_at_xmin);
    for hole in holes {
        eat_hole(outer_poly, hole)
    }
}

fn eat_hole(outer_poly: &mut Polygon, hole: Polygon) {
    let p = *hole.0.first().unwrap();
    let (segment_index, nearest_segment_point, _) = polygon_segments(outer_poly)
        .enumerate()
        .filter_map(|(i, s)| {
            if is_almost(s.start().y, s.end().y)
                || s.start().y.partial_cmp(&p.y).unwrap() == s.end().y.partial_cmp(&p.y).unwrap()
            {
                None
            } else {
                let x = s.horizontal_line_intersection(p.y);
                let distance = (x - p.x).abs();
                Some((i, Node::new(x, p.y), distance))
            }
        })
        .min_by(|(_, _, d1), (_, _, d2)| d1.partial_cmp(d2).unwrap())
        .unwrap();
    let big_poly = outer_poly.0[0..=segment_index]
        .iter()
        .chain(std::iter::once(&nearest_segment_point))
        .chain(hole.0.iter())
        .chain(hole.0.first())
        .chain(std::iter::once(&nearest_segment_point))
        .chain(outer_poly.0[segment_index..].iter().skip(1))
        .dedup()
        .copied()
        .collect::<Vec<Node>>();
    *outer_poly = Polygon::new(big_poly);
}
fn polygon_segments(poly: &Polygon) -> impl Iterator<Item = Segment> + '_ {
    poly.0
        .windows(2)
        .map(|w| Segment::new(w[0], w[1]))
        .chain(std::iter::once(Segment::new(
            *poly.0.last().unwrap(),
            *poly.0.first().unwrap(),
        )))
}

fn start_at_xmin(polygon: &mut Polygon) {
    let x_min_point_index = polygon
        .0
        .iter()
        .enumerate()
        .min_by(|(_, p1), (_, p2)| p1.x.partial_cmp(&p2.x).unwrap())
        .map(|(i, _)| i)
        .unwrap();
    let new_points = polygon.0[x_min_point_index..]
        .iter()
        .chain(&polygon.0[..x_min_point_index])
        .copied()
        .collect::<Vec<_>>();
    polygon.0 = new_points;
}
