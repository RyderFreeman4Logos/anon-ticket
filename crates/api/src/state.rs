// 引入标准库的原子引用计数 `Arc`，用于在线程间安全地共享数据。
use std::sync::Arc;

// 引入领域层服务：
// `InMemoryPidCache`: 用于缓存支付 ID (Payment ID) 的内存缓存服务。
// `TelemetryGuard`: 遥测（日志、指标）系统的守卫对象，用于管理生命周期。
use anon_ticket_domain::services::{cache::InMemoryPidCache, telemetry::TelemetryGuard};
// 引入存储层实现 `SeaOrmStorage`，它是基于 SeaORM 的数据库操作封装。
use anon_ticket_storage::SeaOrmStorage;

// 定义应用状态结构体 `AppState`。
// 该结构体持有整个应用程序运行所需的共享资源。
// `#[derive(Clone)]` 允许克隆该结构体。注意：字段内部通常被设计为轻量级克隆（如使用 Arc）。
#[derive(Clone)]
pub struct AppState {
    // 数据库存储接口，用于持久化数据。
    storage: SeaOrmStorage,
    // 支付 ID 缓存，使用 Arc 包装以便在多个请求/线程间共享。
    cache: Arc<InMemoryPidCache>,
    // 遥测守卫，持有它以确保日志和指标系统保持活动状态。
    telemetry: TelemetryGuard,
}

impl AppState {
    // 构造函数：创建一个新的 `AppState` 实例。
    // 参数分别对应结构体的字段。
    pub fn new(
        storage: SeaOrmStorage,
        cache: Arc<InMemoryPidCache>,
        telemetry: TelemetryGuard,
    ) -> Self {
        Self {
            storage,
            cache,
            telemetry,
        }
    }

    // 获取存储接口的引用。
    // 这是一个 getter 方法，提供对数据库层的访问。
    pub fn storage(&self) -> &SeaOrmStorage {
        &self.storage
    }

    // 获取缓存服务的引用。
    // `cache.as_ref()` 将 `Arc<InMemoryPidCache>` 转换为 `&InMemoryPidCache`。
    pub fn cache(&self) -> &InMemoryPidCache {
        self.cache.as_ref()
    }

    // 获取遥测守卫的引用。
    // 主要用于访问遥测相关的状态或配置。
    pub fn telemetry(&self) -> &TelemetryGuard {
        &self.telemetry
    }
}