#![feature(unsize)]
#![feature(coerce_unsized)]

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::Arc,
};

use thiserror::Error;
use tokio::sync::RwLock;

#[cfg(feature = "proc-macro")]
pub use service_rs_proc_macro::Injectable;

#[cfg(feature = "proc-macro")]
pub trait InjectableExtension: Sized + Send + Sync + 'static {
    fn create_factory() -> ServiceFactory;
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ServiceLifetime {
    Singleton,
    Scoped,
    Transient,
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("Service with type '{type_name}' not found")]
    ServiceNotFound { type_name: &'static str },

    #[error("Service with type '{type_name}' already exists")]
    ServiceAlreadyExists { type_name: &'static str },

    #[error("Service resolution failed for type '{type_name}'")]
    ServiceResolutionFailed { type_name: &'static str },

    #[error("Service initialization failed for type '{type_name}' with error: {error}")]
    ServiceInitializationFailed {
        type_name: &'static str,
        error: Box<dyn std::error::Error>,
    },

    #[error(
        "Service with type '{type_name}' is resolved under ServiceProvider, but it's lifetime is ServiceLifetime::Scoped"
    )]
    ServiceInvalidScope { type_name: &'static str },
}

pub type ServiceFactory = Box<
    dyn Fn(
            ServiceProviderContext,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<Box<dyn Any + Send + Sync>, Box<dyn std::error::Error>>>
                    + Send,
            >,
        > + Send
        + Sync,
>;

pub struct ServiceDescriptor {
    pub(crate) lifetime: ServiceLifetime,
    pub(crate) type_name: &'static str,
    pub(crate) factory: ServiceFactory,
}

impl std::fmt::Debug for ServiceDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceDescriptor")
            .field("lifetime", &self.lifetime)
            .field("type_name", &self.type_name)
            .finish()
    }
}

#[derive(Debug, Default)]
pub struct ServiceCollection {
    pub(crate) services: HashMap<TypeId, ServiceDescriptor>,
}

#[derive(Clone)]
pub enum ServiceProviderContext {
    Root(Arc<ServiceProvider>),
    Scoped(Arc<ScopedServiceProvider>),
}

impl ServiceProviderContext {
    pub async fn get<T: Send + Sync + 'static>(&self) -> Result<Arc<T>, ServiceError> {
        match self {
            ServiceProviderContext::Root(provider) => provider.get::<T>().await,
            ServiceProviderContext::Scoped(scoped) => scoped.get::<T>().await,
        }
    }
}

