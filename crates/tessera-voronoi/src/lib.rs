//! Weighted Voronoi (power diagram) treemaps.
//!
//! Implements the iterative algorithm of Nocaj & Brandes, *"Computing Voronoi
//! Treemaps: Faster, Simpler, and Resolution-independent"* (EuroVis 2012),
//! with the stabilisation heuristics popularised by `d3-voronoi-map`.
//!
//! Every cell is the intersection of the container with half-planes, so all
//! cells of a convex container are convex. Cells are computed by clipping
//! (`O(n²·m)` per diagram) which beats a full power-diagram construction for
//! the `n ≤ ~100` sites a single treemap level ever holds.

mod geom;
mod rng;

pub use geom::{Polygon, Vec2};
pub use rng::SplitMix64;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Site {
    pub pos: Vec2,
    /// Power weight; a site's power distance to `p` is `|p - pos|² - weight`.
    pub weight: f64,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub max_iterations: usize,
    /// Stop once `Σ|area - target| / total_area` falls below this...
    pub convergence: f64,
    /// ...and no cell is off by more than this fraction of its own target,
    /// so small cells cannot hide inside the total (a 12 MB file drawn ten
    /// times too large is a rounding error overall, but a lie on screen).
    pub max_relative: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { max_iterations: 80, convergence: 0.01, max_relative: 0.35 }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Solution {
    pub sites: Vec<Site>,
    pub cells: Vec<Polygon>,
    pub error: f64,
    pub iterations: usize,
}

/// Power diagram of `sites` clipped to `container`; `out[i]` is site `i`'s cell.
///
/// Neighbours are visited nearest first. The power bisector between `i` and
/// `j` sits at distance `t = (d² + wᵢ - wⱼ) / 2d` from site `i`; once even the
/// heaviest remaining neighbour's bisector lies beyond the current cell's
/// radius, no further site can cut the cell and the loop stops early. This
/// makes the typical cost `O(n·k·m)` with `k` ≈ the number of real neighbours.
pub fn power_diagram(container: &Polygon, sites: &[Site], out: &mut Vec<Polygon>) {
    out.clear();
    let n = sites.len();
    let w_max = sites.iter().map(|s| s.weight).fold(f64::NEG_INFINITY, f64::max);
    let mut cur: Vec<Vec2> = Vec::with_capacity(16);
    let mut tmp: Vec<Vec2> = Vec::with_capacity(16);
    let mut order: Vec<(f64, usize)> = Vec::with_capacity(n);
    for (i, si) in sites.iter().enumerate() {
        cur.clear();
        cur.extend_from_slice(&container.pts);
        order.clear();
        order.extend(sites.iter().enumerate().filter(|&(j, _)| j != i).map(|(j, s)| ((s.pos - si.pos).len2(), j)));
        order.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let si2 = si.pos.len2();
        let mut r2 = max_dist2(&cur, si.pos);
        for &(d2, j) in &order {
            let sj = &sites[j];
            if d2 < 1e-18 {
                // Coincident sites: the later one yields to the earlier one.
                if j < i {
                    cur.clear();
                    break;
                }
                continue;
            }
            let d = d2.sqrt();
            let r = r2.sqrt();
            if (d2 + si.weight - w_max) / (2.0 * d) >= r && d2 >= w_max - si.weight {
                break;
            }
            if (d2 + si.weight - sj.weight) / (2.0 * d) >= r {
                continue;
            }
            let c = sj.pos.len2() - si2 - sj.weight + si.weight;
            geom::clip_into(&cur, (sj.pos - si.pos) * 2.0, c, &mut tmp);
            std::mem::swap(&mut cur, &mut tmp);
            if cur.len() < 3 {
                cur.clear();
                break;
            }
            r2 = max_dist2(&cur, si.pos);
        }
        out.push(Polygon { pts: cur.clone() });
    }
}

fn max_dist2(pts: &[Vec2], o: Vec2) -> f64 {
    pts.iter().map(|&p| (p - o).len2()).fold(0.0, f64::max)
}

/// (absolute error `Σ|area - target| / total`, worst relative error).
fn area_error(cells: &[Polygon], targets: &[f64], total: f64) -> (f64, f64) {
    let mut abs = 0.0;
    let mut rel: f64 = 0.0;
    for (c, &t) in cells.iter().zip(targets) {
        let d = (c.area() - t).abs();
        abs += d;
        if t > 0.0 {
            rel = rel.max(d / t);
        }
    }
    (abs / total, rel)
}

/// One number to compare solutions by: absolute error, with a small
/// penalty for the worst cell so a solution fixing it counts as better.
fn score((abs, rel): (f64, f64)) -> f64 {
    abs + 0.002 * rel.min(50.0)
}

/// Keeps every site inside its own cell: `|wᵢ - wⱼ| ≤ |sᵢ - sⱼ|²`.
fn handle_overweighted(sites: &mut [Site]) {
    let n = sites.len();
    for i in 0..n {
        for j in i + 1..n {
            let d2 = (sites[i].pos - sites[j].pos).len2();
            let (hi, lo) = if sites[i].weight > sites[j].weight { (i, j) } else { (j, i) };
            let over = sites[hi].weight - sites[lo].weight - d2;
            if over > 0.0 {
                sites[hi].weight -= over / 2.0;
                sites[lo].weight += over / 2.0;
            }
        }
    }
}

/// Initial placement: a Vogel (sunflower) spiral, largest value at the
/// centre. Gives even spacing and lets big cells settle centrally, which both
/// converges faster and reads better than uniform random sites.
pub fn initial_sites(container: &Polygon, values: &[f64], seed: u64) -> Vec<Site> {
    let n = values.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| values[b].total_cmp(&values[a]));
    let (min, max) = container.bbox();
    let c = container.centroid();
    let half = (max - min) * 0.5;
    let mut rng = SplitMix64::new(seed);
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    let avg_area = container.area() / n.max(1) as f64;
    let mut sites = vec![Site { pos: c, weight: avg_area * 0.5 }; n];
    for (rank, &i) in order.iter().enumerate() {
        let r = ((rank as f64 + 0.5) / n as f64).sqrt() * 0.92;
        let theta = rank as f64 * golden + rng.next_f64() * 0.3;
        let mut p = Vec2::new(c.x + r * theta.cos() * half.x, c.y + r * theta.sin() * half.y);
        if !container.contains(p) {
            p = container.random_point(&mut rng).unwrap_or(c);
        }
        // Tiny jitter avoids exactly coincident sites in degenerate containers.
        p.x += (rng.next_f64() - 0.5) * 1e-6 * half.x.max(1.0);
        sites[i].pos = p;
    }
    sites
}

