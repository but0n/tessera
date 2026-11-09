//! `cargo run -p tessera-voronoi --release --example bench`
use std::time::Instant;
use tessera_voronoi::*;

fn main() {
    let c = Polygon::rect(Vec2::new(0.0, 0.0), Vec2::new(1200.0, 800.0));
    for &n in &[8usize, 24, 48, 64, 96] {
        let mut rng = SplitMix64::new(n as u64);
        // Heavy-tailed like real directory sizes.
        let values: Vec<f64> = (0..n).map(|_| (rng.next_f64() * 9.0).exp()).collect();
        let t = Instant::now();
        let sol = solve(&c, &values, None, 1, &Settings::default());
        println!("n={n:3} err={:.4} its={:3} {:7.2?}", sol.error, sol.iterations, t.elapsed());
    }
}
