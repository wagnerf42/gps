use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct Node {
    pub x: f64,
    pub y: f64,
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.x.to_bits() == other.x.to_bits() && self.y.to_bits() == other.y.to_bits()
    }
}
impl Eq for Node {}
impl std::hash::Hash for Node {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.x.to_bits().hash(state);
        self.y.to_bits().hash(state);
    }
}

impl Node {
    pub fn new(x: f64, y: f64) -> Self {
        Node { x, y }
    }
    pub fn squared_distance_between(&self, other: &Node) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        dx * dx + dy * dy
    }
    pub fn distance_to_segment(&self, v: &Node, w: &Node) -> f64 {
        let l2 = v.squared_distance_between(w);
        if l2 == 0.0 {
            return self.squared_distance_between(v).sqrt();
        }
        // Consider the line extending the segment, parameterized as v + t (w - v).
        // We find projection of point p onto the line.
        // It falls where t = [(p-v) . (w-v)] / |w-v|^2
        // We clamp t from [0,1] to handle points outside the segment vw.
        let x0 = self.x - v.x;
        let y0 = self.y - v.y;
        let x1 = w.x - v.x;
        let y1 = w.y - v.y;
        let dot = x0 * x1 + y0 * y1;
        let t = (dot / l2).clamp(0.0, 1.0);

        let proj = Node {
            x: v.x + x1 * t,
            y: v.y + y1 * t,
        };

        proj.squared_distance_between(self).sqrt()
    }
}

pub fn simplify_path(points: &[Node], epsilon: f64) -> Vec<Node> {
    if points.len() <= 600 {
        optimal_simplification(points, epsilon)
    } else {
        hybrid_simplification(points, epsilon)
    }
}

fn optimal_simplification(points: &[Node], epsilon: f64) -> Vec<Node> {
    let mut cache = HashMap::new();
    simplify_prog_dyn(points, 0, points.len(), epsilon, &mut cache);
    extract_prog_dyn_solution(points, 0, points.len(), &cache)
}

fn hybrid_simplification(points: &[Node], epsilon: f64) -> Vec<Node> {
    if points.len() <= 300 {
        optimal_simplification(points, epsilon)
    } else if points.first().unwrap() == points.last().unwrap() {
        let first = points.first().unwrap();
        let index_farthest = points
            .iter()
            .enumerate()
            .skip(1)
            .max_by(|(_, p1), (_, p2)| {
                first
                    .squared_distance_between(p1)
                    .partial_cmp(&first.squared_distance_between(p2))
                    .unwrap()
            })
            .map(|(i, _)| i)
            .unwrap();

        let start = &points[..(index_farthest + 1)];
        let end = &points[index_farthest..];
        let mut res = hybrid_simplification(start, epsilon);
        res.pop();
        res.append(&mut hybrid_simplification(end, epsilon));
        res
    } else {
        let (index_farthest, farthest_distance) = points
            .iter()
            .map(|p| p.distance_to_segment(points.first().unwrap(), points.last().unwrap()))
            .enumerate()
            .max_by(|(_, d1), (_, d2)| {
                if d1.is_nan() {
                    std::cmp::Ordering::Greater
                } else if d2.is_nan() {
                    std::cmp::Ordering::Less
                } else {
                    d1.partial_cmp(d2).unwrap()
                }
            })
            .unwrap();
        if farthest_distance <= epsilon {
            vec![
                points.first().copied().unwrap(),
                points.last().copied().unwrap(),
            ]
        } else {
            let start = &points[..(index_farthest + 1)];
            let end = &points[index_farthest..];
            let mut res = hybrid_simplification(start, epsilon);
            res.pop();
            res.append(&mut hybrid_simplification(end, epsilon));
            res
        }
    }
}

fn extract_prog_dyn_solution(
    points: &[Node],
    start: usize,
    end: usize,
    cache: &HashMap<(usize, usize), (Option<usize>, usize)>,
) -> Vec<Node> {
    if let Some(choice) = cache.get(&(start, end)).unwrap().0 {
        let mut v1 = extract_prog_dyn_solution(points, start, choice + 1, cache);
        let mut v2 = extract_prog_dyn_solution(points, choice, end, cache);
        v1.pop();
        v1.append(&mut v2);
        v1
    } else {
        vec![points[start], points[end - 1]]
    }
}

fn simplify_prog_dyn(
    points: &[Node],
    start: usize,
    end: usize,
    epsilon: f64,
    cache: &mut HashMap<(usize, usize), (Option<usize>, usize)>,
) -> usize {
    if let Some(val) = cache.get(&(start, end)) {
        val.1
    } else {
        let res = if end - start <= 2 {
            assert_eq!(end - start, 2);
            (None, end - start)
        } else {
            let first_point = &points[start];
            let last_point = &points[end - 1];

            if points[(start + 1)..end]
                .iter()
                .map(|p| p.distance_to_segment(first_point, last_point))
                .all(|d| d <= epsilon)
            {
                (None, 2)
            } else {
                // now we test all possible cutting points
                ((start + 1)..(end - 1)) //TODO: take middle min
                    .map(|i| {
                        let v1 = simplify_prog_dyn(points, start, i + 1, epsilon, cache);
                        let v2 = simplify_prog_dyn(points, i, end, epsilon, cache);
                        (Some(i), v1 + v2 - 1)
                    })
                    .min_by_key(|(_, v)| *v)
                    .unwrap()
            }
        };
        cache.insert((start, end), res);
        res.1
    }
}
