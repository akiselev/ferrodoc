//! Optional Tesseract OCR through a narrow, dynamically loaded C-API boundary.

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, CString, c_char, c_int, c_uchar, c_void},
    fs,
    path::{Path, PathBuf},
    ptr,
};

use ferrodoc_core::{
    BackendId, Bytes, CURRENT_SCHEMA_VERSION, Capability, CoordinateSpace, CoordinateTransform,
    DeterministicProvenance, DeviceId, DeviceKind, Estimate, EstimateConfidence, EstimateSource,
    EvidenceId, LayerId, MediaType, MicroUsd, PageRect, Probability, Rect, ResourceEstimate,
    Sha256Digest, Stage, Unit,
};
use ferrodoc_engine_api::{
    DependencyHealth, Engine, EngineCandidate, EngineCompatibility, EngineDescriptor, EngineError,
    EngineErrorCategory, EngineRequest, EngineResponse, ExecutionContext, HardwareInventory,
    HealthReport, HealthRequest, HealthStatus, NetworkUse,
};
use ferrodoc_ir::{Evidence, EvidenceContent};
use libloading::Library;

/// Stable engine identifier.
pub const ENGINE_ID: &str = "ocr.tesseract";
/// Engine semantic version.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Raw RGBA input media shared with the pure-Rust OCR engine.
pub const RGBA8_MEDIA_TYPE: &str = "application/vnd.ferrodoc.rgba8";
const MAXIMUM_PIXELS: u64 = 100_000_000;

/// Tesseract engine with an optional discovered native session.
pub struct TesseractEngine {
    descriptor: EngineDescriptor,
    loaded: Option<Loaded>,
    diagnostic: String,
    language: String,
}

impl TesseractEngine {
    /// Discovers a supported platform library and initializes one language.
    pub fn discover(language: impl Into<String>) -> Self {
        let language = language.into();
        let mut diagnostics = Vec::new();
        for candidate in platform_candidates() {
            match Loaded::open(Path::new(candidate), None, &language) {
                Ok(loaded) => return Self::ready(loaded, language),
                Err(error) => diagnostics.push(format!("{candidate}: {error}")),
            }
        }
        Self::unavailable(
            language,
            format!(
                "Tesseract >= 4 library or language data unavailable; tried {}",
                diagnostics.join("; ")
            ),
        )
    }

    /// Loads only the explicitly supplied native library and optional tessdata directory.
    pub fn from_paths(
        library: &Path,
        tessdata: Option<&Path>,
        language: impl Into<String>,
    ) -> Self {
        let language = language.into();
        match Loaded::open(library, tessdata, &language) {
            Ok(loaded) => Self::ready(loaded, language),
            Err(error) => Self::unavailable(
                language,
                format!("Tesseract initialization from {library:?} failed: {error}"),
            ),
        }
    }

    fn ready(loaded: Loaded, language: String) -> Self {
        let diagnostic = format!(
            "Tesseract {} is ready with language {language} and model {}",
            loaded.version, loaded.model_digest
        );
        Self {
            descriptor: descriptor(),
            loaded: Some(loaded),
            diagnostic,
            language,
        }
    }

    fn unavailable(language: String, diagnostic: String) -> Self {
        Self {
            descriptor: descriptor(),
            loaded: None,
            diagnostic,
            language,
        }
    }

    /// Runtime version when discovery succeeded.
    pub fn runtime_version(&self) -> Option<&str> {
        self.loaded.as_ref().map(|loaded| loaded.version.as_str())
    }

    /// Exact traineddata digest when discovery succeeded.
    pub fn model_digest(&self) -> Option<Sha256Digest> {
        self.loaded.as_ref().map(|loaded| loaded.model_digest)
    }
}

impl Engine for TesseractEngine {
    fn descriptor(&self) -> &EngineDescriptor {
        &self.descriptor
    }

    fn health(&mut self, _request: HealthRequest) -> Result<HealthReport, EngineError> {
        let status = if self.loaded.is_some() {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unavailable
        };
        Ok(HealthReport {
            status,
            dependencies: vec![
                DependencyHealth {
                    id: "libtesseract>=4".into(),
                    status,
                    message: self.diagnostic.clone(),
                },
                DependencyHealth {
                    id: format!("tessdata.{}", self.language),
                    status,
                    message: self.diagnostic.clone(),
                },
            ],
            message: self.diagnostic.clone(),
        })
    }

