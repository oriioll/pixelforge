#!/usr/bin/env python3
"""Download the third-party binaries and models pixelforge bundles (never committed to git).

Everything is pinned by SHA-256, so a changed upstream file fails loudly instead of silently
shipping something different. Already-downloaded files with the right hash are reused.

Layout produced under --target (default: src-tauri):
    binaries/realesrgan-ncnn-vulkan-x86_64-pc-windows-msvc.exe   (Tauri externalBin sidecar)
    resources/vcomp140.dll                                         (OpenMP runtime of the sidecar)
    resources/models/realesrgan-x4plus.{param,bin}
    resources/models/realesr-general-x4v3.{param,bin}              (converted from the official .pth)
    resources/models/realesr-general-wdn-x4v3.{param,bin}
    resources/models/faces/{scrfd_2.5g.onnx,codeformer.onnx}

Usage:
    python scripts/fetch_assets.py [--target src-tauri] [--skip-faces] [--skip-general]
"""

import argparse
import hashlib
import io
import shutil
import sys
import time
import urllib.request
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from convert_srvgg_to_ncnn import convert, load_state_dict  # noqa: E402

NCNN_ZIP = (
    "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/realesrgan-ncnn-vulkan-20220424-windows.zip",
    "abc02804e17982a3be33675e4d471e91ea374e65b70167abc09e31acb412802d",
)
GENERAL_PTH = (
    "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/realesr-general-x4v3.pth",
    "8dc7edb9ac80ccdc30c3a5dca6616509367f05fbc184ad95b731f05bece96292",
)
GENERAL_WDN_PTH = (
    "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/realesr-general-wdn-x4v3.pth",
    "1641f8c4464b9f097c9fdda5589273713f67cf59f3d909e0bd688f0cee269dca",
)
FACE_MODELS = {
    "scrfd_2.5g.onnx": (
        "https://github.com/facefusion/facefusion-assets/releases/download/models-3.0.0/scrfd_2.5g.onnx",
        "2c07342347cef21a619c49dd5664fb8c09570ae9eda5bff3e385c11eafc45ada",
    ),
    "codeformer.onnx": (
        "https://github.com/facefusion/facefusion-assets/releases/download/models-3.0.0/codeformer.onnx",
        "21710e7ab61c82683576c428e9c1b6fe1ed419586b7b39e394c3449c294b550f",
    ),
}
SIDECAR_TRIPLE = "x86_64-pc-windows-msvc"


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def download(url: str, expected: str, dest: Path, retries: int = 3) -> Path:
    """Download `url` to `dest` unless it is already there with the expected hash.

    Raises:
        RuntimeError: when the downloaded file does not match `expected`.
    """
    if dest.is_file() and sha256(dest) == expected:
        print(f"  cached   {dest.name}")
        return dest
    dest.parent.mkdir(parents=True, exist_ok=True)
    for attempt in range(1, retries + 1):
        try:
            print(f"  download {dest.name}")
            with urllib.request.urlopen(url, timeout=120) as r, dest.open("wb") as out:
                shutil.copyfileobj(r, out, 1 << 20)
            break
        except OSError as e:
            if attempt == retries:
                raise
            print(f"  retry {attempt}/{retries - 1} after {e}")
            time.sleep(2 * attempt)
    actual = sha256(dest)
    if actual != expected:
        dest.unlink()
        raise RuntimeError(f"{dest.name}: sha256 mismatch\n  expected {expected}\n  actual   {actual}")
    return dest


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--target", type=Path, default=Path("src-tauri"))
    ap.add_argument("--cache", type=Path, default=Path(".asset-cache"), help="where raw downloads are kept")
    ap.add_argument("--skip-faces", action="store_true")
    ap.add_argument("--skip-general", action="store_true")
    args = ap.parse_args()

    bin_dir = args.target / "binaries"
    res_dir = args.target / "resources"
    models = res_dir / "models"
    for d in (bin_dir, models):
        d.mkdir(parents=True, exist_ok=True)

    print("Real-ESRGAN ncnn (Vulkan)")
    zpath = download(*NCNN_ZIP, args.cache / "realesrgan-ncnn-vulkan.zip")
    with zipfile.ZipFile(zpath) as z:
        names = {Path(n).name: n for n in z.namelist() if not n.endswith("/")}
        (bin_dir / f"realesrgan-ncnn-vulkan-{SIDECAR_TRIPLE}.exe").write_bytes(z.read(names["realesrgan-ncnn-vulkan.exe"]))
        (res_dir / "vcomp140.dll").write_bytes(z.read(names["vcomp140.dll"]))
        for ext in ("param", "bin"):
            (models / f"realesrgan-x4plus.{ext}").write_bytes(z.read(f"models/realesrgan-x4plus.{ext}"))

    if not args.skip_general:
        print("Real-ESRGAN general-x4v3 (converted to ncnn)")
        for (url, digest), name in ((GENERAL_PTH, "realesr-general-x4v3"), (GENERAL_WDN_PTH, "realesr-general-wdn-x4v3")):
            pth = download(url, digest, args.cache / f"{name}.pth")
            param, blob = convert(load_state_dict(pth), num_conv=32, scale=4)
            (models / f"{name}.param").write_text(param)
            (models / f"{name}.bin").write_bytes(blob)
            print(f"  converted {name} ({len(blob) / 1e6:.1f} MB)")

    if not args.skip_faces:
        print("Face models (SCRFD + CodeFormer)")
        faces = models / "faces"
        faces.mkdir(exist_ok=True)
        for name, (url, digest) in FACE_MODELS.items():
            shutil.copyfile(download(url, digest, args.cache / name), faces / name)

    print("Done.")


if __name__ == "__main__":
    main()
