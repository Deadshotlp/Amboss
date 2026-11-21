//! Amboss – ein Map-Editor für Source 1 / Garry's Mod.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod assets;
mod audio;
mod bsppack;
mod collab;
mod collab_ui;
mod class_picker;
mod p2p;
mod update;
mod update_ui;
mod palette;
mod particle_browser;
mod particle_editor;
mod particle_preview;
mod patch;
mod pcf;
mod physics;
mod place_ui;
mod chrome;
mod commands;
mod compile;
mod compile_help;
mod csg;
mod decal_gen;
mod decompile;
mod dialogs;
mod dialogs2;
mod diff;
mod disp;
mod dmx;
mod door_preview;
mod editor;
mod editor2;
mod fgd;
mod gamefs;
mod gamemode;
mod gltf_export;
mod gameplay;
mod heightmap;
mod i18n;
mod importexport;
mod geometry;
mod gi;
mod icons;
mod keyvalues;
mod lua;
mod lua_ents;
mod map;
mod material_editor;
mod materials;
mod mcp;
mod merge;
mod mesh;
mod mesh_edit;
mod mesh_ui;
mod live;
mod live_ui;
mod walk;
mod workshop;
mod whatsnew;
mod ropes;
mod merge_ui;
mod mdl;
mod mdl_anim;
mod texlib;
mod model_browser;
mod navmesh;
mod notes;
mod netcrypt;
mod models;
mod panels;
mod opt_ui;
mod pack_ui;
mod optimize;
mod overlay_mesh;
mod pointfile;
mod prefab_ui;
mod prefabs;
mod setup;
mod model_import;
mod model_import_ui;
mod skybox;
mod spline;
mod sky_ui;
mod primitives;
mod procgen;
mod procgen2;
mod procgen_ui;
mod scatter;
mod screenshot;
mod propper;
mod problems;
mod tabs;
mod tex_tools;
mod templates;
mod theme;
mod tiles;
mod timeline;
mod transform;
mod tutorial;
mod view2d;
mod view3d;
mod vtf;
mod vtf_write;

/// Kommandozeile für Git & Co.:
/// `amboss --diff ALT NEU` und `amboss --merge3 BASIS UNSERE ANDERE [AUSGABE]` (Git-Merge-Treiber: %O %A %B)
fn diff_cli(args: &[String]) -> Option<i32> {
    let pos = |flag: &str| args.iter().position(|a| a == flag);
    let load = |p: &str| merge_ui::load_any(std::path::Path::new(p), 0.25);
    if let Some(i) = pos("--diff") {
        let (Some(a), Some(b)) = (args.get(i + 1), args.get(i + 2)) else {
            eprintln!("Aufruf: amboss --diff ALT.vmf NEU.vmf");
            return Some(2);
        };
        let (old, new) = match (load(a), load(b)) {
            (Ok(x), Ok(y)) => (x, y),
            (Err(e), _) | (_, Err(e)) => {
                eprintln!("amboss: {e}");
                return Some(2);
            }
        };
        let ed = editor::Editor::new(fgd::Fgd::builtin());
        let d = diff::diff(&old, &new, &ed);
        println!("{}", d.summary());
        for (k, x, y) in &d.world {
            println!("  Map-Eigenschaft {k}: {} -> {}", x.as_deref().unwrap_or("-"), y.as_deref().unwrap_or("-"));
        }
        for o in &d.objects {
            println!("  {:<9} {}", o.change.label(), o.label);
        }
        return Some(if d.objects.is_empty() && d.world.is_empty() { 0 } else { 1 });
    }
    if let Some(i) = pos("--merge3") {
        let (Some(b), Some(o), Some(t)) = (args.get(i + 1), args.get(i + 2), args.get(i + 3)) else {
            eprintln!("Aufruf: amboss --merge3 BASIS.vmf UNSERE.vmf ANDERE.vmf [AUSGABE.vmf]");
            return Some(2);
        };
        let out = args.get(i + 4).filter(|s| !s.starts_with("--")).unwrap_or(o);
        let (base, mut ours, theirs) = match (load(b), load(o), load(t)) {
            (Ok(x), Ok(y), Ok(z)) => (x, y, z),
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                eprintln!("amboss: {e}");
                return Some(2);
            }
        };
        let r = diff::merge3(&base, &mut ours, &theirs);
        if let Err(e) = std::fs::write(out, ours.to_vmf()) {
            eprintln!("amboss: {out}: {e}");
            return Some(2);
        }
        eprintln!("amboss: {}", r.summary());
        for c in &r.conflicts {
            eprintln!("  KONFLIKT: {}", c.text);
        }
        return Some(if r.conflicts.is_empty() { 0 } else { 1 });
    }
    None
}

/// Kantenglättung (MSAA) aus einer kleinen Datei neben den Einstellungen – wird vor dem Fensterstart gebraucht.
/// Standard aus, damit Amboss auch auf Grafikkarten ohne MSAA startet.
pub fn gfx_file() -> Option<std::path::PathBuf> {
    eframe::storage_dir("Amboss").map(|d| d.join("grafik.txt"))
}

pub fn gfx_msaa_pub() -> u16 {
    gfx_msaa()
}

fn gfx_msaa() -> u16 {
    gfx_file().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| s.lines().find_map(|l| l.strip_prefix("msaa=").and_then(|v| v.trim().parse().ok()))).unwrap_or(0)
}

/// MSAA-Stufe speichern (gilt ab dem nächsten Start)
pub fn set_gfx_msaa(v: u16) -> std::io::Result<()> {
    let p = gfx_file().ok_or_else(|| std::io::Error::other("kein Einstellungsordner"))?;
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(p, format!("msaa={v}\n"))
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = diff_cli(&args) {
        std::process::exit(code);
    }
    if args.iter().any(|a| a == "--mcp-stdio") {
        mcp::run_stdio(&args);
        return Ok(());
    }
    let msaa = gfx_msaa();
    let options = eframe::NativeOptions {
        multisampling: msaa,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1600.0, 950.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title(concat!("Amboss ", env!("CARGO_PKG_VERSION")))
            .with_drag_and_drop(true)
            .with_icon(app_icon()),
        depth_buffer: 24,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native("Amboss", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}

/// Programm-Icon (Amboss auf orangem Grund) für Fenster und Taskleiste
fn app_icon() -> eframe::egui::IconData {
    let dec = png::Decoder::new(std::io::Cursor::new(&include_bytes!("../assets/icon/amboss.png")[..]));
    let mut r = dec.read_info().expect("Icon");
    let mut buf = vec![0; r.output_buffer_size().unwrap_or(256 * 256 * 4)];
    let info = r.next_frame(&mut buf).expect("Icon");
    buf.truncate(info.buffer_size());
    eframe::egui::IconData { rgba: buf, width: info.width, height: info.height }
}
