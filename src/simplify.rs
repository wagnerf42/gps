use super::Node;
use std::collections::{hash_map::Entry, HashMap};

pub fn simplify_path(points: &[Node], epsilon: f64) -> Vec<Node> {
    if points.len() <= 600 {
        optimal_simplification(points, epsilon)
    } else {
        hybrid_simplification(points, epsilon)
    }
}

pub fn optimal_simplification(points: &[Node], epsilon: f64) -> Vec<Node> {
    let mut cache = HashMap::new();
    simplify_prog_dyn(points, 0, points.len(), epsilon, &mut cache);
    extract_prog_dyn_solution(points, 0, points.len(), &cache)
}

pub fn optimal_simplification2(points: &[Node], epsilon: f64) -> Vec<Node> {
    let mut cache = HashMap::new();
    let mut dist_cache = HashMap::new();
    simplify_prog_dyn2(
        points,
        0,
        points.len(),
        epsilon,
        &mut cache,
        &mut dist_cache,
    );
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
                    .squared_distance_to(p1)
                    .partial_cmp(&first.squared_distance_to(p2))
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
    if let Some(choice) = cache.get(&(start, end)).and_then(|c| c.0) {
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

fn points_near_enough(
    points: &[Node],
    start: usize,
    end: usize,
    epsilon: f64,
    dist_cache: &mut HashMap<(usize, usize), bool>,
) -> bool {
    match dist_cache.entry((start, end)) {
        Entry::Occupied(o) => *o.get(),
        Entry::Vacant(v) => {
            let first_point = &points[start];
            let last_point = &points[end - 1];
            let near_enough = points[(start + 1)..end]
                .iter()
                .map(|p| p.distance_to_segment(first_point, last_point))
                .all(|d| d <= epsilon);
            v.insert(near_enough);
            near_enough
        }
    }
}

fn simplify_prog_dyn2(
    points: &[Node],
    start: usize,
    end: usize,
    epsilon: f64,
    cache: &mut HashMap<(usize, usize), (Option<usize>, usize)>,
    dist_cache: &mut HashMap<(usize, usize), bool>,
) -> usize {
    if let Some(val) = cache.get(&(start, end)) {
        val.1
    } else {
        let res = if end - start <= 2 {
            assert_eq!(end - start, 2);
            (None, end - start)
        } else {
            if points_near_enough(points, start, end, epsilon, dist_cache) {
                (None, 2)
            } else {
                // now we test all possible cutting points
                ((start + 1)..(end - 1)) //TODO: take middle min
                    .filter_map(|i| {
                        if points_near_enough(points, i, end, epsilon, dist_cache) {
                            let v1 = simplify_prog_dyn2(
                                points,
                                start,
                                i + 1,
                                epsilon,
                                cache,
                                dist_cache,
                            );
                            Some((Some(i), v1 + 2 - 1))
                        } else {
                            None
                        }
                    })
                    .min_by_key(|(_, v)| *v)
                    .unwrap()
            }
        };
        cache.insert((start, end), res);
        res.1
    }
}
