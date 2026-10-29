//! Direct2D, DirectWrite and DirectComposition plumbing shared by every widget.

use std::collections::HashMap;

use inlay_core::color::Rgba;
use inlay_core::config::Align;
use windows::Win32::Foundation::{E_FAIL, HMODULE, HWND};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Imaging::IWICBitmap;
use windows::core::{BOOL, Error, HSTRING, Interface, Param, Result, w};
use windows_numerics::{Matrix3x2, Vector2};

pub const ICON_FONT: &str = "Segoe Fluent Icons";

const PIXEL_FORMAT: D2D1_PIXEL_FORMAT = D2D1_PIXEL_FORMAT {
    format: DXGI_FORMAT_B8G8R8A8_UNORM,
    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
};

pub fn rect(x: f32, y: f32, w: f32, h: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

pub fn point(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

fn color(c: Rgba) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: c[0],
        g: c[1],
        b: c[2],
        a: c[3],
    }
}

fn rounded(r: D2D_RECT_F, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: r,
        radiusX: radius,
        radiusY: radius,
    }
}

/// Decoration drawn beneath a piece of text.
#[derive(Clone, Copy, Default)]
pub struct TextFx {
    pub shadow: Option<Rgba>,
    /// Outline colour and width.
    pub stroke: Option<(Rgba, f32)>,
}

/// The eight directions an outline is stamped in.
const OUTLINE: [(f32, f32); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (0.707, 0.707),
    (-0.707, 0.707),
    (0.707, -0.707),
    (-0.707, -0.707),
];

pub struct TextStyle<'a> {
    pub font: &'a str,
    pub size: f32,
    pub weight: u32,
    pub align: Align,
    /// Wrap onto more lines; when false the text is cut with an ellipsis.
    pub wrap: bool,
}

/// The parts only needed to put pixels in a window.
struct Devices {
    dxgi_device: IDXGIDevice,
    dxgi_factory: IDXGIFactory2,
    dcomp: IDCompositionDevice,
}

/// A window's swap chain and the composition objects that show it.
pub struct Surface {
    swap: IDXGISwapChain1,
    bitmap: Option<ID2D1Bitmap1>,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
}

pub struct Gfx {
    /// Used for the two calls that exist only on a device context.
    dc: ID2D1DeviceContext,
    /// The same object. Drawing goes through the base interface, whose
    /// method names do not collide with the device context's overloads.
    pub rt: ID2D1RenderTarget,
    factory: ID2D1Factory1,
    dwrite: IDWriteFactory,
    brush: ID2D1SolidColorBrush,
    formats: HashMap<(String, u32, u32), IDWriteTextFormat>,
    /// Fonts from the app's own `fonts` folder, if it holds any.
    fonts: Option<IDWriteFontCollection>,
    devices: Option<Devices>,
}

/// Loads every font file in the config folder's `fonts` directory, so a
/// font can be used here without installing it in Windows.
fn load_fonts(dwrite: &IDWriteFactory) -> Option<IDWriteFontCollection> {
    let files: Vec<_> = std::fs::read_dir(crate::config_dir().join("fonts"))
        .ok()?
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| {
                    ["ttf", "otf", "ttc"].contains(&ext.to_ascii_lowercase().as_str())
                })
        })
        .collect();
    if files.is_empty() {
        return None;
    }
    unsafe {
        let factory: IDWriteFactory5 = dwrite.cast().ok()?;
        let builder: IDWriteFontSetBuilder1 = factory.CreateFontSetBuilder().ok()?;
        for path in &files {
            let added = dwrite
                .CreateFontFileReference(&HSTRING::from(path.as_os_str()), None)
                .and_then(|file| builder.AddFontFile(&file));
            if let Err(error) = added {
                crate::log(&format!("could not load font {}: {error}", path.display()));
            }
        }
        let set = builder.CreateFontSet().ok()?;
        factory
            .CreateFontCollectionFromFontSet(&set)
            .ok()?
            .cast()
            .ok()
    }
}

