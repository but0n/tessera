use std::fmt;

/// Byte quantity value object. Formats with SI (base-10) units, matching
/// what Finder reports, so numbers agree with the rest of the OS.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteSize(pub u64);

const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];

impl ByteSize {
    /// Splits into a number and its unit so typography can style them apart.
    pub fn parts(self) -> (String, &'static str) {
        let b = self.0;
        if b < 1000 {
            return (b.to_string(), UNITS[0]);
        }
        let mut v = b as f64;
        let mut u = 0;
        while v >= 999.95 && u < UNITS.len() - 1 {
            v /= 1000.0;
            u += 1;
        }
        let digits = if v >= 100.0 {
            0
        } else if v >= 10.0 {
            1
        } else {
            2
        };
        (format!("{v:.digits$}"), UNITS[u])
    }
}

impl fmt::Display for ByteSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (n, u) = self.parts();
        write!(f, "{n} {u}")
    }
}

/// Compact integer formatting with thin grouping: `1 234 567`.
pub fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_si_units() {
        assert_eq!(ByteSize(0).to_string(), "0 B");
        assert_eq!(ByteSize(999).to_string(), "999 B");
        assert_eq!(ByteSize(1_000).to_string(), "1.00 KB");
        assert_eq!(ByteSize(12_340_000).to_string(), "12.3 MB");
        assert_eq!(ByteSize(512_000_000_000).to_string(), "512 GB");
        assert_eq!(ByteSize(999_960).to_string(), "1.00 MB");
    }

    #[test]
    fn groups_digits() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(1234567), "1,234,567");
        assert_eq!(group_digits(100), "100");
    }
}