    fn estimate(
        &mut self,
        request: &EngineRequest,
        _inventory: &HardwareInventory,
    ) -> Result<Vec<EngineCandidate>, EngineError> {
        require_capability(request)?;
        Ok(vec![EngineCandidate {
            engine_id: ENGINE_ID.into(),
            backend: BackendId::new("tesseract-c-api").expect("static backend"),
            device: DeviceId::new(DeviceKind::Cpu, None).expect("static device"),
            resources: ResourceEstimate {
                peak_ram: Estimate::Known(Bytes::new(1024 * Bytes::MIB)),
                warm_ram: Estimate::Known(Bytes::new(384 * Bytes::MIB)),
                peak_vram: Estimate::Known(Bytes::new(0)),
                warm_vram: Estimate::Known(Bytes::new(0)),
                latency: Estimate::Unknown,
                remote_cost: Estimate::Known(MicroUsd::new(0)),
                quality: Estimate::Unknown,
                source: Estimate::Known(EstimateSource {
                    confidence: EstimateConfidence::Conservative,
                    method: "static Tesseract CPU process envelope".into(),
                }),
            },
        }])
    }

    fn execute(
        &mut self,
        request: EngineRequest,
        context: &ExecutionContext<'_>,
    ) -> Result<EngineResponse, EngineError> {
        require_capability(&request)?;
        context.checkpoint()?;
        if request.input.media_type != MediaType::new(RGBA8_MEDIA_TYPE).expect("static media type")
        {
            return Err(invalid(format!(
                "Tesseract input must have media type {RGBA8_MEDIA_TYPE}"
            )));
        }
        let page_index = request
            .page_index
            .ok_or_else(|| invalid("page_index is required"))?;
        let width = dimension(&request, "width")?;
        let height = dimension(&request, "height")?;
        let dpi = dimension(&request, "dpi")?;
        let pixels = u64::from(width)
            .checked_mul(u64::from(height))
            .ok_or_else(|| resource("image dimensions overflow"))?;
        if pixels > MAXIMUM_PIXELS {
            return Err(resource("image exceeds Tesseract pixel limit"));
        }
        let expected_len = pixels
            .checked_mul(4)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| resource("RGBA byte count overflow"))?;
        let bytes = context.blobs.resolve(&request.input)?;
        if bytes.len() != expected_len {
            return Err(invalid("RGBA byte length does not match width and height"));
        }
        let loaded = self.loaded.as_mut().ok_or_else(|| {
            EngineError::new(
                EngineErrorCategory::Dependency,
                false,
                self.diagnostic.clone(),
            )
        })?;
        let input_digest = request
            .input
            .expected_digest
            .unwrap_or_else(|| Sha256Digest::of_bytes(&bytes));
        loaded.image = bytes
            .chunks_exact(4)
            .map(|pixel| {
                let luminance = 299_u32 * u32::from(pixel[0])
                    + 587_u32 * u32::from(pixel[1])
                    + 114_u32 * u32::from(pixel[2]);
                (luminance / 1000) as u8
            })
            .collect();
        let width = c_int::try_from(width).map_err(|_| resource("width exceeds C API"))?;
        let height = c_int::try_from(height).map_err(|_| resource("height exceeds C API"))?;
        let dpi = c_int::try_from(dpi).map_err(|_| resource("DPI exceeds C API"))?;
        let stride = width;
        // SAFETY: the initialized handle is exclusively owned by `loaded`; RGBA bytes remain
        // alive through recognition, dimensions and stride were checked, and function pointers
        // were resolved from the retained library.
        let (text, confidence) = unsafe {
            (loaded.api.set_image)(
                loaded.handle,
                loaded.image.as_ptr(),
                width,
                height,
                1,
                stride,
            );
            (loaded.api.set_source_resolution)(loaded.handle, dpi);
            if (loaded.api.recognize)(loaded.handle, ptr::null_mut()) != 0 {
                return Err(internal("Tesseract recognition failed"));
            }
            let raw = (loaded.api.get_utf8_text)(loaded.handle);
            if raw.is_null() {
                return Err(internal("Tesseract returned a null text buffer"));
            }
            let text = CStr::from_ptr(raw).to_string_lossy().trim().to_string();
            (loaded.api.delete_text)(raw);
            let confidence =
                (loaded.api.mean_text_conf)(loaded.handle).clamp(0, 100) as f64 / 100.0;
            (text, confidence)
        };
        context.checkpoint()?;

