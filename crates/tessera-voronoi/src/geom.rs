use std::ops::{Add, Mul, Sub};

use crate::rng::SplitMix64;

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Vec2 { x, y }
    }
    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }
    pub fn cross(self, o: Vec2) -> f64 {
        self.x * o.y - self.y * o.x
    }
    pub fn len2(self) -> f64 {
        self.dot(self)
    }
    pub fn len(self) -> f64 {
        self.len2().sqrt()
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f64) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }
}

/// Convex polygon with positive signed area (shoelace formula), i.e. the
/// interior lies to the left of every edge in a y-up frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polygon {
    pub pts: Vec<Vec2>,
}

/// Sutherland–Hodgman against one half-plane `n·p <= c`.
pub(crate) fn clip_into(src: &[Vec2], n: Vec2, c: f64, out: &mut Vec<Vec2>) {
    out.clear();
    let m = src.len();
    if m == 0 {
        return;
    }
    let mut p = src[m - 1];
    let mut dp = n.dot(p) - c;
    for &q in src {
        let dq = n.dot(q) - c;
        if dq <= 0.0 {
            if dp > 0.0 {
                out.push(p + (q - p) * (dp / (dp - dq)));
            }
            out.push(q);
        } else if dp <= 0.0 {
            out.push(p + (q - p) * (dp / (dp - dq)));
        }
        p = q;
        dp = dq;
    }
    // Drop near-duplicate vertices produced by grazing cuts.
    if out.len() > 1 {
        let mut k = 0;
        for i in 0..out.len() {
            if k == 0 || (out[i] - out[k - 1]).len2() > 1e-12 {
                out[k] = out[i];
                k += 1;
            }
        }
        if k > 1 && (out[0] - out[k - 1]).len2() <= 1e-12 {
            k -= 1;
        }
        out.truncate(k);
    }
    if out.len() < 3 {
        out.clear();
    }
}

