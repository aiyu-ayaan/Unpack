use serde::Serialize;
use std::io::Read;
use std::path::Path;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Archive {
    TarGz,
    TarXz,
    TarBz2,
    TarZst,
    Tar,
    Zip,
    SevenZ,
    Gz,
    Xz,
    Bz2,
    Zst,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Deb,
    Rpm,
    PacmanPkg,
    AlpineApk,
    AndroidApk,
    Snap,
    FlatpakBundle,
    FlatpakRef,
    FlatpakRepo,
    AppImage,
    RunInstaller,
    BinInstaller,
    Script,
    Archive(Archive),
    Dsc,
    Elf,
    Unknown,
}

pub fn sniff(path: &Path) -> std::io::Result<Kind> {
    let mut head = vec![0u8; 512];
    let n = std::fs::File::open(path)?.read(&mut head)?;
    head.truncate(n);
    let name = path.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    Ok(sniff_bytes(&name, &head))
}

fn ends(name: &str, sfx: &[&str]) -> bool {
    sfx.iter().any(|s| name.ends_with(s))
}

pub fn sniff_bytes(name: &str, head: &[u8]) -> Kind {
    let starts = |m: &[u8]| head.starts_with(m);
    let text = || String::from_utf8_lossy(head);

    if starts(b"\x7fELF") {
        if head.len() > 10 && head[8] == b'A' && head[9] == b'I' && (head[10] == 2 || head[10] == 1) {
            return Kind::AppImage;
        }
        if name.ends_with(".run") {
            return Kind::RunInstaller;
        }
        if name.ends_with(".bin") {
            return Kind::BinInstaller;
        }
        return Kind::Elf;
    }
    if starts(b"!<arch>\n") && head.len() > 8 && head[8..].starts_with(b"debian-binary") {
        return Kind::Deb;
    }
    if starts(&[0xED, 0xAB, 0xEE, 0xDB]) {
        return Kind::Rpm;
    }
    if starts(b"PK\x03\x04") {
        return if name.ends_with(".apk") { Kind::AndroidApk } else { Kind::Archive(Archive::Zip) };
    }
    if starts(b"hsqs") {
        return Kind::Snap;
    }
    if starts(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Kind::Archive(Archive::SevenZ);
    }
    // Arch packages are compressed tars; name decides because the magic is just the compressor's.
    let compressed = starts(&[0x28, 0xB5, 0x2F, 0xFD])
        || starts(&[0xFD, b'7', b'z', b'X', b'Z', 0])
        || starts(&[0x1F, 0x8B]);
    if compressed && name.contains(".pkg.tar") {
        return Kind::PacmanPkg;
    }
    if starts(&[0x1F, 0x8B]) {
        return if name.ends_with(".apk") {
            Kind::AlpineApk
        } else if ends(name, &[".tar.gz", ".tgz"]) {
            Kind::Archive(Archive::TarGz)
        } else {
            Kind::Archive(Archive::Gz)
        };
    }
    if starts(&[0xFD, b'7', b'z', b'X', b'Z', 0]) {
        return if ends(name, &[".tar.xz", ".txz"]) {
            Kind::Archive(Archive::TarXz)
        } else {
            Kind::Archive(Archive::Xz)
        };
    }
    if starts(b"BZh") {
        return if ends(name, &[".tar.bz2", ".tbz2", ".tbz"]) {
            Kind::Archive(Archive::TarBz2)
        } else {
            Kind::Archive(Archive::Bz2)
        };
    }
    if starts(&[0x28, 0xB5, 0x2F, 0xFD]) {
        return if ends(name, &[".tar.zst", ".tzst"]) {
            Kind::Archive(Archive::TarZst)
        } else {
            Kind::Archive(Archive::Zst)
        };
    }
    if head.len() > 262 && &head[257..262] == b"ustar" {
        return if name.contains(".pkg.tar") { Kind::PacmanPkg } else { Kind::Archive(Archive::Tar) };
    }
    let t = text();
    if t.starts_with("[Flatpak Ref]") {
        return Kind::FlatpakRef;
    }
    if t.starts_with("[Flatpak Repo]") {
        return Kind::FlatpakRepo;
    }
    if name.ends_with(".dsc") {
        return Kind::Dsc;
    }
    if name.ends_with(".flatpak") {
        return Kind::FlatpakBundle;
    }
    if t.starts_with("#!") || name.ends_with(".sh") {
        return if name.ends_with(".run") {
            Kind::RunInstaller
        } else if name.ends_with(".bin") {
            Kind::BinInstaller
        } else {
            Kind::Script
        };
    }
    Kind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elf_appimage() -> Vec<u8> {
        let mut v = vec![0u8; 64];
        v[..4].copy_from_slice(b"\x7fELF");
        v[8..11].copy_from_slice(b"AI\x02");
        v
    }

    #[test]
    fn packages() {
        assert_eq!(sniff_bytes("a.deb", b"!<arch>\ndebian-binary   2.0"), Kind::Deb);
        assert_eq!(sniff_bytes("a.rpm", &[0xED, 0xAB, 0xEE, 0xDB, 0, 0]), Kind::Rpm);
        assert_eq!(sniff_bytes("a.snap", b"hsqs...."), Kind::Snap);
        assert_eq!(sniff_bytes("a.appimage", &elf_appimage()), Kind::AppImage);
        assert_eq!(sniff_bytes("a.pkg.tar.zst", &[0x28, 0xB5, 0x2F, 0xFD, 0]), Kind::PacmanPkg);
        assert_eq!(sniff_bytes("a.pkg.tar.xz", &[0xFD, b'7', b'z', b'X', b'Z', 0]), Kind::PacmanPkg);
    }

    #[test]
    fn apk_clash() {
        assert_eq!(sniff_bytes("app.apk", b"PK\x03\x04rest"), Kind::AndroidApk);
        assert_eq!(sniff_bytes("app.apk", &[0x1F, 0x8B, 8]), Kind::AlpineApk);
    }

    #[test]
    fn archives() {
        let gz = [0x1F, 0x8B, 8];
        assert_eq!(sniff_bytes("a.tar.gz", &gz), Kind::Archive(Archive::TarGz));
        assert_eq!(sniff_bytes("a.tgz", &gz), Kind::Archive(Archive::TarGz));
        assert_eq!(sniff_bytes("a.gz", &gz), Kind::Archive(Archive::Gz));
        assert_eq!(sniff_bytes("a.tar.bz2", b"BZh9"), Kind::Archive(Archive::TarBz2));
        assert_eq!(sniff_bytes("a.zip", b"PK\x03\x04"), Kind::Archive(Archive::Zip));
        assert_eq!(sniff_bytes("a.7z", &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]), Kind::Archive(Archive::SevenZ));
        assert_eq!(sniff_bytes("a.tar.zst", &[0x28, 0xB5, 0x2F, 0xFD]), Kind::Archive(Archive::TarZst));
        let mut tar = vec![0u8; 512];
        tar[257..262].copy_from_slice(b"ustar");
        assert_eq!(sniff_bytes("a.tar", &tar), Kind::Archive(Archive::Tar));
    }

    #[test]
    fn text_kinds() {
        assert_eq!(sniff_bytes("x.flatpakref", b"[Flatpak Ref]\nName=a"), Kind::FlatpakRef);
        assert_eq!(sniff_bytes("x.flatpakrepo", b"[Flatpak Repo]\n"), Kind::FlatpakRepo);
        assert_eq!(sniff_bytes("x.flatpak", b"\0\0\0"), Kind::FlatpakBundle);
        assert_eq!(sniff_bytes("x.dsc", b"Format: 3.0 (quilt)"), Kind::Dsc);
        assert_eq!(sniff_bytes("i.sh", b"#!/bin/sh\n"), Kind::Script);
        assert_eq!(sniff_bytes("i.run", b"#!/bin/sh\n# Makeself"), Kind::RunInstaller);
        assert_eq!(sniff_bytes("i.bin", b"#!/bin/sh\n"), Kind::BinInstaller);
        assert_eq!(sniff_bytes("i", b"#!/bin/bash\n"), Kind::Script);
        assert_eq!(sniff_bytes("x.txt", b"hello"), Kind::Unknown);
    }

    #[test]
    fn elf_variants() {
        let mut e = vec![0u8; 16];
        e[..4].copy_from_slice(b"\x7fELF");
        assert_eq!(sniff_bytes("setup.run", &e), Kind::RunInstaller);
        assert_eq!(sniff_bytes("setup.bin", &e), Kind::BinInstaller);
        assert_eq!(sniff_bytes("tool", &e), Kind::Elf);
    }
}
