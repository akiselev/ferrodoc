//! Shared behavioral qualification for embedded engine implementations.

use std::{collections::BTreeMap, time::Instant};

use ferrodoc_core::{Capability, Estimate, ScopedBlob};

use crate::{
    BlobResolver, CancellationToken, Engine, EngineError, EngineErrorCategory, EngineRequest,
    ExecutionContext, HardwareInventory, HealthRequest, HealthStatus, TraceSink,
};

/// Returns an honest inventory fixture with every host quantity unknown.
pub fn unknown_inventory() -> HardwareInventory {
    HardwareInventory {
        logical_cpus: Estimate::Unknown,
        physical_cpus: Estimate::Unknown,
        cpu_source: Estimate::Unknown,
        ram_total: Estimate::Unknown,
        ram_available: Estimate::Unknown,
        ram_source: Estimate::Unknown,
        devices: Vec::new(),
    }
}

/// Runs the common descriptor, health, estimate, execution, cancellation, and error checks.
pub fn run(
    engine: &mut dyn Engine,
    request: EngineRequest,
    input: Vec<u8>,
    inventory: &HardwareInventory,
) -> Result<(), String> {
    let descriptor = engine.descriptor().clone();
    descriptor.validate().map_err(|error| error.to_string())?;
    if !descriptor.capabilities.contains(&request.capability) {
        return Err("fixture capability is absent from descriptor".into());
    }
    let health = engine
        .health(HealthRequest::Dependencies)
        .map_err(|error| error.to_string())?;
    if health.status != HealthStatus::Healthy || health.message.trim().is_empty() {
        return Err(format!(
            "engine is not ready for conformance: {}",
            health.message
        ));
    }
    if health
        .dependencies
        .iter()
        .any(|dependency| dependency.id.trim().is_empty() || dependency.message.trim().is_empty())
    {
        return Err("health dependencies require stable IDs and diagnostics".into());
    }
    let candidates = engine
        .estimate(&request, inventory)
        .map_err(|error| error.to_string())?;
    if candidates.is_empty() {
        return Err("engine returned no placement candidates".into());
    }
    for candidate in &candidates {
        if candidate.engine_id != descriptor.id {
            return Err("candidate engine ID differs from descriptor".into());
        }
        let compatible = descriptor.compatibility.iter().any(|compatibility| {
            compatibility.backend == candidate.backend
                && compatibility.devices.contains(&candidate.device.kind())
        });
        if !compatible {
            return Err("candidate placement is absent from descriptor".into());
        }
        if !matches!(candidate.resources.source, Estimate::Known(_))
            || !matches!(candidate.resources.peak_ram, Estimate::Known(_))
        {
            return Err("candidate requires a sourced conservative RAM envelope".into());
        }
    }

    let resolver = FixtureResolver(input);
    let first = engine
        .execute(
            request.clone(),
            &context(&resolver, CancellationToken::default()),
        )
        .map_err(|error| error.to_string())?;
    if first.request_id != request.request_id {
        return Err("response request ID differs from request".into());
    }
    if descriptor.deterministic {
        let second = engine
            .execute(
                request.clone(),
                &context(&resolver, CancellationToken::default()),
            )
            .map_err(|error| error.to_string())?;
        if first != second {
            return Err("deterministic engine returned different responses".into());
        }
    }

    let cancellation = CancellationToken::default();
    cancellation.cancel();
    let error = engine
        .execute(request.clone(), &context(&resolver, cancellation))
        .expect_err("cancelled request unexpectedly succeeded");
    require_error(error, EngineErrorCategory::Cancelled)?;

    let unsupported = all_capabilities()
        .into_iter()
        .find(|capability| !descriptor.capabilities.contains(capability))
        .ok_or_else(|| "engine unexpectedly declares every capability".to_string())?;
    let mut unsupported_request = request;
    unsupported_request.capability = unsupported;
    let error = engine
        .estimate(&unsupported_request, inventory)
        .expect_err("unsupported estimate unexpectedly succeeded");
    if !matches!(
        error.category,
        EngineErrorCategory::Unsupported | EngineErrorCategory::InvalidRequest
    ) || error.message.trim().is_empty()
    {
        return Err("unsupported operation was not mapped to a structured error".into());
    }
    Ok(())
}

fn require_error(error: EngineError, expected: EngineErrorCategory) -> Result<(), String> {
    if error.category != expected || error.message.trim().is_empty() {
        return Err(format!(
            "expected {expected:?} with diagnostic, got {:?}",
            error.category
        ));
    }
    Ok(())
}

fn context(resolver: &FixtureResolver, cancellation: CancellationToken) -> ExecutionContext<'_> {
    ExecutionContext {
        cancellation,
        deadline: Some(Instant::now() + std::time::Duration::from_secs(300)),
        blobs: resolver,
        trace: &FixtureTrace,
    }
}

struct FixtureResolver(Vec<u8>);

impl BlobResolver for FixtureResolver {
    fn resolve(&self, blob: &ScopedBlob) -> Result<Vec<u8>, EngineError> {
        if blob.range.offset() != 0 || blob.range.len() != self.0.len() as u64 {
            return Err(EngineError::new(
                EngineErrorCategory::InvalidRequest,
                false,
                "conformance fixture blob range mismatch",
            ));
        }
        Ok(self.0.clone())
    }
}

struct FixtureTrace;

impl TraceSink for FixtureTrace {
    fn event(&self, _code: &str, _fields: &BTreeMap<String, String>) {}
}

fn all_capabilities() -> [Capability; 10] {
    [
        Capability::DocumentOpen,
        Capability::PageRender,
        Capability::TextExtract,
        Capability::LayoutDetect,
        Capability::ReadingOrderDetect,
        Capability::OcrPage,
        Capability::OcrRegion,
        Capability::TableRecognize,
        Capability::FormulaRecognize,
        Capability::QualityScore,
    ]
}