impl ServiceCollection {
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }

    pub fn add_singleton_with_factory<T, F, Fut>(mut self, factory: F) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
        F: Fn(ServiceProviderContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Box<dyn Any + Send + Sync>, Box<dyn std::error::Error>>>
            + Send
            + 'static,
    {
        let type_id = TypeId::of::<T>();
        let service = ServiceDescriptor {
            lifetime: ServiceLifetime::Singleton,
            type_name: std::any::type_name::<T>(),
            factory: Box::new(move |ctx: ServiceProviderContext| Box::pin(factory(ctx))),
        };
        self.services.insert(type_id, service);
        self
    }

    pub fn add_scoped_with_factory<T, F, Fut>(mut self, factory: F) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
        F: Fn(ServiceProviderContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Box<dyn Any + Send + Sync>, Box<dyn std::error::Error>>>
            + Send
            + 'static,
    {
        let type_id = TypeId::of::<T>();
        let service = ServiceDescriptor {
            lifetime: ServiceLifetime::Scoped,
            type_name: std::any::type_name::<T>(),
            factory: Box::new(move |ctx: ServiceProviderContext| Box::pin(factory(ctx))),
        };
        self.services.insert(type_id, service);
        self
    }

    pub fn add_transient_with_factory<T, F, Fut>(mut self, factory: F) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
        F: Fn(ServiceProviderContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Box<dyn Any + Send + Sync>, Box<dyn std::error::Error>>>
            + Send
            + 'static,
    {
        let type_id = TypeId::of::<T>();
        let service = ServiceDescriptor {
            lifetime: ServiceLifetime::Transient,
            type_name: std::any::type_name::<T>(),
            factory: Box::new(move |ctx: ServiceProviderContext| Box::pin(factory(ctx))),
        };
        self.services.insert(type_id, service);
        self
    }

    #[cfg(feature = "proc-macro")]
    pub fn add_singleton<T>(mut self) -> Self
    where
        T: InjectableExtension,
    {
        let type_id = TypeId::of::<T>();
        let service = ServiceDescriptor {
            lifetime: ServiceLifetime::Singleton,
            type_name: std::any::type_name::<T>(),
            factory: T::create_factory(),
        };
        self.services.insert(type_id, service);
        self
    }

    #[cfg(feature = "proc-macro")]
    pub fn add_scoped<T>(mut self) -> Self
    where
        T: InjectableExtension,
    {
        let type_id = TypeId::of::<T>();
        let service = ServiceDescriptor {
            lifetime: ServiceLifetime::Scoped,
            type_name: std::any::type_name::<T>(),
            factory: T::create_factory(),
        };
        self.services.insert(type_id, service);
        self
    }

    #[cfg(feature = "proc-macro")]
    pub fn add_transient<T>(mut self) -> Self
    where
        T: InjectableExtension,
    {
        let type_id = TypeId::of::<T>();
        let service = ServiceDescriptor {
            lifetime: ServiceLifetime::Transient,
            type_name: std::any::type_name::<T>(),
            factory: T::create_factory(),
        };
        self.services.insert(type_id, service);
        self
    }

    #[cfg(feature = "proc-macro")]
    pub fn add_singleton_interface<T, TImpl>(mut self) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
        TImpl: InjectableExtension + Unpin + 'static + std::marker::Unsize<T>,
    {
        let type_id = TypeId::of::<Box<T>>();
        let impl_factory = Arc::new(TImpl::create_factory());
        let service = ServiceDescriptor {
            lifetime: ServiceLifetime::Singleton,
            type_name: std::any::type_name::<Box<T>>(),
            factory: Box::new(move |ctx: ServiceProviderContext| {
                let impl_factory = Arc::clone(&impl_factory);
                Box::pin(async move {
                    let concrete = impl_factory(ctx).await?;
                    let downcasted = concrete.downcast::<TImpl>().map_err(|_| {
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Failed to downcast",
                        )) as Box<dyn std::error::Error>
                    })?;
                    let trait_obj: Box<T> = downcasted;
                    Ok(Box::new(trait_obj) as Box<dyn Any + Send + Sync>)
                })
            }),
        };
        self.services.insert(type_id, service);
        self
    }

    #[cfg(feature = "proc-macro")]
    pub fn add_scoped_interface<T, TImpl>(mut self) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
        TImpl: InjectableExtension + Unpin + 'static + std::marker::Unsize<T>,
    {
        let type_id = TypeId::of::<Box<T>>();
        let impl_factory = Arc::new(TImpl::create_factory());
        let service = ServiceDescriptor {
            lifetime: ServiceLifetime::Scoped,
            type_name: std::any::type_name::<Box<T>>(),
            factory: Box::new(move |ctx: ServiceProviderContext| {
                let impl_factory = Arc::clone(&impl_factory);
                Box::pin(async move {
                    let concrete = impl_factory(ctx).await?;
                    let downcasted = concrete.downcast::<TImpl>().map_err(|_| {
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Failed to downcast",
                        )) as Box<dyn std::error::Error>
                    })?;
                    let trait_obj: Box<T> = downcasted;
                    Ok(Box::new(trait_obj) as Box<dyn Any + Send + Sync>)
                })
            }),
        };
        self.services.insert(type_id, service);
        self
    }

    #[cfg(feature = "proc-macro")]
    pub fn add_transient_interface<T, TImpl>(mut self) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
        TImpl: InjectableExtension + Unpin + 'static + std::marker::Unsize<T>,
    {
        let type_id = TypeId::of::<Box<T>>();
        let impl_factory = Arc::new(TImpl::create_factory());
        let service = ServiceDescriptor {
            lifetime: ServiceLifetime::Transient,
            type_name: std::any::type_name::<Box<T>>(),
            factory: Box::new(move |ctx: ServiceProviderContext| {
                let impl_factory = Arc::clone(&impl_factory);
                Box::pin(async move {
                    let concrete = impl_factory(ctx).await?;
                    let downcasted = concrete.downcast::<TImpl>().map_err(|_| {
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Failed to downcast",
                        )) as Box<dyn std::error::Error>
                    })?;
                    let trait_obj: Box<T> = downcasted;
                    Ok(Box::new(trait_obj) as Box<dyn Any + Send + Sync>)
                })
            }),
        };
        self.services.insert(type_id, service);
        self
    }

    pub fn len(&self) -> usize {
        self.services.len()
    }

    pub fn build(self) -> Arc<ServiceProvider> {
        Arc::new(ServiceProvider {
            collection: self,
            services: RwLock::new(HashMap::new()),
        })
    }
}