fn has_family(collection: &IDWriteFontCollection, family: &str) -> bool {
    let (mut index, mut exists) = (0, BOOL(0));
    let found =
        unsafe { collection.FindFamilyName(&HSTRING::from(family), &mut index, &mut exists) };
    found.is_ok() && exists.as_bool()
}

fn create_d3d() -> Result<ID3D11Device> {
    // An always-on widget must not keep a laptop's discrete GPU awake, so the
    // low-power adapter is asked for by name rather than left to the default.
    let frugal: Option<IDXGIAdapter> = unsafe {
        CreateDXGIFactory1::<IDXGIFactory6>()
            .and_then(|factory| {
                factory.EnumAdapterByGpuPreference(0, DXGI_GPU_PREFERENCE_MINIMUM_POWER)
            })
            .ok()
    };
    // The drawing here is a few rectangles and some text. Driver worker
    // threads would cost memory and buy nothing.
    let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT
        | D3D11_CREATE_DEVICE_PREVENT_INTERNAL_THREADING_OPTIMIZATIONS;

    let mut device = None;
    let attempts = [
        (frugal.as_ref(), D3D_DRIVER_TYPE_UNKNOWN),
        (None, D3D_DRIVER_TYPE_HARDWARE),
        // The software fallback: a CI runner or a remote session has no GPU.
        (None, D3D_DRIVER_TYPE_WARP),
    ];
    for (adapter, driver) in attempts {
        if adapter.is_none() && driver == D3D_DRIVER_TYPE_UNKNOWN {
            continue;
        }
        let created = unsafe {
            D3D11CreateDevice(
                adapter,
                driver,
                HMODULE::default(),
                flags,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )
        };
        if created.is_ok() {
            break;
        }
    }
    device.ok_or_else(|| Error::from(E_FAIL))
}

impl Gfx {
    /// For drawing into windows.
    pub fn new() -> Result<Self> {
        unsafe {
            let dxgi_device: IDXGIDevice = create_d3d()?.cast()?;
            let factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dc = factory
                .CreateDevice(&dxgi_device)?
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            let adapter = dxgi_device.GetAdapter()?;
            if let Ok(desc) = adapter.GetDesc() {
                // Worth one log line: which GPU this runs on decides its cost.
                let name = String::from_utf16_lossy(&desc.Description);
                crate::log(&format!(
                    "graphics adapter: {}",
                    name.trim_end_matches('\0')
                ));
            }
            let devices = Devices {
                // The factory the device's own adapter came from.
                dxgi_factory: adapter.GetParent()?,
                dcomp: DCompositionCreateDevice(&dxgi_device)?,
                dxgi_device,
            };
            Self::with_context(dc, factory, Some(devices))
        }
    }

