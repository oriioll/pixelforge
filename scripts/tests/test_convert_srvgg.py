"""Self-contained checks for the ncnn converter (run: python -m unittest discover scripts/tests)."""

import sys
import unittest
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).parents[1]))
from convert_srvgg_to_ncnn import NCNN_FLOAT16_TAG, convert  # noqa: E402


def fake_state(num_conv: int, feat: int = 64, scale: int = 4):
    rng = np.random.default_rng(0)
    state, idx = {}, 0
    shapes = [(feat, 3)] + [(feat, feat)] * num_conv + [(3 * scale * scale, feat)]
    for i, (o, inn) in enumerate(shapes):
        state[f"body.{idx}.weight"] = rng.standard_normal((o, inn, 3, 3)).astype(np.float32)
        state[f"body.{idx}.bias"] = rng.standard_normal(o).astype(np.float32)
        idx += 1
        if i < num_conv + 1:
            state[f"body.{idx}.weight"] = rng.random(feat).astype(np.float32)
            idx += 1
    return state


class ConvertTests(unittest.TestCase):
    def test_layer_and_blob_counts_follow_the_depth(self):
        for num_conv in (2, 16, 32):
            param, _ = convert(fake_state(num_conv), num_conv, 4)
            layers, blobs = map(int, param.splitlines()[1].split())
            self.assertEqual(layers, 2 * (num_conv + 1) + 6)  # num_conv=16 -> 40, like the official animevideov3
            self.assertEqual(blobs, 2 * (num_conv + 1) + 7)  # num_conv=16 -> 41

    def test_blob_names_match_what_realesrgan_ncnn_expects(self):
        param, _ = convert(fake_state(2), 2, 4)
        self.assertIn(" data\n", param)
        self.assertTrue(param.rstrip().endswith("output"))

    def test_bin_size_and_tags(self):
        num_conv, feat = 2, 64
        _, blob = convert(fake_state(num_conv), num_conv, 4)
        convs = [(feat, 3)] + [(feat, feat)] * num_conv + [(48, feat)]
        expected = sum(4 + o * i * 9 * 2 + o * 4 for o, i in convs) + (num_conv + 1) * feat * 4
        self.assertEqual(len(blob), expected)
        self.assertEqual(int.from_bytes(blob[:4], "little"), NCNN_FLOAT16_TAG)

    def test_wrong_scale_is_rejected(self):
        with self.assertRaises(AssertionError):
            convert(fake_state(2, scale=4), 2, 2)


if __name__ == "__main__":
    unittest.main()