        let mut parameters = request.parameters.clone();
        parameters.insert("language".into(), serde_json::json!(self.language));
        parameters.insert(
            "tesseract_runtime".into(),
            serde_json::json!(loaded.version),
        );
        let provenance = DeterministicProvenance {
            schema_version: CURRENT_SCHEMA_VERSION,
            input_digest,
            engine_id: ENGINE_ID.into(),
            engine_version: ENGINE_VERSION.into(),
            model_digest: Some(loaded.model_digest),
            parameters,
            stage: Stage::Ocr,
        };
        let provenance_digest = provenance
            .identity_digest()
            .map_err(|error| internal(error.to_string()))?;
        let layer_id = LayerId::derive(&[provenance_digest.as_bytes()]);
        let evidence = if text.is_empty() {
            Vec::new()
        } else {
            vec![Evidence {
                id: EvidenceId::derive(&[provenance_digest.as_bytes(), text.as_bytes()]),
                layer_id: layer_id.clone(),
                content: EvidenceContent::Text { text },
                geometry: Some(PageRect {
                    page_index,
                    rect: Rect::new(
                        0.0,
                        0.0,
                        f64::from(width),
                        f64::from(height),
                        CoordinateSpace::Image,
                        Unit::Pixel,
                    )
                    .expect("validated image dimensions"),
                    source_transform: CoordinateTransform::IDENTITY,
                }),
                confidence: Some(Probability::new(confidence).expect("clamped confidence")),
                provenance,
                engine_metadata: BTreeMap::from([
                    ("recognizer".into(), serde_json::json!("tesseract-c-api")),
                    ("runtime_version".into(), serde_json::json!(loaded.version)),
                    ("language".into(), serde_json::json!(self.language)),
                ]),
            }]
        };
        Ok(EngineResponse {
            request_id: request.request_id,
            evidence,
            metadata: BTreeMap::from([
                ("layer_id".into(), serde_json::json!(layer_id)),
                (
                    "model_digest".into(),
                    serde_json::json!(loaded.model_digest),
                ),
            ]),
        })
    }
}

fn descriptor() -> EngineDescriptor {
    EngineDescriptor {
        id: ENGINE_ID.into(),
        version: ENGINE_VERSION.into(),
        capabilities: BTreeSet::from([Capability::OcrPage, Capability::OcrRegion]),
        compatibility: vec![EngineCompatibility {
            backend: BackendId::new("tesseract-c-api").expect("static backend"),
            devices: BTreeSet::from([DeviceKind::Cpu]),
        }],
        deterministic: true,
        network_use: NetworkUse::None,
        max_concurrency: 1,
    }
}

fn require_capability(request: &EngineRequest) -> Result<(), EngineError> {
    if matches!(
        request.capability,
        Capability::OcrPage | Capability::OcrRegion
    ) {
        Ok(())
    } else {
        Err(EngineError::new(
            EngineErrorCategory::Unsupported,
            false,
            "Tesseract supports only OCR capabilities",
        ))
    }
}

fn dimension(request: &EngineRequest, key: &str) -> Result<u32, EngineError> {
    request
        .parameters
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| invalid(format!("missing or invalid {key} parameter")))
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::InvalidRequest, false, message)
}

fn resource(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::ResourceExhausted, false, message)
}

fn internal(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::Internal, false, message)
}

