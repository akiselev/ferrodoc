//! Runtime composition skeleton.
//!
//! Phase 1 supports explicit embedded-engine registration only. Planning,
//! scheduling, process hosting, caching, and model coordination enter in their
//! designated phases rather than as placeholder behavior.

use std::collections::BTreeMap;

use ferrodoc_engine_api::{
    Engine, EngineDescriptor, EngineError, EngineRequest, EngineResponse, ExecutionContext,
    HealthReport, HealthRequest,
};
use thiserror::Error;

/// Embedded registry failure.
#[derive(Debug, Error)]
pub enum RuntimeError {
    /// Descriptor validation or engine execution failed.
    #[error(transparent)]
    Engine(#[from] EngineError),
    /// An engine ID was registered more than once.
    #[error("duplicate engine ID {0:?}")]
    DuplicateEngine(String),
    /// No registered engine has the requested ID.
    #[error("unknown engine ID {0:?}")]
    UnknownEngine(String),
}

/// Explicit registry of embedded engine implementations.
#[derive(Default)]
pub struct EmbeddedRegistry {
    engines: BTreeMap<String, Box<dyn Engine>>,
}

impl EmbeddedRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one engine after validating its descriptor.
    pub fn register(&mut self, engine: impl Engine + 'static) -> Result<(), RuntimeError> {
        engine.descriptor().validate()?;
        let id = engine.descriptor().id.clone();
        if self.engines.contains_key(&id) {
            return Err(RuntimeError::DuplicateEngine(id));
        }
        self.engines.insert(id, Box::new(engine));
        Ok(())
    }

    /// Returns descriptors in stable engine-ID order.
    pub fn descriptors(&self) -> Vec<&EngineDescriptor> {
        self.engines
            .values()
            .map(|engine| engine.descriptor())
            .collect()
    }

    /// Executes a health check on one embedded engine.
    pub fn health(
        &mut self,
        engine_id: &str,
        request: HealthRequest,
    ) -> Result<HealthReport, RuntimeError> {
        self.engines
            .get_mut(engine_id)
            .ok_or_else(|| RuntimeError::UnknownEngine(engine_id.into()))?
            .health(request)
            .map_err(Into::into)
    }

    /// Executes one request directly through the semantic engine API.
    pub fn execute(
        &mut self,
        engine_id: &str,
        request: EngineRequest,
        context: &ExecutionContext<'_>,
    ) -> Result<EngineResponse, RuntimeError> {
        self.engines
            .get_mut(engine_id)
            .ok_or_else(|| RuntimeError::UnknownEngine(engine_id.into()))?
            .execute(request, context)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ferrodoc_core::{BackendId, Capability, DeviceKind};
    use ferrodoc_engine_api::{
        EngineCandidate, EngineCompatibility, HardwareInventory, HealthStatus, NetworkUse,
    };

    use super::*;

    struct Mock {
        descriptor: EngineDescriptor,
    }

    impl Mock {
        fn new(id: &str) -> Self {
            Self {
                descriptor: EngineDescriptor {
                    id: id.into(),
                    version: "1.0.0".into(),
                    capabilities: BTreeSet::from([Capability::OcrPage]),
                    compatibility: vec![EngineCompatibility {
                        backend: BackendId::new("mock").unwrap(),
                        devices: BTreeSet::from([DeviceKind::Cpu]),
                    }],
                    deterministic: true,
                    network_use: NetworkUse::None,
                    max_concurrency: 1,
                },
            }
        }
    }

    impl Engine for Mock {
        fn descriptor(&self) -> &EngineDescriptor {
            &self.descriptor
        }

        fn health(&mut self, _request: HealthRequest) -> Result<HealthReport, EngineError> {
            Ok(HealthReport {
                status: HealthStatus::Healthy,
                dependencies: Vec::new(),
                message: "ready".into(),
            })
        }

        fn estimate(
            &self,
            _request: &EngineRequest,
            _inventory: &HardwareInventory,
        ) -> Result<Vec<EngineCandidate>, EngineError> {
            Ok(Vec::new())
        }

        fn execute(
            &mut self,
            request: EngineRequest,
            _context: &ExecutionContext<'_>,
        ) -> Result<EngineResponse, EngineError> {
            Ok(EngineResponse {
                request_id: request.request_id,
                evidence: Vec::new(),
                metadata: BTreeMap::new(),
            })
        }
    }

    #[test]
    fn registry_rejects_duplicate_ids() {
        let mut registry = EmbeddedRegistry::new();
        registry.register(Mock::new("mock")).unwrap();
        assert!(matches!(
            registry.register(Mock::new("mock")),
            Err(RuntimeError::DuplicateEngine(_))
        ));
        assert_eq!(registry.descriptors()[0].id, "mock");
    }
}
