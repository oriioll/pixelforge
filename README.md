# pixelforge

Mejora la calidad **real** de imágenes que se ven mal por compresión (Telegram, redes, ediciones), no solo su tamaño. Gratis, local y sin conexión: sin cuentas, sin marcas de agua, sin límites.

Está afinada para **fotos de fútbol** (césped, lluvia, cielo oscuro, luces de estadio, camisetas) y **selfies de espejo**, y busca que el resultado aguante el zoom en un móvil.

**Descarga:** [última release](https://github.com/oriioll/pixelforge/releases/latest) (Windows 10/11) · [web](https://oriioll.github.io/pixelforge/)

## Qué hace

| | Fútbol (móvil vertical) | Selfie de espejo |
| --- | --- | --- |
| Artefactos JPEG | Detección de la rejilla 8×8 y deblocking antes de escalar | Igual |
| Ruido | Denoise suave: conserva lluvia y ambiente | Más denoise de luma y croma (ruido de interior) |
| Luz | Sombras levantadas y luces de estadio contenidas, sin tocar el color de las camisetas | Sombras/altas luces, balance de blancos y niveles automáticos |
| Nitidez | Fuerte donde hay foco (jugador, escudo, césped); casi nula en el público desenfocado; solo en bordes reales | Moderada en todo el encuadre (texto, ropa, reflejos); menos sobre la piel; ojos reforzados |
| Bokeh | Si la foto original estaba desenfocada, se vuelve hacia un reescalado simple en vez de conservar textura inventada | Protección más suave |
| Caras | CodeFormer, fidelidad 0.80 | CodeFormer, fidelidad 0.90, desactivable, más reinyección de textura de piel |

Común: **Real-ESRGAN** (`realesrgan-ncnn-vulkan` como sidecar; modelos `x4plus` y `general-x4v3` con fuerza de denoise ajustable) → **supersampling** (×4 y reducción al tamaño elegido, 2K por defecto) → afinado final. Salida **2K / 4K / original ×2**, por teselas (`-t`) y por **lotes**. Slider de intensidad por preset y comparador antes/después con zoom y arrastre.

Definiciones: 2K = lado largo de 2560 px, 4K = 3840 px (pensado para fotos verticales de móvil), ×2 = el doble de la original. Se guarda JPG (calidad 95) o PNG junto a la original como `nombre_pixelforge_2k.jpg`, sin sobrescribir. No se conservan los metadatos EXIF (sí se aplica la orientación).

## Arquitectura

```
React + TS + Tailwind + shadcn/ui ──invoke/events──▶ src-tauri (Rust, Tauri 2)
                                                        │  comandos, historial, estado
                                                        ▼
                                          crates/pixelforge-core  (sin dependencias de UI)
                                          ├─ restore   deblocking + denoise previos al escalado
                                          ├─ engine    sidecar realesrgan-ncnn-vulkan (progreso, cancelar)
                                          ├─ pipeline  etapas, bokeh, caras, tono, nitidez
                                          ├─ masks     foco, césped, piel, bordes
                                          ├─ tone / sharpen / presets / detect
                                          └─ faces     SCRFD + CodeFormer (ONNX Runtime, feature `codeformer`)
```

Decisiones que conviene conocer:

- **Sin segmentación por red neuronal.** La separación sujeto/fondo se hace con un mapa de *foco local* (Laplaciano) sobre la foto original, más máscaras de césped y piel por color. Es determinista, no necesita otro modelo y respeta el bokeh por construcción. Si más adelante hace falta separar mejor, el sitio para un modelo de matting es `masks.rs`.
- **`general-x4v3` en ncnn.** El release oficial de `realesrgan-ncnn-vulkan` no lo incluye. `scripts/convert_srvgg_to_ncnn.py` convierte los `.pth` oficiales a ncnn sin PyTorch (se validó contra el `animevideov3` oficial: mismo `.param`, biases y PReLU idénticos, pesos fp16 a ≤1 ulp).
- **Fuerza de denoise.** ncnn no interpola pesos como la versión Python; con valores intermedios se ejecutan `general-x4v3` y `general-wdn-x4v3` y se mezclan las salidas (aproximación).
- **Detección de caras a 640 px.** Funciona bien en selfies y primeros planos; en planos generales de campo las caras son demasiado pequeñas y se ignoran a propósito (menos de 32 px).

## Desarrollo

Requisitos: Node 22, Rust estable y, para ejecutar la app, una GPU con Vulkan. La app completa se compila en Windows.

```bash
npm ci
python scripts/fetch_assets.py          # sidecar + modelos (≈ 420 MB, con SHA-256 fijados)
npm run tauri dev
```

`--skip-faces` y `--skip-general` omiten esas descargas. Para compilar sin CodeFormer: `npm run tauri build -- -- --no-default-features`.

Tests (no necesitan GPU):

```bash
cargo test -p pixelforge-core            # filtros, deblocking, máscaras, caras, pipeline con un sidecar simulado
python -m unittest discover scripts/tests
npm run build                            # typecheck + build del frontend
```

## CI/CD

- `ci.yml`: build del frontend, tests del core, compilación del core con CodeFormer, tests de scripts y `cargo check` del shell de Tauri en Windows.
- `release.yml`: al empujar un tag `v*` descarga los modelos, compila el instalador NSIS de Windows y lo publica en GitHub Releases. El tag debe coincidir con la versión de `src-tauri/tauri.conf.json`. Si la compilación con CodeFormer falla, publica un instalador sin caras y lo avisa en los logs.

Para publicar una versión: sube la versión en `package.json`, `src-tauri/Cargo.toml` y `src-tauri/tauri.conf.json`, y `git tag vX.Y.Z && git push origin vX.Y.Z`.

El instalador no está firmado con certificado de pago: Windows SmartScreen mostrará un aviso la primera vez.

## Hoja de ruta: Android (Fase 2)

Tauri 2 compila a Android, pero el diseño de escritorio no se traslada tal cual:

- **No hay sidecar.** Android no permite lanzar un ejecutable externo como en escritorio. Hay que compilar ncnn (con Vulkan) y el bucle de teselas de `realesrgan-ncnn-vulkan` como librería nativa del NDK y llamarla por JNI/FFI.
- **Solo `general-x4v3`** en móvil (2,4 MB, rápido). `x4plus` (33 MB) es demasiado lento y pesado para el teléfono.
- **CodeFormer fuera de la v1 móvil**: ONNX Runtime para Android es viable, pero añade ~380 MB al APK. Alternativa: descarga opcional del modelo.
- El APK se firmaría con un keystore de depuración generado en el workflow y se subiría a la misma release; la landing ya detecta un `.apk` en la última release.

## Licencias

Código bajo [MIT](LICENSE). Los modelos incluidos tienen sus propias licencias; **CodeFormer y SCRFD son de uso no comercial**. Detalle en [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