fn platform_candidates() -> &'static [&'static str] {
    #[cfg(target_os = "linux")]
    {
        &["libtesseract.so.5", "libtesseract.so.4", "libtesseract.so"]
    }
    #[cfg(target_os = "macos")]
    {
        &[
            "libtesseract.5.dylib",
            "libtesseract.4.dylib",
            "libtesseract.dylib",
        ]
    }
    #[cfg(target_os = "windows")]
    {
        &["libtesseract-5.dll", "tesseract55.dll", "tesseract54.dll"]
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        &[]
    }
}

struct Loaded {
    api: Api,
    handle: *mut c_void,
    version: String,
    model_digest: Sha256Digest,
    image: Vec<u8>,
}

// SAFETY: the handle is exclusively owned, all access requires `&mut self` through `Engine`,
// and Tesseract is never invoked concurrently for one instance.
unsafe impl Send for Loaded {}

impl Loaded {
    fn open(library_path: &Path, tessdata: Option<&Path>, language: &str) -> Result<Self, String> {
        // SAFETY: library constructors and symbol lookup are contained in `Api`; the library is
        // retained for at least as long as every copied function pointer.
        let api = unsafe { Api::open(library_path) }?;
        // SAFETY: `version` is a resolved function returning a library-owned C string.
        let version = unsafe {
            let pointer = (api.version)();
            if pointer.is_null() {
                return Err("TessVersion returned null".into());
            }
            CStr::from_ptr(pointer).to_string_lossy().into_owned()
        };
        let major = version
            .split_once('.')
            .map_or(version.as_str(), |(major, _)| major)
            .parse::<u32>()
            .map_err(|_| format!("unparseable Tesseract version {version:?}"))?;
        if major < 4 {
            return Err(format!(
                "unsupported Tesseract version {version}; require >= 4"
            ));
        }
        let language_c = CString::new(language).map_err(|_| "language contains NUL")?;
        let tessdata_c = tessdata
            .map(|path| {
                path.to_str()
                    .ok_or("tessdata path is not UTF-8")
                    .and_then(|path| CString::new(path).map_err(|_| "tessdata path contains NUL"))
            })
            .transpose()?;
        // SAFETY: function pointers are valid, arguments are valid C strings or null, and the
        // returned handle is checked before use.
        let handle = unsafe { (api.create)() };
        if handle.is_null() {
            return Err("TessBaseAPICreate returned null".into());
        }
        let init = unsafe {
            (api.init3)(
                handle,
                tessdata_c
                    .as_ref()
                    .map_or(ptr::null(), |path| path.as_ptr()),
                language_c.as_ptr(),
            )
        };
        if init != 0 {
            // SAFETY: the handle was created above and has not been deleted.
            unsafe { (api.delete)(handle) };
            return Err(format!(
                "language data initialization failed for {language:?}"
            ));
        }
        // SAFETY: initialized handle remains live; returned path is library-owned.
        let data_path = unsafe {
            let pointer = (api.get_datapath)(handle);
            if pointer.is_null() {
                None
            } else {
                Some(PathBuf::from(
                    CStr::from_ptr(pointer).to_string_lossy().into_owned(),
                ))
            }
        };
        let model_path = data_path
            .or_else(|| tessdata.map(Path::to_path_buf))
            .map(|directory| directory.join(format!("{language}.traineddata")));
        let model_digest = model_path
            .as_ref()
            .ok_or_else(|| "Tesseract did not disclose its tessdata path".to_string())
            .and_then(|path| {
                fs::read(path)
                    .map(|bytes| Sha256Digest::of_bytes(&bytes))
                    .map_err(|error| format!("read traineddata {path:?}: {error}"))
            });
        let model_digest = match model_digest {
            Ok(digest) => digest,
            Err(error) => {
                // SAFETY: initialized handle is being torn down exactly once.
                unsafe {
                    (api.end)(handle);
                    (api.delete)(handle);
                }
                return Err(error);
            }
        };
        let mut api = api;
        api.pin_for_process_lifetime();
        Ok(Self {
            api,
            handle,
            version,
            model_digest,
            image: Vec::new(),
        })
    }
}

impl Drop for Loaded {
    fn drop(&mut self) {
        // SAFETY: `handle` was initialized successfully and is owned by this value.
        // `TessBaseAPIDelete` performs final cleanup; an explicit `End` is optional.
        unsafe { (self.api.delete)(self.handle) };
    }
}

