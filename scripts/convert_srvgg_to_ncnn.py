#!/usr/bin/env python3
"""Convert a Real-ESRGAN SRVGGNetCompact checkpoint (.pth) to ncnn (.param + .bin).

The official realesrgan-ncnn-vulkan release does not ship realesr-general-x4v3, but its
architecture is the same one used by realesr-animevideov3 (a stack of 3x3 convolutions with
PReLU, a pixel shuffle and a nearest-neighbour residual), only deeper. This script writes the
ncnn files directly, with no PyTorch or pnnx needed (only numpy).

The output format was checked byte for byte against the official
realesr-animevideov3-x4.{param,bin} (see tests/test_convert_srvgg.py).

Usage:
    convert_srvgg_to_ncnn.py model.pth OUT_BASENAME [--num-conv 32] [--scale 4]
"""

import argparse
import collections
import pickle
import zipfile
from pathlib import Path

import numpy as np

# ncnn storage tags: float16 weights are preceded by this magic number.
NCNN_FLOAT16_TAG = 0x01306B47

_DTYPES = {
    "FloatStorage": np.float32,
    "HalfStorage": np.float16,
    "DoubleStorage": np.float64,
}


class _Storage:
    def __init__(self, name):
        self.name = name


def load_state_dict(path: Path) -> "collections.OrderedDict[str, np.ndarray]":
    """Read a torch zip checkpoint into numpy arrays without importing torch.

    Raises:
        ValueError: if the checkpoint has no `params`/`params_ema`/flat state dict.
    """
    zf = zipfile.ZipFile(path)
    pkl = next(n for n in zf.namelist() if n.endswith("data.pkl"))
    prefix = pkl.rsplit("/", 1)[0]

    def rebuild(storage, offset, size, stride, *_):
        count = int(np.prod(size)) if len(size) else 1
        return storage[offset : offset + count].reshape(size)

    class Unpickler(pickle.Unpickler):
        def find_class(self, module, name):
            if name == "_rebuild_tensor_v2":
                return rebuild
            if module == "collections" and name == "OrderedDict":
                return collections.OrderedDict
            if name.endswith("Storage"):
                return _Storage(name)
            return super().find_class(module, name)

        def persistent_load(self, pid):
            _, storage_type, key, _loc, _numel = pid
            raw = zf.read(f"{prefix}/data/{key}")
            return np.frombuffer(raw, dtype=_DTYPES[storage_type.name])

    with zf.open(pkl) as f:
        ckpt = Unpickler(f).load()
    for key in ("params", "params_ema"):
        if key in ckpt:
            return ckpt[key]
    if any(k.startswith("body.") for k in ckpt):
        return ckpt
    raise ValueError("checkpoint has no 'params', 'params_ema' or flat state dict")


def convert(state, num_conv: int, scale: int, num_feat: int = 64):
    """Return (param_text, bin_bytes) for a SRVGGNetCompact with `num_conv` body convolutions."""
    layers = [
        "Input                    input.1                  0 1 data",
        "Split                    splitncnn_input0         1 2 data input.1_splitncnn_0 input.1_splitncnn_1",
    ]
    blobs = {"data", "input.1_splitncnn_0", "input.1_splitncnn_1"}
    bin_parts = []
    cur = "input.1_splitncnn_1"
    out_ch = 3 * scale * scale

    def conv(idx, src, dst, k, name):
        w = state[f"body.{idx}.weight"]
        b = state[f"body.{idx}.bias"]
        layers.append(f"Convolution              {name:<24} 1 1 {src} {dst} 0={w.shape[0]} 1={k} 4=1 5=1 6={w.size}")
        bin_parts.append(np.uint32(NCNN_FLOAT16_TAG).tobytes())
        bin_parts.append(w.astype(np.float16).tobytes())
        bin_parts.append(b.astype(np.float32).tobytes())
        blobs.add(dst)

    def prelu(idx, src, dst, name):
        slope = state[f"body.{idx}.weight"]
        layers.append(f"PReLU                    {name:<24} 1 1 {src} {dst} 0={slope.size}")
        bin_parts.append(slope.astype(np.float32).tobytes())
        blobs.add(dst)

    n = 0
    for i in range(num_conv + 1):  # input conv + body convs, each followed by a PReLU
        conv(2 * i, cur, f"c{i}", 3, f"Conv_{n}")
        prelu(2 * i + 1, f"c{i}", f"a{i}", f"PRelu_{n + 1}")
        cur, n = f"a{i}", n + 2
    last = 2 * (num_conv + 1)
    conv(last, cur, "pre_shuffle", 3, f"Conv_{n}")
    assert state[f"body.{last}.weight"].shape[0] == out_ch, "output channels do not match the scale"
    layers.append(f"PixelShuffle             DepthToSpace_{n + 1:<9} 1 1 pre_shuffle shuffled 0={scale}")
    layers.append(f"Interp                   Resize_{n + 3:<16} 1 1 input.1_splitncnn_0 resized 0=1 1={scale:.6e} 2={scale:.6e}")
    layers.append(f"BinaryOp                 Add_{n + 4:<19} 2 1 shuffled resized output")
    blobs.update({"shuffled", "resized", "output"})
    param = "7767517\n" + f"{len(layers)} {len(blobs)}\n" + "\n".join(layers) + "\n"
    return param, b"".join(bin_parts)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pth", type=Path)
    ap.add_argument("out", type=Path, help="output path without extension")
    ap.add_argument("--num-conv", type=int, default=32, help="body convolutions (32 for general-x4v3, 16 for animevideov3)")
    ap.add_argument("--scale", type=int, default=4)
    args = ap.parse_args()
    param, blob = convert(load_state_dict(args.pth), args.num_conv, args.scale)
    args.out.with_suffix(".param").write_text(param)
    args.out.with_suffix(".bin").write_bytes(blob)
    print(f"wrote {args.out.with_suffix('.param')} and {args.out.with_suffix('.bin')} ({len(blob)} bytes)")


if __name__ == "__main__":
    main()
