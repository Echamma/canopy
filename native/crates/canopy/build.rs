//! Gives canopy.exe its icon in Explorer, Start, search and the taskbar (the window icon is set at runtime in main.rs).
//!
//! There is no C compiler or rc.exe on the build machine, so this writes the Windows resource file (.res) itself from
//! assets/canopy.ico and hands it to rust-lld, which links .res files directly.

use std::{env, fs, path::PathBuf};

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const LANG_EN_US: u16 = 0x0409;

fn main() {
    let ico = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../../assets/canopy.ico");
    println!("cargo:rerun-if-changed={}", ico.display());
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let bytes = fs::read(&ico).unwrap_or_else(|e| panic!("reading {}: {e}", ico.display()));
    let res = icon_res(&bytes);
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("canopy-icon.res");
    fs::write(&out, res).unwrap();
    // Only the Canopy shell gets this icon; Moodie, File Manager and Video Chat keep their own.
    println!("cargo:rustc-link-arg-bin=canopy={}", out.display());
}

/// Builds a .res file holding every image of an .ico as RT_ICON 1..n and one RT_GROUP_ICON (id 1) listing them.
fn icon_res(ico: &[u8]) -> Vec<u8> {
    let u16_at = |i: usize| u16::from_le_bytes([ico[i], ico[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([ico[i], ico[i + 1], ico[i + 2], ico[i + 3]]);
    assert!(ico.len() >= 6 && u16_at(0) == 0 && u16_at(2) == 1, "not an .ico file");
    let count = u16_at(4) as usize;

    // The empty entry every .res file starts with.
    let mut res = vec![0, 0, 0, 0, 0x20, 0, 0, 0, 0xff, 0xff, 0, 0, 0xff, 0xff, 0, 0];
    res.extend_from_slice(&[0; 16]);

    let mut group = Vec::new();
    group.extend_from_slice(&0u16.to_le_bytes());
    group.extend_from_slice(&1u16.to_le_bytes());
    group.extend_from_slice(&(count as u16).to_le_bytes());
    for i in 0..count {
        let entry = 6 + 16 * i;
        let size = u32_at(entry + 8) as usize;
        let offset = u32_at(entry + 12) as usize;
        let id = (i + 1) as u16;
        push_resource(&mut res, RT_ICON, id, 0x1010, &ico[offset..offset + size]);
        // GRPICONDIRENTRY: the ICONDIRENTRY's first 12 bytes, then the RT_ICON id instead of the file offset.
        group.extend_from_slice(&ico[entry..entry + 12]);
        group.extend_from_slice(&id.to_le_bytes());
    }
    push_resource(&mut res, RT_GROUP_ICON, 1, 0x1030, &group);
    res
}

fn push_resource(res: &mut Vec<u8>, kind: u16, id: u16, flags: u16, data: &[u8]) {
    let mut header = Vec::new();
    header.extend_from_slice(&(data.len() as u32).to_le_bytes());
    header.extend_from_slice(&32u32.to_le_bytes()); // header size: always 32 with an ordinal type and name
    header.extend_from_slice(&[0xff, 0xff]);
    header.extend_from_slice(&kind.to_le_bytes());
    header.extend_from_slice(&[0xff, 0xff]);
    header.extend_from_slice(&id.to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes()); // data version
    header.extend_from_slice(&flags.to_le_bytes());
    header.extend_from_slice(&LANG_EN_US.to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes()); // version
    header.extend_from_slice(&0u32.to_le_bytes()); // characteristics
    debug_assert_eq!(header.len(), 32);
    res.extend_from_slice(&header);
    res.extend_from_slice(data);
    while res.len() % 4 != 0 {
        res.push(0);
    }
}