impl Polygon {
    pub fn rect(min: Vec2, max: Vec2) -> Polygon {
        Polygon { pts: vec![min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)] }
    }

    pub fn circle(c: Vec2, r: f64, segments: usize) -> Polygon {
        let pts = (0..segments)
            .map(|i| {
                let t = i as f64 / segments as f64 * std::f64::consts::TAU;
                Vec2::new(c.x + r * t.cos(), c.y + r * t.sin())
            })
            .collect();
        Polygon { pts }
    }

    pub fn is_empty(&self) -> bool {
        self.pts.len() < 3
    }

    pub fn signed_area(&self) -> f64 {
        let n = self.pts.len();
        if n < 3 {
            return 0.0;
        }
        let mut a = 0.0;
        for i in 0..n {
            a += self.pts[i].cross(self.pts[(i + 1) % n]);
        }
        a * 0.5
    }

    pub fn area(&self) -> f64 {
        self.signed_area().abs()
    }

    pub fn centroid(&self) -> Vec2 {
        let n = self.pts.len();
        if n == 0 {
            return Vec2::default();
        }
        let (mut cx, mut cy, mut a) = (0.0, 0.0, 0.0);
        let o = self.pts[0];
        for i in 0..n {
            let p = self.pts[i] - o;
            let q = self.pts[(i + 1) % n] - o;
            let k = p.cross(q);
            a += k;
            cx += (p.x + q.x) * k;
            cy += (p.y + q.y) * k;
        }
        if a.abs() < 1e-12 {
            let s = self.pts.iter().fold(Vec2::default(), |s, &p| s + p);
            return s * (1.0 / n as f64);
        }
        Vec2::new(o.x + cx / (3.0 * a), o.y + cy / (3.0 * a))
    }

    pub fn bbox(&self) -> (Vec2, Vec2) {
        let mut min = Vec2::new(f64::INFINITY, f64::INFINITY);
        let mut max = Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in &self.pts {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
        (min, max)
    }

    fn orientation(&self) -> f64 {
        if self.signed_area() >= 0.0 { 1.0 } else { -1.0 }
    }

    pub fn contains(&self, p: Vec2) -> bool {
        let n = self.pts.len();
        if n < 3 {
            return false;
        }
        let o = self.orientation();
        (0..n).all(|i| {
            let a = self.pts[i];
            let b = self.pts[(i + 1) % n];
            (b - a).cross(p - a) * o >= -1e-9
        })
    }

    pub fn is_convex(&self) -> bool {
        let n = self.pts.len();
        if n < 3 {
            return false;
        }
        let o = self.orientation();
        (0..n).all(|i| {
            let a = self.pts[i];
            let b = self.pts[(i + 1) % n];
            let c = self.pts[(i + 2) % n];
            (b - a).cross(c - b) * o >= -1e-6
        })
    }

    /// Keeps the part where `n·p <= c`.
    pub fn clip(&self, n: Vec2, c: f64) -> Polygon {
        let mut out = Vec::with_capacity(self.pts.len() + 1);
        clip_into(&self.pts, n, c, &mut out);
        Polygon { pts: out }
    }

    /// Erodes the polygon by distance `d` (all edges move inward). Exact for
    /// convex polygons; returns an empty polygon if it collapses.
    pub fn inset(&self, d: f64) -> Polygon {
        let n = self.pts.len();
        if n < 3 {
            return Polygon::default();
        }
        if d <= 0.0 {
            return self.clone();
        }
        let o = self.orientation();
        let mut cur = self.pts.clone();
        let mut tmp = Vec::with_capacity(n + 1);
        for i in 0..n {
            let a = self.pts[i];
            let b = self.pts[(i + 1) % n];
            let e = b - a;
            let len = e.len();
            if len < 1e-12 {
                continue;
            }
            // Outward normal for the polygon's orientation.
            let out_n = Vec2::new(e.y, -e.x) * (o / len);
            clip_into(&cur, out_n, out_n.dot(a) - d, &mut tmp);
            std::mem::swap(&mut cur, &mut tmp);
            if cur.is_empty() {
                break;
            }
        }
        Polygon { pts: cur }
    }

    /// Radius of the largest circle centred at the centroid that fits inside.
    pub fn inradius_at_centroid(&self) -> f64 {
        let c = self.centroid();
        let n = self.pts.len();
        let mut r = f64::INFINITY;
        for i in 0..n {
            let a = self.pts[i];
            let b = self.pts[(i + 1) % n];
            let e = b - a;
            let len = e.len();
            if len > 1e-12 {
                r = r.min((e.cross(c - a) / len).abs());
            }
        }
        if r.is_finite() { r } else { 0.0 }
    }

    pub fn random_point(&self, rng: &mut SplitMix64) -> Option<Vec2> {
        if self.is_empty() {
            return None;
        }
        let (min, max) = self.bbox();
        for _ in 0..64 {
            let p = Vec2::new(min.x + rng.next_f64() * (max.x - min.x), min.y + rng.next_f64() * (max.y - min.y));
            if self.contains(p) {
                return Some(p);
            }
        }
        Some(self.centroid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inset_square() {
        let p = Polygon::rect(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        let q = p.inset(2.0);
        assert!((q.area() - 36.0).abs() < 1e-9);
        assert!(p.inset(6.0).is_empty());
        // Orientation independent.
        let mut r = p.clone();
        r.pts.reverse();
        assert!((r.inset(2.0).area() - 36.0).abs() < 1e-9);
    }

    #[test]
    fn centroid_and_clip() {
        let p = Polygon::rect(Vec2::new(0.0, 0.0), Vec2::new(4.0, 2.0));
        let c = p.centroid();
        assert!((c.x - 2.0).abs() < 1e-12 && (c.y - 1.0).abs() < 1e-12);
        let half = p.clip(Vec2::new(1.0, 0.0), 1.0);
        assert!((half.area() - 2.0).abs() < 1e-12);
        assert!(p.contains(Vec2::new(1.0, 1.0)));
        assert!(!p.contains(Vec2::new(5.0, 1.0)));
        assert!((p.inradius_at_centroid() - 1.0).abs() < 1e-12);
    }
}