    /// For drawing into an in-memory bitmap, with no GPU and no window.
    pub fn for_bitmap(bitmap: &IWICBitmap) -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                pixelFormat: PIXEL_FORMAT,
                dpiX: 96.0,
                dpiY: 96.0,
                ..Default::default()
            };
            let dc = factory
                .CreateWicBitmapRenderTarget(bitmap, &props)?
                .cast()?;
            Self::with_context(dc, factory, None)
        }
    }

    fn with_context(
        dc: ID2D1DeviceContext,
        factory: ID2D1Factory1,
        devices: Option<Devices>,
    ) -> Result<Self> {
        let rt: ID2D1RenderTarget = dc.cast()?;
        unsafe {
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            Ok(Self {
                brush: rt.CreateSolidColorBrush(&color([1.0; 4]), None)?,
                fonts: load_fonts(&dwrite),
                dwrite,
                formats: HashMap::new(),
                dc,
                rt,
                factory,
                devices,
            })
        }
    }

    // ----- window surfaces -------------------------------------------------

    pub fn surface(&self, hwnd: HWND, w: u32, h: u32) -> Result<Surface> {
        let devices = self.devices.as_ref().ok_or_else(|| Error::from(E_FAIL))?;
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: w.max(1),
            Height: h.max(1),
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            ..Default::default()
        };
        unsafe {
            let swap = devices.dxgi_factory.CreateSwapChainForComposition(
                &devices.dxgi_device,
                &desc,
                None,
            )?;
            let target = devices.dcomp.CreateTargetForHwnd(hwnd, true)?;
            let visual = devices.dcomp.CreateVisual()?;
            visual.SetContent(&swap)?;
            target.SetRoot(&visual)?;
            devices.dcomp.Commit()?;
            Ok(Surface {
                bitmap: Some(self.back_buffer(&swap)?),
                swap,
                _target: target,
                _visual: visual,
            })
        }
    }

    fn back_buffer(&self, swap: &IDXGISwapChain1) -> Result<ID2D1Bitmap1> {
        let props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: PIXEL_FORMAT,
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            ..Default::default()
        };
        unsafe {
            let buffer: IDXGISurface = swap.GetBuffer(0)?;
            self.dc.CreateBitmapFromDxgiSurface(&buffer, Some(&props))
        }
    }

    pub fn resize(&self, surface: &mut Surface, w: u32, h: u32) -> Result<()> {
        // Every reference to the old buffers has to go before they can be resized.
        surface.bitmap = None;
        unsafe {
            self.dc.SetTarget(None);
            surface.swap.ResizeBuffers(
                2,
                w.max(1),
                h.max(1),
                DXGI_FORMAT_UNKNOWN,
                DXGI_SWAP_CHAIN_FLAG(0),
            )?;
            surface.bitmap = Some(self.back_buffer(&surface.swap)?);
        }
        Ok(())
    }

    /// Starts a frame on a window, cleared to fully transparent.
    pub fn begin(&self, surface: &Surface) {
        unsafe {
            if let Some(bitmap) = &surface.bitmap {
                self.dc.SetTarget(bitmap);
            }
            self.begin_draw();
        }
    }

    pub fn end(&self, surface: &Surface) -> Result<()> {
        unsafe {
            let drawn = self.end_draw();
            self.dc.SetTarget(None);
            drawn?;
            surface.swap.Present(0, DXGI_PRESENT(0)).ok()
        }
    }

    pub fn begin_draw(&self) {
        unsafe {
            self.rt.BeginDraw();
            self.rt.SetTransform(&Matrix3x2::identity());
            self.rt.Clear(Some(&color([0.0; 4])));
        }
    }

    pub fn end_draw(&self) -> Result<()> {
        unsafe { self.rt.EndDraw(None, None) }
    }

    /// Scales and offsets everything drawn after it. Widgets draw in
    /// display-independent pixels from their own top-left corner.
    pub fn set_transform(&self, scale: f32, dx: f32, dy: f32) {
        let matrix = Matrix3x2 {
            M11: scale,
            M12: 0.0,
            M21: 0.0,
            M22: scale,
            M31: dx,
            M32: dy,
        };
        unsafe { self.rt.SetTransform(&matrix) };
    }

    /// Enlarges everything drawn from here on about the point `(cx, cy)`,
    /// which stays where it is. Returns the transform to `restore`.
    pub fn scale_about(&self, factor: f32, cx: f32, cy: f32) -> Matrix3x2 {
        let mut old = Matrix3x2 {
            M11: 1.0,
            M12: 0.0,
            M21: 0.0,
            M22: 1.0,
            M31: 0.0,
            M32: 0.0,
        };
        unsafe { self.rt.GetTransform(&mut old) };
        // The transform in place is only ever a scale and an offset, so the
        // two compose by hand: enlarge about the point, then apply the old.
        let matrix = Matrix3x2 {
            M11: factor * old.M11,
            M22: factor * old.M22,
            M31: cx * (1.0 - factor) * old.M11 + old.M31,
            M32: cy * (1.0 - factor) * old.M22 + old.M32,
            ..old
        };
        unsafe { self.rt.SetTransform(&matrix) };
        old
    }

    /// Turns everything drawn from here on by `quarter` quarter turns
    /// clockwise about the origin (-1 is anticlockwise, 2 a half turn), then moves it by
    /// `(dx, dy)`. Returns the transform to `restore`.
    pub fn turn(&self, quarter: i8, dx: f32, dy: f32) -> Matrix3x2 {
        let mut old = Matrix3x2::identity();
        unsafe { self.rt.GetTransform(&mut old) };
        let (sin, cos) = match quarter {
            1 => (1.0, 0.0),
            -1 => (-1.0, 0.0),
            2 => (0.0, -1.0),
            _ => (0.0, 1.0),
        };
        // As in `scale_about`, the transform in place is a scale and an offset.
        let matrix = Matrix3x2 {
            M11: cos * old.M11,
            M12: sin * old.M22,
            M21: -sin * old.M11,
            M22: cos * old.M22,
            M31: dx * old.M11 + old.M31,
            M32: dy * old.M22 + old.M32,
        };
        unsafe { self.rt.SetTransform(&matrix) };
        old
    }

    pub fn restore(&self, transform: Matrix3x2) {
        unsafe { self.rt.SetTransform(&transform) };
    }

    // ----- shapes ----------------------------------------------------------

    fn solid(&self, c: Rgba) -> &ID2D1SolidColorBrush {
        unsafe { self.brush.SetColor(&color(c)) };
        &self.brush
    }

    pub fn fill_rect(&self, r: D2D_RECT_F, c: Rgba) {
        unsafe { self.rt.FillRectangle(&r, self.solid(c)) };
    }

    /// Restricts drawing to `r` until `pop_clip`.
    pub fn push_clip(&self, r: D2D_RECT_F) {
        unsafe {
            self.rt.PushAxisAlignedClip(&r, D2D1_ANTIALIAS_MODE_ALIASED);
        }
    }

    pub fn pop_clip(&self) {
        unsafe { self.rt.PopAxisAlignedClip() };
    }

    pub fn fill_round(&self, r: D2D_RECT_F, radius: f32, c: Rgba) {
        unsafe {
            self.rt
                .FillRoundedRectangle(&rounded(r, radius), self.solid(c));
        }
    }

    pub fn stroke_round(&self, r: D2D_RECT_F, radius: f32, c: Rgba, width: f32) {
        unsafe {
            self.rt
                .DrawRoundedRectangle(&rounded(r, radius), self.solid(c), width, None);
        }
    }

    pub fn fill_circle(&self, x: f32, y: f32, radius: f32, c: Rgba) {
        let ellipse = D2D1_ELLIPSE {
            point: point(x, y),
            radiusX: radius,
            radiusY: radius,
        };
        unsafe { self.rt.FillEllipse(&ellipse, self.solid(c)) };
    }

    /// A quarter of a disc: straight from `centre` to `from`, clockwise
    /// round the arc to `to`, and straight back.
    pub fn fill_quarter(
        &self,
        centre: (f32, f32),
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        c: Rgba,
    ) -> Result<()> {
        unsafe {
            let geometry = self.factory.CreatePathGeometry()?;
            let sink = geometry.Open()?;
            sink.BeginFigure(point(centre.0, centre.1), D2D1_FIGURE_BEGIN_FILLED);
            sink.AddLine(point(from.0, from.1));
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: point(to.0, to.1),
                size: D2D_SIZE_F {
                    width: radius,
                    height: radius,
                },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: D2D1_ARC_SIZE_SMALL,
            });
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            self.rt.FillGeometry(&geometry, self.solid(c), None);
        }
        Ok(())
    }

    /// A horizontal gradient across `[x0, x1]`.
    pub fn gradient(&self, x0: f32, x1: f32, a: Rgba, b: Rgba) -> Result<ID2D1LinearGradientBrush> {
        self.linear_gradient(point(x0, 0.0), point(x1, 0.0), a, b)
    }

    /// A vertical gradient across `[y0, y1]`.
    pub fn vertical_gradient(
        &self,
        y0: f32,
        y1: f32,
        a: Rgba,
        b: Rgba,
    ) -> Result<ID2D1LinearGradientBrush> {
        self.linear_gradient(point(0.0, y0), point(0.0, y1), a, b)
    }

    fn linear_gradient(
        &self,
        start: Vector2,
        end: Vector2,
        a: Rgba,
        b: Rgba,
    ) -> Result<ID2D1LinearGradientBrush> {
        let stops = [
            D2D1_GRADIENT_STOP {
                position: 0.0,
                color: color(a),
            },
            D2D1_GRADIENT_STOP {
                position: 1.0,
                color: color(b),
            },
        ];
        let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
            startPoint: start,
            endPoint: end,
        };
        unsafe {
            let collection = self.rt.CreateGradientStopCollection(
                &stops,
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            )?;
            self.rt.CreateLinearGradientBrush(&props, None, &collection)
        }
    }

    pub fn fill_round_with(&self, r: D2D_RECT_F, radius: f32, brush: &ID2D1Brush) {
        unsafe { self.rt.FillRoundedRectangle(&rounded(r, radius), brush) };
    }

    /// A closed, filled shape through `points`, smoothed with Catmull-Rom
    /// curves, standing on the baseline `base_y`.
    pub fn fill_curve(&self, points: &[(f32, f32)], base_y: f32, brush: &ID2D1Brush) -> Result<()> {
        let (Some(&first), Some(&last)) = (points.first(), points.last()) else {
            return Ok(());
        };
        let at = |i: isize| points[i.clamp(0, points.len() as isize - 1) as usize];
        unsafe {
            let geometry = self.factory.CreatePathGeometry()?;
            let sink = geometry.Open()?;
            sink.BeginFigure(point(first.0, base_y), D2D1_FIGURE_BEGIN_FILLED);
            sink.AddLine(point(first.0, first.1));
            for i in 0..points.len() as isize - 1 {
                let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
                sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: point(p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0),
                    point2: point(p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0),
                    point3: point(p2.0, p2.1),
                });
            }
            sink.AddLine(point(last.0, base_y));
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            self.rt.FillGeometry(&geometry, brush, None);
        }
        Ok(())
    }

    // ----- bitmaps ---------------------------------------------------------

    /// Uploads premultiplied BGRA pixels.
    pub fn bitmap(&self, w: u32, h: u32, bgra: &[u8]) -> Result<ID2D1Bitmap> {
        if bgra.len() < (w * h * 4) as usize || w == 0 || h == 0 {
            return Err(Error::from(E_FAIL));
        }
        let props = D2D1_BITMAP_PROPERTIES {
            pixelFormat: PIXEL_FORMAT,
            dpiX: 96.0,
            dpiY: 96.0,
        };
        let size = D2D_SIZE_U {
            width: w,
            height: h,
        };
        unsafe {
            self.rt
                .CreateBitmap(size, Some(bgra.as_ptr().cast()), w * 4, &props)
        }
    }

    /// Fills a rounded rectangle with a bitmap scaled to cover it, centre-cropped.
    pub fn fill_round_bitmap(
        &self,
        bitmap: &ID2D1Bitmap,
        dest: D2D_RECT_F,
        radius: f32,
        opacity: f32,
    ) -> Result<()> {
        let size = unsafe { bitmap.GetSize() };
        let (dw, dh) = (dest.right - dest.left, dest.bottom - dest.top);
        let scale = (dw / size.width).max(dh / size.height);
        let brush_props = D2D1_BRUSH_PROPERTIES {
            opacity,
            transform: Matrix3x2 {
                M11: scale,
                M12: 0.0,
                M21: 0.0,
                M22: scale,
                M31: dest.left + (dw - size.width * scale) / 2.0,
                M32: dest.top + (dh - size.height * scale) / 2.0,
            },
        };
        let bitmap_props = D2D1_BITMAP_BRUSH_PROPERTIES {
            extendModeX: D2D1_EXTEND_MODE_CLAMP,
            extendModeY: D2D1_EXTEND_MODE_CLAMP,
            interpolationMode: D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
        };
        unsafe {
            let brush =
                self.rt
                    .CreateBitmapBrush(bitmap, Some(&bitmap_props), Some(&brush_props))?;
            self.rt.FillRoundedRectangle(&rounded(dest, radius), &brush);
        }
        Ok(())
    }

    // ----- text ------------------------------------------------------------

    /// The families loaded from the app's own `fonts` folder.
    pub fn own_fonts(&self) -> Vec<String> {
        let Some(fonts) = &self.fonts else {
            return Vec::new();
        };
        unsafe {
            (0..fonts.GetFontFamilyCount())
                .filter_map(|index| {
                    let names = fonts.GetFontFamily(index).ok()?.GetFamilyNames().ok()?;
                    let len = names.GetStringLength(0).ok()? as usize;
                    let mut name = vec![0u16; len + 1];
                    names.GetString(0, &mut name).ok()?;
                    Some(String::from_utf16_lossy(&name[..len]))
                })
                .collect()
        }
    }

    /// Whether a font family can be used: from the app's own folder or
    /// installed in Windows. DirectWrite substitutes silently when it cannot.
    pub fn has_font(&self, family: &str) -> bool {
        if self
            .fonts
            .as_ref()
            .is_some_and(|own| has_family(own, family))
        {
            return true;
        }
        let mut system = None;
        let got = unsafe { self.dwrite.GetSystemFontCollection(&mut system, false) };
        got.is_ok() && system.is_some_and(|fonts| has_family(&fonts, family))
    }

    pub fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_w: f32,
        max_h: f32,
    ) -> Result<IDWriteTextLayout> {
        let key = (
            style.font.to_owned(),
            (style.size * 4.0) as u32,
            style.weight,
        );
        let format = match self.formats.get(&key) {
            Some(format) => format.clone(),
            None => {
                let format = unsafe {
                    self.dwrite.CreateTextFormat(
                        &HSTRING::from(style.font),
                        // A family from the app's own folder, else the system's.
                        self.fonts
                            .as_ref()
                            .filter(|own| has_family(own, style.font)),
                        DWRITE_FONT_WEIGHT(style.weight as i32),
                        DWRITE_FONT_STYLE_NORMAL,
                        DWRITE_FONT_STRETCH_NORMAL,
                        style.size,
                        w!("en-us"),
                    )?
                };
                self.formats.insert(key, format.clone());
                format
            }
        };

        let wide: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let layout =
                self.dwrite
                    .CreateTextLayout(&wide, &format, max_w.max(1.0), max_h.max(1.0))?;
            layout.SetTextAlignment(match style.align {
                Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
            })?;
            if style.wrap {
                layout.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
            } else {
                layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
                let trimming = DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: 0,
                    delimiterCount: 0,
                };
                let ellipsis = self.dwrite.CreateEllipsisTrimmingSign(&format)?;
                layout.SetTrimming(&trimming, &ellipsis)?;
            }
            Ok(layout)
        }
    }

    /// Adds `spacing` after every character, for small-caps style labels.
    pub fn letter_space(layout: &IDWriteTextLayout, chars: u32, spacing: f32) -> Result<()> {
        let range = DWRITE_TEXT_RANGE {
            startPosition: 0,
            length: chars,
        };
        unsafe {
            layout
                .cast::<IDWriteTextLayout1>()?
                .SetCharacterSpacing(0.0, spacing, 0.0, range)
        }
    }

    /// Width and height of the laid-out text.
    pub fn measure(layout: &IDWriteTextLayout) -> (f32, f32) {
        let mut metrics = DWRITE_TEXT_METRICS::default();
        // On failure the metrics stay zero, which draws nothing rather than wrongly.
        let _ = unsafe { layout.GetMetrics(&mut metrics) };
        (metrics.widthIncludingTrailingWhitespace, metrics.height)
    }

    /// The boxes covering a run of UTF-16 code units, one per visual line.
    pub fn range_rects(layout: &IDWriteTextLayout, start: u32, len: u32) -> Vec<D2D_RECT_F> {
        let mut boxes = [DWRITE_HIT_TEST_METRICS::default(); 8];
        let mut count = 0;
        let hit =
            unsafe { layout.HitTestTextRange(start, len, 0.0, 0.0, Some(&mut boxes), &mut count) };
        if hit.is_err() {
            return Vec::new();
        }
        boxes[..(count as usize).min(boxes.len())]
            .iter()
            .map(|b| rect(b.left, b.top, b.width, b.height))
            .collect()
    }

    /// Draws text over an optional shadow and outline. The decoration's own
    /// colours are given at full strength; everything is faded together by
    /// the alpha of `c`.
    ///
    /// The outline is the text stamped eight times around itself. A true
    /// outline needs the glyph geometry; for the widths a lyric line wants,
    /// stamping is indistinguishable and a fraction of the code.
    pub fn draw_text_fx(&self, layout: &IDWriteTextLayout, x: f32, y: f32, c: Rgba, fx: &TextFx) {
        let alpha = c[3];
        // Translucent stamps pile up where they overlap and come out nearly
        // opaque. With an outline, the whole thing is drawn solid into a
        // layer and the layer is faded instead.
        let layered = fx.stroke.is_some() && alpha < 0.999;
        let scale = if layered { 1.0 } else { alpha };
        let reach = fx.stroke.map_or(0.0, |(_, width)| width);
        unsafe {
            if layered {
                let everywhere = D2D_RECT_F {
                    left: f32::MIN,
                    top: f32::MIN,
                    right: f32::MAX,
                    bottom: f32::MAX,
                };
                let params = D2D1_LAYER_PARAMETERS {
                    contentBounds: everywhere,
                    maskTransform: Matrix3x2::identity(),
                    opacity: alpha,
                    ..Default::default()
                };
                self.rt.PushLayer(&params, None);
            }
            if let Some(shadow) = fx.shadow {
                for (offset, share) in [(1.0, 1.0), (3.0, 0.42)] {
                    self.rt.DrawTextLayout(
                        point(x, y + offset + reach),
                        layout,
                        self.solid([shadow[0], shadow[1], shadow[2], shadow[3] * share * scale]),
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                    );
                }
            }
            if let Some((color, width)) = fx.stroke {
                for (dx, dy) in OUTLINE {
                    self.rt.DrawTextLayout(
                        point(x + dx * width, y + dy * width),
                        layout,
                        self.solid([color[0], color[1], color[2], color[3] * scale]),
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                    );
                }
            }
            self.rt.DrawTextLayout(
                point(x, y),
                layout,
                self.solid([c[0], c[1], c[2], scale]),
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
            );
            if layered {
                self.rt.PopLayer();
            }
        }
    }

    pub fn draw_text(&self, layout: &IDWriteTextLayout, x: f32, y: f32, c: Rgba, shadow: bool) {
        unsafe {
            if shadow {
                // Two offset passes read as a soft shadow at a fraction of the
                // cost of a real blur, and keep text legible on a bright wallpaper.
                for (offset, alpha) in [(1.0, 0.38), (3.0, 0.16)] {
                    self.rt.DrawTextLayout(
                        point(x, y + offset),
                        layout,
                        self.solid([0.0, 0.0, 0.0, alpha * c[3]]),
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                    );
                }
            }
            self.rt.DrawTextLayout(
                point(x, y),
                layout,
                self.solid(c),
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
            );
        }
    }

    /// Draws `layout` only inside `clip`, for highlighting part of a line.
    pub fn draw_text_clipped(
        &self,
        layout: &IDWriteTextLayout,
        x: f32,
        y: f32,
        c: Rgba,
        clip: D2D_RECT_F,
    ) {
        self.draw_text_with(layout, x, y, self.solid(c), clip);
    }

    /// Draws `layout` only inside `clip`, filled with `brush`. A gradient
    /// brush is positioned in the widget's coordinates, not the layout's.
    pub fn draw_text_with<B: Param<ID2D1Brush>>(
        &self,
        layout: &IDWriteTextLayout,
        x: f32,
        y: f32,
        brush: B,
        clip: D2D_RECT_F,
    ) {
        let clip = D2D_RECT_F {
            left: clip.left + x,
            top: clip.top + y,
            right: clip.right + x,
            bottom: clip.bottom + y,
        };
        unsafe {
            self.rt
                .PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
            self.rt.DrawTextLayout(
                point(x, y),
                layout,
                brush,
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
            );
            self.rt.PopAxisAlignedClip();
        }
    }
}
