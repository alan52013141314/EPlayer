use std::{env, fs, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=icon.png");
    println!("cargo:rerun-if-env-changed=RC");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let source = image::open("icon.png").expect("read icon.png");
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let mut frames = Vec::new();
    for size in sizes {
        let img = source.resize_exact(size, size, image::imageops::FilterType::Lanczos3);
        let mut bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        frames.push(bytes.into_inner());
    }
    let mut ico = vec![0, 0, 1, 0, sizes.len() as u8, 0];
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (size, frame) in sizes.into_iter().zip(&frames) {
        ico.extend_from_slice(&[size as u8, size as u8, 0, 0, 1, 0, 32, 0]);
        ico.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += frame.len() as u32;
    }
    for frame in frames {
        ico.extend(frame);
    }
    let icon = out.join("eplayer.ico");
    fs::write(&icon, ico).unwrap();
    let rcfile = out.join("eplayer.rc");
    fs::write(
        &rcfile,
        format!(
            "1 ICON \"{}\"\n",
            icon.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    let rc = env::var_os("RC").map(PathBuf::from).unwrap_or_else(|| {
        let base =
            PathBuf::from(env::var_os("ProgramFiles(x86)").unwrap()).join("Windows Kits/10/bin");
        let mut versions: Vec<_> = fs::read_dir(base)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path().join("x64/rc.exe"))
            .filter(|p| p.is_file())
            .collect();
        versions.sort();
        versions
            .pop()
            .expect("Windows SDK rc.exe required; set RC to its path")
    });
    let res = out.join("eplayer.res");
    assert!(
        Command::new(rc)
            .arg("/nologo")
            .arg("/fo")
            .arg(&res)
            .arg(rcfile)
            .status()
            .unwrap()
            .success(),
        "resource compilation failed"
    );
    println!("cargo:rustc-link-arg-bin=eplayer={}", res.display());
}
