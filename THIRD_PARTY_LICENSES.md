# Third-party components

pixelforge's own code is MIT. The installer bundles the following components, downloaded at build
time by `scripts/fetch_assets.py` (they are not stored in this repository). Each keeps its own license.

| Component | Used for | License |
| --- | --- | --- |
| [Real-ESRGAN](https://github.com/xinntao/Real-ESRGAN) and [realesrgan-ncnn-vulkan](https://github.com/xinntao/Real-ESRGAN-ncnn-vulkan) | Upscaling (models `x4plus`, `general-x4v3`) | BSD-3-Clause / MIT |
| [ncnn](https://github.com/Tencent/ncnn) | Inference engine inside the sidecar | BSD-3-Clause |
| [CodeFormer](https://github.com/sczhou/CodeFormer) (ONNX export by [FaceFusion](https://github.com/facefusion/facefusion-assets)) | Face restoration | **NTU S-Lab License 1.0: non-commercial use only** |
| SCRFD (from [InsightFace](https://github.com/deepinsight/insightface), ONNX export by FaceFusion) | Face detection | **InsightFace pretrained models: non-commercial research use** |
| [ONNX Runtime](https://github.com/microsoft/onnxruntime) via the [`ort`](https://github.com/pykeio/ort) crate | Runs CodeFormer and SCRFD | MIT / Apache-2.0 |
| [Tauri](https://tauri.app), [React](https://react.dev), [Radix UI](https://www.radix-ui.com), [Tailwind CSS](https://tailwindcss.com), [Geist font](https://vercel.com/font) | Application shell and UI | MIT / Apache-2.0 / OFL |

**Important:** because of the CodeFormer and SCRFD licenses, the *distributed installer* is for personal,
non-commercial use. To use pixelforge commercially, build it without face restoration
(`--no-default-features`) or replace those two models with ones licensed for commercial use.
