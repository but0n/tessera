//! Tessera — see where your disk went, and take it back.
//!
//! Composition root: the only place that knows every layer. It picks the
//! infrastructure adapters, builds the application services and hands them
//! to the presentation layer.

mod app;
mod gfx;
mod snapshot;
mod state;
mod theme;
mod ui;
mod views;

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use tessera_app::cartography::{Cartographer, MapParams};
use tessera_app::scan::Waker;
use tessera_app::{Services, Workspace};
use tessera_domain::ScanPolicy;
use tessera_infra::{SampleReader, SampleTrash, SampleVolume, StatVfs, SystemTrash, best_reader, default_policy, home_dir, sample_root};
use winit::event_loop::EventLoop;

use crate::state::Tab;

const USAGE: &str = "usage: tessera [PATH | --sample] [--snapshot OUT.png [--view overview|map] [--size WxH] [--scale S] [--focus NAME] [--hover X,Y] [--select REL/PATH] [--click X,Y | --scroll X,Y,DY [--after MS]] [--demo]]";

fn main() {
    let mut root: Option<PathBuf> = None;
    let mut sample = false;
    let mut snap: Option<snapshot::Options> = None;
    let mut args = std::env::args().skip(1);
    let mut opts = snapshot::Options {
        out: PathBuf::new(),
        tab: Tab::Overview,
        size: (2880, 1800),
        scale: 2.0,
        demo: false,
        focus: None,
        hover: None,
        click: None,
        after_ms: 0,
        select: None,
        tree_scroll: 0.0,
        scroll: None,
    };
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            "--snapshot" => opts.out = args.next().expect(USAGE).into(),
            "--view" => opts.tab = if args.next().as_deref() == Some("map") { Tab::Map } else { Tab::Overview },
            "--size" => {
                let s = args.next().expect(USAGE);
                let (w, h) = s.split_once('x').expect(USAGE);
                opts.size = (w.parse().expect(USAGE), h.parse().expect(USAGE));
            }
            "--scale" => opts.scale = args.next().expect(USAGE).parse().expect(USAGE),
            "--focus" => opts.focus = args.next(),
            "--hover" => {
                let s = args.next().expect(USAGE);
                let (x, y) = s.split_once(',').expect(USAGE);
                opts.hover = Some((x.parse().expect(USAGE), y.parse().expect(USAGE)));
            }
            "--click" => {
                let s = args.next().expect(USAGE);
                let (x, y) = s.split_once(',').expect(USAGE);
                opts.click = Some((x.parse().expect(USAGE), y.parse().expect(USAGE)));
            }
            "--after" => opts.after_ms = args.next().expect(USAGE).parse().expect(USAGE),
            "--demo" => opts.demo = true,
            "--sample" => sample = true,
            "--select" => opts.select = args.next(),
            "--scroll" => {
                let s = args.next().expect(USAGE);
                let v: Vec<f32> = s.split(',').map(|x| x.parse().expect(USAGE)).collect();
                opts.scroll = Some((v[0], v[1], v[2]));
            }
            "--tree-scroll" => opts.tree_scroll = args.next().expect(USAGE).parse().expect(USAGE),
            p => root = Some(PathBuf::from(p)),
        }
    }
    if !opts.out.as_os_str().is_empty() {
        snap = Some(opts);
    }
    // `--sample` serves a fictional folder through the same ports, for
    // demos and screenshots that must not show anyone's real files.
    let (root, services, policy) = if sample {
        let services = Services { reader: Arc::new(SampleReader), trash: Arc::new(SampleTrash), volume: Arc::new(SampleVolume) };
        (sample_root(), services, ScanPolicy { one_filesystem: true, protected: Arc::default() })
    } else {
        let root = root.or_else(home_dir).unwrap_or_else(|| PathBuf::from("/"));
        let root = std::fs::canonicalize(&root).unwrap_or(root);
        (root, Services { reader: best_reader(), trash: Arc::new(SystemTrash::default()), volume: Arc::new(StatVfs) }, default_policy())
    };

    if let Some(opts) = snap {
        let waker: Waker = Arc::new(|| {});
        let ws = Workspace::open(root, services, policy, waker.clone());
        let carto = Cartographer::new(MapParams::default(), waker);
        snapshot::run(ws, carto, opts);
        return;
    }

    let event_loop = EventLoop::<app::Wake>::with_user_event().build().expect("event loop");
    let proxy = Mutex::new(event_loop.create_proxy());
    let waker: Waker = Arc::new(move || {
        let _ = proxy.lock().send_event(app::Wake);
    });
    let ws = Workspace::open(root, services, policy, waker.clone());
    let carto = Cartographer::new(MapParams::default(), waker);
    let mut app = app::App::new(ws, carto);
    event_loop.run_app(&mut app).expect("run");
}
