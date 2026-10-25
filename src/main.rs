#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod decode;
mod desktop;
mod diagnostics;
mod dsp;
mod library;
mod lyrics;
mod media;
mod settings;
mod spectrum;
mod theme;
mod widgets;
use cpal::traits::{DeviceTrait, HostTrait};
use eframe::egui;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        if !std::env::args_os()
            .skip(1)
            .any(|arg| arg.to_string_lossy().starts_with("--"))
        {
            rfd::MessageDialog::new()
                .set_title("EPlayer 無法啟動")
                .set_description(format!("{error:#}"))
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|x| x == "--order-test") {
        return diagnostics::order_test(
            args.get(2)
                .ok_or_else(|| anyhow::anyhow!("Missing 3-second fixture"))?
                .into(),
        );
    }
    if args.get(1).is_some_and(|x| x == "--queue-edit-test") {
        return diagnostics::queue_edit_test(
            args.iter().skip(2).map(std::path::PathBuf::from).collect(),
        );
    }
    if args.get(1).is_some_and(|x| x == "--seek-stress") {
        return diagnostics::seek_stress(
            args.get(2)
                .ok_or_else(|| anyhow::anyhow!("Missing fixture"))?
                .into(),
        );
    }
    if args.get(1).is_some_and(|x| x == "--soak") {
        let seconds = args
            .get(2)
            .ok_or_else(|| anyhow::anyhow!("Missing duration"))?
            .to_string_lossy()
            .parse()?;
        return diagnostics::soak(
            seconds,
            args.get(3)
                .ok_or_else(|| anyhow::anyhow!("Missing fixture directory"))?
                .into(),
            args.get(4)
                .ok_or_else(|| anyhow::anyhow!("Missing report path"))?
                .into(),
        );
    }
    if args.get(1).is_some_and(|x| x == "--render-probe") {
        return audio::render_probe(
            args.get(2)
                .ok_or_else(|| anyhow::anyhow!("Missing output file"))?
                .into(),
            args.iter().skip(3).map(std::path::PathBuf::from).collect(),
        );
    }
    if args.get(1).is_some_and(|x| x == "--spectrum-test") {
        return diagnostics::spectrum_test(
            args.get(2)
                .ok_or_else(|| anyhow::anyhow!("Missing fixture"))?
                .into(),
        );
    }
    if args.get(1).is_some_and(|x| x == "--library-test") {
        return library::smoke_test(
            args.get(2)
                .ok_or_else(|| anyhow::anyhow!("Missing fixture folder"))?
                .into(),
        );
    }
    if args.get(1).is_some_and(|x| x == "--control-test") {
        return diagnostics::control_test(
            args.get(2)
                .ok_or_else(|| anyhow::anyhow!("Missing 3-second fixture"))?
                .into(),
        );
    }
    if args.get(1).is_some_and(|x| x == "--play-smoke") {
        let engine = audio::AudioEngine::new()?;
        engine.set_volume(0.1);
        engine.send(audio::Command::Queue(
            args.iter().skip(2).map(std::path::PathBuf::from).collect(),
            0,
        ));
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < 20 {
            while let Ok(event) = engine.events.try_recv() {
                match event {
                    audio::Event::Loaded { token, index, info } => {
                        println!("Loaded {token}/{index}: {}", info.title)
                    }
                    audio::Event::Error(error) => eprintln!("{error}"),
                }
            }
            if engine
                .state
                .finished
                .load(std::sync::atomic::Ordering::Relaxed)
                && !engine
                    .state
                    .playing
                    .load(std::sync::atomic::Ordering::Relaxed)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        println!(
            "{}",
            serde_json::json!({"device":engine.device_name,"sample_rate":engine.sample_rate,"output_frames":engine.state.output_frames.load(std::sync::atomic::Ordering::Relaxed),"underruns":engine.state.underruns.load(std::sync::atomic::Ordering::Relaxed),"position":engine.position()})
        );
        return Ok(());
    }
    if args.get(1).is_some_and(|x| x == "--probe") {
        let mut failures = 0;
        for path in args.iter().skip(2) {
            match media::probe(std::path::Path::new(path)) {
                Ok(report) => println!("{report}"),
                Err(error) => {
                    failures += 1;
                    println!(
                        "{}",
                        serde_json::json!({"path":path.to_string_lossy(),"error":format!("{error:#}")})
                    );
                }
            }
        }
        anyhow::ensure!(failures == 0, "{failures} file probes failed");
        return Ok(());
    }
    if args.get(1).is_some_and(|x| x == "--devices") {
        for device in cpal::default_host().output_devices()? {
            println!("{}: {:?}", device.name()?, device.default_output_config());
        }
        return Ok(());
    }
    eframe::run_native(
        "EPlayer",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_decorations(false)
                .with_inner_size([960.0, 620.0])
                .with_min_inner_size([800.0, 560.0])
                .with_icon(theme::icon()),
            ..Default::default()
        },
        Box::new(move |cc| {
            Ok(Box::new(app::PlayerApp::new(
                cc,
                args.iter().any(|x| x == "--gui-smoke"),
                args.iter()
                    .skip(1)
                    .map(std::path::PathBuf::from)
                    .filter(|p| p.is_file() && library::is_audio(p))
                    .filter_map(|p| p.canonicalize().ok())
                    .collect(),
            )))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