struct Api {
    library: Option<Library>,
    version: unsafe extern "C" fn() -> *const c_char,
    create: unsafe extern "C" fn() -> *mut c_void,
    init3: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    get_datapath: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    set_image: unsafe extern "C" fn(*mut c_void, *const c_uchar, c_int, c_int, c_int, c_int),
    set_source_resolution: unsafe extern "C" fn(*mut c_void, c_int),
    recognize: unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int,
    get_utf8_text: unsafe extern "C" fn(*mut c_void) -> *mut c_char,
    mean_text_conf: unsafe extern "C" fn(*mut c_void) -> c_int,
    delete_text: unsafe extern "C" fn(*mut c_char),
    end: unsafe extern "C" fn(*mut c_void),
    delete: unsafe extern "C" fn(*mut c_void),
}

impl Api {
    unsafe fn open(path: &Path) -> Result<Self, String> {
        // SAFETY: loading is caller-authorized discovery of a platform library; it is retained.
        let library = unsafe { Library::new(path) }.map_err(|error| error.to_string())?;
        macro_rules! symbol {
            ($name:literal, $type:ty) => {{
                // SAFETY: each symbol name and signature follows Tesseract's stable C API.
                let resolved = unsafe { library.get::<$type>(concat!($name, "\0").as_bytes()) }
                    .map_err(|error| format!("missing {}: {error}", $name))?;
                *resolved
            }};
        }
        Ok(Self {
            version: symbol!("TessVersion", unsafe extern "C" fn() -> *const c_char),
            create: symbol!("TessBaseAPICreate", unsafe extern "C" fn() -> *mut c_void),
            init3: symbol!(
                "TessBaseAPIInit3",
                unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int
            ),
            get_datapath: symbol!(
                "TessBaseAPIGetDatapath",
                unsafe extern "C" fn(*mut c_void) -> *const c_char
            ),
            set_image: symbol!(
                "TessBaseAPISetImage",
                unsafe extern "C" fn(*mut c_void, *const c_uchar, c_int, c_int, c_int, c_int)
            ),
            set_source_resolution: symbol!(
                "TessBaseAPISetSourceResolution",
                unsafe extern "C" fn(*mut c_void, c_int)
            ),
            recognize: symbol!(
                "TessBaseAPIRecognize",
                unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int
            ),
            get_utf8_text: symbol!(
                "TessBaseAPIGetUTF8Text",
                unsafe extern "C" fn(*mut c_void) -> *mut c_char
            ),
            mean_text_conf: symbol!(
                "TessBaseAPIMeanTextConf",
                unsafe extern "C" fn(*mut c_void) -> c_int
            ),
            delete_text: symbol!("TessDeleteText", unsafe extern "C" fn(*mut c_char)),
            end: symbol!("TessBaseAPIEnd", unsafe extern "C" fn(*mut c_void)),
            delete: symbol!("TessBaseAPIDelete", unsafe extern "C" fn(*mut c_void)),
            library: Some(library),
        })
    }

    fn pin_for_process_lifetime(&mut self) {
        // Some Tesseract builds retain OpenMP/runtime code references past API deletion.
        // Unloading such a library can crash background teardown. Engine transports are
        // process-scoped, so pin the successfully initialized library until OS cleanup.
        if let Some(library) = self.library.take() {
            std::mem::forget(library);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_explicit_library_is_a_health_diagnostic() {
        let mut engine = TesseractEngine::from_paths(
            Path::new("/definitely/missing/libtesseract.so"),
            None,
            "eng",
        );
        let health = engine.health(HealthRequest::Dependencies).unwrap();
        assert_eq!(health.status, HealthStatus::Unavailable);
        assert!(health.message.contains("failed"));
    }

    #[test]
    fn descriptor_is_portable_and_network_free() {
        let engine = TesseractEngine::discover("eng");
        engine.descriptor().validate().unwrap();
        assert_eq!(engine.descriptor().network_use, NetworkUse::None);
        assert_eq!(
            engine.descriptor().compatibility[0].devices,
            BTreeSet::from([DeviceKind::Cpu])
        );
    }
}