#[derive(Debug, Default)]
pub struct ServiceProvider {
    pub(crate) collection: ServiceCollection,
    pub(crate) services: RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl ServiceProvider {
    pub fn create_scope(self: &Arc<Self>) -> Arc<ScopedServiceProvider> {
        Arc::new(ScopedServiceProvider {
            provider: Arc::clone(self),
            services: RwLock::new(HashMap::new()),
        })
    }

    pub async fn get<T>(self: &Arc<Self>) -> Result<Arc<T>, ServiceError>
    where
        T: Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();

        // lookup from collection
        let descriptor = self.collection.services.get(&type_id).map_or_else(
            || {
                Err(ServiceError::ServiceNotFound {
                    type_name: std::any::type_name::<T>(),
                })
            },
            |service| Ok(service),
        )?;

        match descriptor.lifetime {
            ServiceLifetime::Singleton => {
                if let Some(service) = self.services.read().await.get(&type_id) {
                    let cloned = Arc::clone(service);
                    return cloned.downcast::<T>().map_err(|_| {
                        ServiceError::ServiceResolutionFailed {
                            type_name: std::any::type_name::<T>(),
                        }
                    });
                }

                let service = (descriptor.factory)(ServiceProviderContext::Root(Arc::clone(self)))
                    .await
                    .map_err(|e| ServiceError::ServiceInitializationFailed {
                        type_name: std::any::type_name::<T>(),
                        error: e,
                    })?;

                let arc_service: Arc<dyn Any + Send + Sync> = Arc::from(service);

                self.services
                    .write()
                    .await
                    .insert(type_id, Arc::clone(&arc_service));

                return arc_service.downcast::<T>().map_err(|_| {
                    ServiceError::ServiceResolutionFailed {
                        type_name: std::any::type_name::<T>(),
                    }
                });
            }
            ServiceLifetime::Scoped => Err(ServiceError::ServiceInvalidScope {
                type_name: std::any::type_name::<T>(),
            }),
            ServiceLifetime::Transient => {
                let service = (descriptor.factory)(ServiceProviderContext::Root(Arc::clone(self)))
                    .await
                    .map_err(|e| ServiceError::ServiceInitializationFailed {
                        type_name: std::any::type_name::<T>(),
                        error: e,
                    })?;

                let arc_service: Arc<dyn Any + Send + Sync> = Arc::from(service);

                return arc_service.downcast::<T>().map_err(|_| {
                    ServiceError::ServiceResolutionFailed {
                        type_name: std::any::type_name::<T>(),
                    }
                });
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct ScopedServiceProvider {
    pub(crate) provider: Arc<ServiceProvider>,
    pub(crate) services: RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl ScopedServiceProvider {
    pub async fn get<T>(self: &Arc<Self>) -> Result<Arc<T>, ServiceError>
    where
        T: Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();

        // lookup from collection
        let descriptor = self
            .provider
            .collection
            .services
            .get(&type_id)
            .map_or_else(
                || {
                    Err(ServiceError::ServiceNotFound {
                        type_name: std::any::type_name::<T>(),
                    })
                },
                |service| Ok(service),
            )?;

        match descriptor.lifetime {
            ServiceLifetime::Singleton => self.provider.get::<T>().await,
            ServiceLifetime::Scoped => {
                if let Some(service) = self.services.read().await.get(&type_id) {
                    let cloned = Arc::clone(service);
                    return cloned.downcast::<T>().map_err(|_| {
                        ServiceError::ServiceResolutionFailed {
                            type_name: std::any::type_name::<T>(),
                        }
                    });
                }

                let service =
                    (descriptor.factory)(ServiceProviderContext::Scoped(Arc::clone(self)))
                        .await
                        .map_err(|e| ServiceError::ServiceInitializationFailed {
                            type_name: std::any::type_name::<T>(),
                            error: e,
                        })?;

                let arc_service: Arc<dyn Any + Send + Sync> = Arc::from(service);

                self.services
                    .write()
                    .await
                    .insert(type_id, Arc::clone(&arc_service));

                return arc_service.downcast::<T>().map_err(|_| {
                    ServiceError::ServiceResolutionFailed {
                        type_name: std::any::type_name::<T>(),
                    }
                });
            }
            ServiceLifetime::Transient => self.provider.get::<T>().await,
        }
    }
}
