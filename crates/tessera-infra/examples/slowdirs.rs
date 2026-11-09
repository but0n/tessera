//! Lists the slowest directory reads under a root (diagnostics).
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tessera_app::ports::DirectoryReader;
use tessera_app::scan::ScanSession;
use tessera_domain::Listing;
use tessera_infra::{best_reader, default_policy};

struct Timed(Arc<dyn DirectoryReader>, Arc<Mutex<Vec<(Duration, PathBuf, bool)>>>);
impl DirectoryReader for Timed {
    fn name(&self) -> &'static str {
        "timed"
    }
    fn read(&self, p: &Path, out: &mut Listing) -> std::io::Result<()> {
        let t = Instant::now();
        let r = self.0.read(p, out);
        let el = t.elapsed();
        if el > Duration::from_millis(20) {
            self.1.lock().push((el, p.to_path_buf(), r.is_ok()));
        }
        r
    }
}
fn main() {
    let root = std::env::args().nth(1).unwrap();
    let log = Arc::new(Mutex::new(vec![]));
    let s = ScanSession::start(root.into(), Arc::new(Timed(best_reader(), log.clone())), default_policy(), Arc::new(|| {}));
    while !s.is_finished() {
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut v = log.lock().clone();
    v.sort_by_key(|e| std::cmp::Reverse(e.0));
    println!("total {:.2?}; {} slow reads", s.stats().elapsed(), v.len());
    for (d, p, ok) in v.iter().take(15) {
        println!("{d:>10.2?} {} {}", if *ok { "ok " } else { "ERR" }, p.display());
    }
}