/// Computes a Voronoi treemap level: one convex cell per value, with cell
/// areas proportional to `values` inside `container`.
///
/// `warm` seeds the solver with a previous solution (same length as
/// `values`), which makes successive layouts of changing data stable and
/// typically converge in a handful of iterations.
pub fn solve(container: &Polygon, values: &[f64], warm: Option<&[Site]>, seed: u64, settings: &Settings) -> Solution {
    let n = values.len();
    let total = container.area();
    if n == 0 || total <= 0.0 {
        return Solution::default();
    }
    if n == 1 {
        // A positive weight keeps this usable as a warm start once
        // siblings appear (weights are scaled multiplicatively).
        return Solution { sites: vec![Site { pos: container.centroid(), weight: total * 0.5 }], cells: vec![container.clone()], error: 0.0, iterations: 0 };
    }
    let sum: f64 = values.iter().map(|v| v.max(0.0)).sum();
    let targets: Vec<f64> = if sum > 0.0 { values.iter().map(|v| v.max(0.0) / sum * total).collect() } else { vec![total / n as f64; n] };

    // Weight updates are multiplicative, so a weight at (or near) zero can
    // never recover: keep every weight on the scale of the cell areas.
    let floor = total / n as f64 * 1e-6;
    let mut sites = match warm {
        Some(w) if w.len() == n => {
            let mut rng = SplitMix64::new(seed ^ 0x9e37_79b9);
            // Degenerate weights (all ~0, or not finite) carry no layout:
            // keep the positions, restart the weights.
            let usable = w.iter().all(|s| s.weight.is_finite()) && w.iter().any(|s| s.weight > floor);
            let fresh = total / n as f64 * 0.5;
            w.iter()
                .map(|s| {
                    let pos = if container.contains(s.pos) { s.pos } else { container.random_point(&mut rng).unwrap_or(container.centroid()) };
                    Site { pos, weight: if usable { s.weight.max(floor) } else { fresh } }
                })
                .collect()
        }
        _ => initial_sites(container, &targets, seed),
    };
    handle_overweighted(&mut sites);

    let mut cells = Vec::with_capacity(n);
    power_diagram(container, &sites, &mut cells);
    let mut error = area_error(&cells, &targets, total);
    let mut best = (score(error), sites.clone());

    // Flicker mitigation: damp steps whenever the error grows again.
    let mut damping: f64 = 0.0;
    let mut max_ratio: f64 = 2.0;
    let mut iterations = 0;
    while iterations < settings.max_iterations && (error.0 > settings.convergence || error.1 > settings.max_relative) {
        iterations += 1;

        // 1. Lloyd step: move sites toward their cell centroids.
        for (s, c) in sites.iter_mut().zip(&cells) {
            if c.pts.len() >= 3 {
                let to = c.centroid();
                s.pos = s.pos + (to - s.pos) * (1.0 - damping);
            }
        }
        handle_overweighted(&mut sites);
        power_diagram(container, &sites, &mut cells);

        // 2. Weight step: grow cells that are too small, shrink the others.
        for ((s, c), t) in sites.iter_mut().zip(&cells).zip(&targets) {
            let a = c.area();
            let ratio = if a > 1e-12 { t / a } else { max_ratio };
            let ratio = ratio.clamp(1.0 / max_ratio, max_ratio);
            s.weight = (s.weight * ratio).max(floor);
        }
        handle_overweighted(&mut sites);
        power_diagram(container, &sites, &mut cells);

        let e = area_error(&cells, &targets, total);
        if score(e) > score(error) {
            damping = (damping + 0.1).min(0.6);
            max_ratio = (max_ratio * 0.85).max(1.05);
        } else {
            damping *= 0.9;
        }
        error = e;
        if score(error) < best.0 {
            best = (score(error), sites.clone());
        }
    }

    if best.0 < score(error) {
        sites = best.1;
        power_diagram(container, &sites, &mut cells);
        error = area_error(&cells, &targets, total);
    }
    Solution { sites, cells, error: error.0, iterations }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: f64, h: f64) -> Polygon {
        Polygon::rect(Vec2::new(0.0, 0.0), Vec2::new(w, h))
    }

    #[test]
    fn cells_partition_the_container() {
        let c = rect(800.0, 500.0);
        let values: Vec<f64> = (1..=24).map(|i| (i * i) as f64).collect();
        let sol = solve(&c, &values, None, 7, &Settings::default());
        let sum: f64 = sol.cells.iter().map(|p| p.area()).sum();
        assert!((sum - c.area()).abs() / c.area() < 1e-6, "cells tile the container");
        assert!(sol.cells.iter().all(|p| p.pts.len() >= 3), "no empty cells");
        assert!(sol.cells.iter().all(|p| p.is_convex()));
    }

    #[test]
    fn converges_on_skewed_distributions() {
        let c = rect(1200.0, 800.0);
        let mut rng = SplitMix64::new(42);
        let values: Vec<f64> = (0..48).map(|_| (rng.next_f64() * 6.0).exp()).collect();
        let sol = solve(&c, &values, None, 1, &Settings { max_iterations: 200, convergence: 0.02, ..Settings::default() });
        assert!(sol.error < 0.05, "area error {} after {} its", sol.error, sol.iterations);
    }

    #[test]
    fn warm_start_converges_fast() {
        let c = rect(1000.0, 700.0);
        let values: Vec<f64> = (1..=30).map(|i| i as f64).collect();
        let s = Settings::default();
        let cold = solve(&c, &values, None, 3, &s);
        let mut v2 = values.clone();
        v2[0] *= 1.3;
        let warm = solve(&c, &v2, Some(&cold.sites), 3, &s);
        assert!(warm.iterations <= cold.iterations, "{} vs {}", warm.iterations, cold.iterations);
    }

    /// One huge value next to a tiny one: the tiny cell must stay tiny,
    /// both cold and from a degenerate warm start (all weights zero, as a
    /// single-cell level used to record).
    #[test]
    fn extreme_ratios_keep_small_cells_small() {
        let c = rect(1100.0, 1000.0);
        let values = [25.0e9, 12.0e6];
        let s = Settings { max_iterations: 120, convergence: 0.008, ..Settings::default() };
        let zero = [Site { pos: Vec2::new(409.0, 388.0), weight: 0.0 }, Site { pos: Vec2::new(591.0, 175.0), weight: 0.0 }];
        for warm in [None, Some(&zero[..])] {
            let sol = solve(&c, &values, warm, 7, &s);
            let small = sol.cells[1].area() / c.area();
            let want = values[1] / values.iter().sum::<f64>();
            assert!(small < want * 1.5, "small cell {small:.5} vs {want:.5} (warm: {})", warm.is_some());
        }
    }

    #[test]
    fn single_and_empty() {
        let c = rect(10.0, 10.0);
        assert!(solve(&c, &[], None, 0, &Settings::default()).cells.is_empty());
        assert_eq!(solve(&c, &[3.0], None, 0, &Settings::default()).cells[0].area(), 100.0);
    }

    #[test]
    fn works_in_circular_container() {
        let c = Polygon::circle(Vec2::new(0.0, 0.0), 200.0, 64);
        let values = [50.0, 20.0, 10.0, 8.0, 5.0, 4.0, 2.0, 1.0];
        let sol = solve(&c, &values, None, 9, &Settings { max_iterations: 150, convergence: 0.01, ..Settings::default() });
        assert!(sol.error < 0.03, "error {}", sol.error);
    }
}
