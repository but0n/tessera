//! Headless scan benchmark:
//! `cargo run -p tessera-infra --release --example scan -- <path> [portable]`
use std::sync::Arc;
use std::time::{Duration, Instant};

use tessera_app::cartography::{MapParams, MapRequest, compute_now};
use tessera_app::ports::DirectoryReader;
use tessera_app::scan::ScanSession;
use tessera_domain::{ByteSize, NodeId};
use tessera_infra::{PortableReader, best_reader, default_policy};

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args.next().unwrap_or_else(|| std::env::var("HOME").unwrap());
    let reader: Arc<dyn DirectoryReader> = match args.next().as_deref() {
        Some("portable") => Arc::new(PortableReader),
        _ => best_reader(),
    };
    let t = Instant::now();
    let s = ScanSession::start(root.into(), reader, default_policy(), Arc::new(|| {}));
    while !s.is_finished() {
        std::thread::sleep(Duration::from_millis(5));
    }
    let el = t.elapsed();
    let inv = s.inventory().read();
    let n = inv.node(NodeId::ROOT);
    println!(
        "{:>16}: {} files, {} dirs, {} in {:.2?} ({:.0} entries/s), {} unreadable, {} MB arena",
        s.reader_name(),
        n.files(),
        n.dirs(),
        ByteSize(inv.total_size()),
        el,
        inv.len() as f64 / el.as_secs_f64(),
        inv.failures(),
        inv.len() * std::mem::size_of::<tessera_domain::Node>() / 1_000_000
    );
    drop(inv);
    let t = Instant::now();
    let l = compute_now(&MapRequest { inventory: s.inventory().clone(), focus: NodeId::ROOT, width: 1100.0, height: 760.0 }, &MapParams::default());
    println!("map: {} cells in {:.2?}", l.cells.len(), t.elapsed());
}
