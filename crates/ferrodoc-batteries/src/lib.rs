//! Optional batteries-included composition for applications that want official
//! Ferrodoc engines linked directly instead of discovered as isolated processes.
//!
//! The default feature set stays CPU-oriented. Heavy VLM/ORT engines and GPU
//! backends are opt-in so consumers retain control over compile time and native
//! dependencies.

use anyhow::Result;
use ferrodoc_pipeline::Pipeline;

/// Register every engine enabled by this crate's Cargo features into `pipeline`.
/// The same engine implementations are used by the standalone Cargo plugins;
/// only the transport/isolation boundary changes.
pub fn register_enabled(pipeline: &mut Pipeline) -> Result<()> {
    #[cfg(feature = "rulebased")]
    pipeline.add_embedded_engine(ferrodoc_layout_rulebased::RuleBasedLayoutEngine::default())?;
    #[cfg(feature = "ocrs")]
    pipeline.add_embedded_engine(ferrodoc_engine_ocrs::OcrsEngine::new())?;
    #[cfg(feature = "burn")]
    pipeline.add_embedded_engine(ferrodoc_engine_burn::BurnRouterEngine::new())?;
    #[cfg(feature = "tesseract")]
    pipeline.add_embedded_engine(ferrodoc_engine_tesseract::TesseractEngine::new())?;
    #[cfg(feature = "ort")]
    pipeline.add_embedded_engine(ferrodoc_engine_ort::OrtEngine::new())?;
    #[cfg(feature = "oar")]
    pipeline.add_embedded_engine(ferrodoc_engine_oar::OarEngine::new())?;
    #[cfg(feature = "oar-classic")]
    pipeline.add_embedded_engine(ferrodoc_engine_oar_classic::OarClassicEngine::new())?;
    #[cfg(feature = "llamacpp")]
    pipeline.add_embedded_engine(ferrodoc_engine_llamacpp::LlamaCppEngine::new())?;
    #[cfg(feature = "mistralrs")]
    pipeline.add_embedded_engine(ferrodoc_engine_mistralrs::MistralRsEngine::new())?;
    #[cfg(feature = "remote")]
    pipeline.add_embedded_engine(ferrodoc_engine_remote::RemoteEngine::default())?;
    #[cfg(feature = "mistral-ocr")]
    pipeline.add_embedded_engine(ferrodoc_engine_mistral::MistralOcrEngine::default())?;
    Ok(())
}
