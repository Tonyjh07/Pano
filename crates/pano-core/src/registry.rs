//! 适配器注册表：`AdapterId → Box<dyn Adapter>` 映射。
//!
//! id 重复 → [`CoreError::DuplicateAdapter`]，启动失败（开发期尽早暴露）。

use std::collections::HashMap;

use crate::adapter::{Adapter, AdapterId};
use crate::error::CoreError;

/// 适配器注册表。
#[derive(Default)]
pub struct Registry {
    adapters: HashMap<AdapterId, Box<dyn Adapter>>,
}

impl Registry {
    /// 空注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个适配器；id 重复返回 [`CoreError::DuplicateAdapter`]。
    pub fn register(&mut self, adapter: Box<dyn Adapter>) -> Result<(), CoreError> {
        let id = adapter.meta().id.clone();
        if self.adapters.contains_key(&id) {
            return Err(CoreError::DuplicateAdapter(id));
        }
        self.adapters.insert(id, adapter);
        Ok(())
    }

    /// 按 id 获取适配器（只读）。
    pub fn get(&self, id: &AdapterId) -> Option<&(dyn Adapter + '_)> {
        self.adapters.get(id).map(|b| &**b)
    }

    /// 按 id 获取适配器（可变）。
    pub fn get_mut(&mut self, id: &AdapterId) -> Option<&mut (dyn Adapter + 'static)> {
        self.adapters.get_mut(id).map(|b| &mut **b)
    }

    /// 全部已注册适配器 id。
    pub fn ids(&self) -> Vec<AdapterId> {
        self.adapters.keys().cloned().collect()
    }

    /// 已注册适配器数量。
    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{AdapterContext, AdapterError, AdapterMeta, AdapterStatus, ConfigSchema};
    use crate::capability::Capability;

    struct DummyAdapter {
        id: AdapterId,
    }

    impl Adapter for DummyAdapter {
        fn meta(&self) -> AdapterMeta {
            AdapterMeta {
                id: self.id.clone(),
                name: "dummy".into(),
                description: String::new(),
                version: "0.1.0".into(),
            }
        }

        fn capabilities(&self) -> Vec<Capability> {
            vec![Capability::new(Capability::TIME_SERIES)]
        }

        fn config_schema(&self) -> ConfigSchema {
            ConfigSchema::empty()
        }

        fn start(&mut self, _ctx: AdapterContext) -> Result<(), AdapterError> {
            Ok(())
        }

        fn stop(&mut self) -> Result<(), AdapterError> {
            Ok(())
        }

        fn status(&self) -> AdapterStatus {
            AdapterStatus::Stopped
        }
    }

    fn dummy(id: &str) -> Box<dyn Adapter> {
        Box::new(DummyAdapter {
            id: AdapterId::new(id),
        })
    }

    #[test]
    fn register_and_get() {
        let mut reg = Registry::new();
        reg.register(dummy("example.a")).unwrap();
        reg.register(dummy("example.b")).unwrap();

        assert_eq!(reg.len(), 2);
        assert!(reg.get(&AdapterId::new("example.a")).is_some());
        assert!(reg.get(&AdapterId::new("missing")).is_none());
        assert_eq!(reg.ids().len(), 2);
    }

    #[test]
    fn duplicate_id_rejected() {
        let mut reg = Registry::new();
        reg.register(dummy("example.a")).unwrap();
        let err = reg.register(dummy("example.a")).unwrap_err();
        assert!(matches!(
            err,
            CoreError::DuplicateAdapter(id) if id.as_str() == "example.a"
        ));
    }
}
