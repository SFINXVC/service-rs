//! # service-rs
//!
//! An async-first, lightweight dependency injection (DI) container for Rust.
//!
//! This library provides a simple and ergonomic way to manage dependencies in your Rust applications,
//! inspired by [Microsoft.Extensions.DependencyInjection](https://learn.microsoft.com/en-us/dotnet/api/microsoft.extensions.dependencyinjection).
//!
//! ## Features
//!
//! - **Three service lifetimes**: Singleton, Scoped, and Transient
//! - **Async-first design**: All service resolution is async using `tokio`
//! - **Thread-safe**: Services are wrapped in `Arc<T>` for safe sharing across threads
//! - **Automatic dependency injection**: Use the `#[derive(Injectable)]` macro for automatic constructor injection
//! - **Trait object support**: Register implementations for trait objects
//! - **Scoped services**: Create service scopes with scoped lifetime management
//!
//! ## Service Lifetimes
//!
//! - **Singleton**: One instance created and shared across the entire application
//! - **Scoped**: One instance per scope; same instance within a scope, new instance for each scope
//! - **Transient**: New instance created every time the service is requested
//!
//! ## Example
//!
//! ```no_run
//! use service_rs::ServiceCollection;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let collection = ServiceCollection::new()
//!         .add_singleton_with_factory::<i32, _, _>(|_| async {
//!             Ok(Box::new(42) as Box<dyn std::any::Any + Send + Sync>)
//!         });
//!
//!     let provider = collection.build();
//!     let num: Arc<i32> = provider.get::<i32>().await.unwrap();
//!     assert_eq!(*num, 42);
//! }
//! ```

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

/// Extension trait for types that can be automatically injected.
///
/// This trait is automatically implemented when you use the `#[derive(Injectable)]` macro.
/// It generates a factory function that resolves all dependencies from the service provider.
///
/// # Example
///
/// ```no_run
/// use service_rs::{Injectable, ServiceCollection};
/// use std::sync::Arc;
///
/// #[derive(Injectable)]
/// struct MyService {
///     dependency: Arc<i32>,
/// }
/// ```
#[cfg(feature = "proc-macro")]
pub trait InjectableExtension: Sized + Send + Sync + 'static {
    /// Creates a factory function for this type.
    fn create_factory() -> ServiceFactory;
}

/// Defines the lifetime of a service in the dependency injection container.
#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ServiceLifetime {
    /// A single instance is created and shared across the entire application.
    Singleton,
    /// A single instance is created per scope. Different scopes get different instances.
    Scoped,
    /// A new instance is created every time the service is requested.
    Transient,
}

/// Errors that can occur during service registration and resolution.
#[derive(Debug, Error)]
pub enum ServiceError {
    /// The requested service type was not registered in the container.
    #[error("Service with type '{type_name}' not found")]
    ServiceNotFound { type_name: &'static str },

    /// Attempted to register a service type that already exists.
    #[error("Service with type '{type_name}' already exists")]
    ServiceAlreadyExists { type_name: &'static str },

    /// Failed to downcast the service to the requested type.
    #[error("Service resolution failed for type '{type_name}'")]
    ServiceResolutionFailed { type_name: &'static str },

    /// The service factory threw an error during initialization.
    #[error("Service initialization failed for type '{type_name}' with error: {error}")]
    ServiceInitializationFailed {
        type_name: &'static str,
        error: Box<dyn std::error::Error>,
    },

    /// Attempted to resolve a scoped service from the root provider instead of a scope.
    #[error(
        "Service with type '{type_name}' is resolved under ServiceProvider, but it's lifetime is ServiceLifetime::Scoped"
    )]
    ServiceInvalidScope { type_name: &'static str },
}

/// A factory function that creates service instances.
///
/// The factory receives a [`ServiceProviderContext`] and returns a pinned future
/// that resolves to a boxed service instance or an error.
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

/// Internal descriptor that stores service registration information.
///
/// This struct is used internally by the service container to track registered services.
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

/// A collection of service descriptors used to build a service provider.
///
/// Use this to register services with different lifetimes before building
/// the final [`ServiceProvider`].
///
/// # Example
///
/// ```no_run
/// use service_rs::ServiceCollection;
///
/// let collection = ServiceCollection::new()
///     .add_singleton_with_factory::<i32, _, _>(|_| async {
///         Ok(Box::new(42) as Box<dyn std::any::Any + Send + Sync>)
///     });
///
/// let provider = collection.build();
/// ```
#[derive(Debug, Default)]
pub struct ServiceCollection {
    pub(crate) services: HashMap<TypeId, ServiceDescriptor>,
}

/// Context passed to service factories to enable dependency resolution.
///
/// This enum represents either a root provider or a scoped provider,
/// allowing factories to resolve dependencies from the appropriate context.
#[derive(Clone)]
pub enum ServiceProviderContext {
    /// Root service provider context.
    Root(Arc<ServiceProvider>),
    /// Scoped service provider context.
    Scoped(Arc<ScopedServiceProvider>),
}

impl ServiceProviderContext {
    /// Resolves a service from the current context.
    ///
    /// # Errors
    ///
    /// Returns a [`ServiceError`] if the service cannot be resolved.
    pub async fn get<T: Send + Sync + 'static>(&self) -> Result<Arc<T>, ServiceError> {
        match self {
            ServiceProviderContext::Root(provider) => provider.get::<T>().await,
            ServiceProviderContext::Scoped(scoped) => scoped.get::<T>().await,
        }
    }
}

impl ServiceCollection {
    /// Creates a new empty service collection.
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }

    /// Registers a singleton service using a factory function.
    ///
    /// The factory is called once when the service is first requested, and the same
    /// instance is returned for all subsequent requests.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use service_rs::ServiceCollection;
    ///
    /// let collection = ServiceCollection::new()
    ///     .add_singleton_with_factory::<i32, _, _>(|_| async {
    ///         Ok(Box::new(42) as Box<dyn std::any::Any + Send + Sync>)
    ///     });
    /// ```
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

    /// Registers a scoped service using a factory function.
    ///
    /// The factory is called once per scope when the service is first requested within that scope.
    /// The same instance is returned for all requests within the same scope.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use service_rs::ServiceCollection;
    ///
    /// let collection = ServiceCollection::new()
    ///     .add_scoped_with_factory::<String, _, _>(|_| async {
    ///         Ok(Box::new("scoped".to_string()) as Box<dyn std::any::Any + Send + Sync>)
    ///     });
    /// ```
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

    /// Registers a transient service using a factory function.
    ///
    /// The factory is called every time the service is requested, creating a new instance each time.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use service_rs::ServiceCollection;
    ///
    /// let collection = ServiceCollection::new()
    ///     .add_transient_with_factory::<String, _, _>(|_| async {
    ///         Ok(Box::new("transient".to_string()) as Box<dyn std::any::Any + Send + Sync>)
    ///     });
    /// ```
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

    /// Registers a singleton service using the `Injectable` derive macro.
    ///
    /// The service type must implement [`InjectableExtension`] via the `#[derive(Injectable)]` macro.
    /// Dependencies are automatically resolved from the service provider.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use service_rs::{Injectable, ServiceCollection};
    /// use std::sync::Arc;
    ///
    /// #[derive(Injectable)]
    /// struct MyService {
    ///     dependency: Arc<i32>,
    /// }
    ///
    /// let collection = ServiceCollection::new()
    ///     .add_singleton::<MyService>();
    /// ```
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

    /// Registers a scoped service using the `Injectable` derive macro.
    ///
    /// The service type must implement [`InjectableExtension`] via the `#[derive(Injectable)]` macro.
    /// Dependencies are automatically resolved from the service provider.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use service_rs::{Injectable, ServiceCollection};
    /// use std::sync::Arc;
    ///
    /// #[derive(Injectable)]
    /// struct MyService {
    ///     dependency: Arc<i32>,
    /// }
    ///
    /// let collection = ServiceCollection::new()
    ///     .add_scoped::<MyService>();
    /// ```
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

    /// Registers a transient service using the `Injectable` derive macro.
    ///
    /// The service type must implement [`InjectableExtension`] via the `#[derive(Injectable)]` macro.
    /// Dependencies are automatically resolved from the service provider.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use service_rs::{Injectable, ServiceCollection};
    /// use std::sync::Arc;
    ///
    /// #[derive(Injectable)]
    /// struct MyService {
    ///     dependency: Arc<i32>,
    /// }
    ///
    /// let collection = ServiceCollection::new()
    ///     .add_transient::<MyService>();
    /// ```
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

    /// Registers a singleton service for a trait object.
    ///
    /// This allows you to register an implementation type that will be resolved as a trait object.
    /// The implementation must derive `Injectable` and implement the trait.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use service_rs::{Injectable, ServiceCollection};
    ///
    /// trait Logger: Send + Sync {
    ///     fn log(&self, msg: &str);
    /// }
    ///
    /// #[derive(Injectable)]
    /// struct ConsoleLogger;
    ///
    /// impl Logger for ConsoleLogger {
    ///     fn log(&self, msg: &str) {
    ///         println!("{}", msg);
    ///     }
    /// }
    ///
    /// let collection = ServiceCollection::new()
    ///     .add_singleton_interface::<dyn Logger, ConsoleLogger>();
    /// ```
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

    /// Registers a scoped service for a trait object.
    ///
    /// This allows you to register an implementation type that will be resolved as a trait object.
    /// The implementation must derive `Injectable` and implement the trait.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use service_rs::{Injectable, ServiceCollection};
    ///
    /// trait Repository: Send + Sync {
    ///     fn save(&self);
    /// }
    ///
    /// #[derive(Injectable)]
    /// struct DbRepository;
    ///
    /// impl Repository for DbRepository {
    ///     fn save(&self) {}
    /// }
    ///
    /// let collection = ServiceCollection::new()
    ///     .add_scoped_interface::<dyn Repository, DbRepository>();
    /// ```
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

    /// Registers a transient service for a trait object.
    ///
    /// This allows you to register an implementation type that will be resolved as a trait object.
    /// The implementation must derive `Injectable` and implement the trait.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use service_rs::{Injectable, ServiceCollection};
    ///
    /// trait Handler: Send + Sync {
    ///     fn handle(&self);
    /// }
    ///
    /// #[derive(Injectable)]
    /// struct RequestHandler;
    ///
    /// impl Handler for RequestHandler {
    ///     fn handle(&self) {}
    /// }
    ///
    /// let collection = ServiceCollection::new()
    ///     .add_transient_interface::<dyn Handler, RequestHandler>();
    /// ```
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

    /// Returns the number of registered services.
    pub fn len(&self) -> usize {
        self.services.len()
    }

    /// Returns true if the collection contains no services.
    pub fn is_empty(&self) -> bool {
        self.services.is_empty()
    }

    /// Builds the service provider from this collection.
    ///
    /// Consumes the collection and returns an [`Arc<ServiceProvider>`] that can be used
    /// to resolve services.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use service_rs::ServiceCollection;
    ///
    /// let collection = ServiceCollection::new();
    /// let provider = collection.build();
    /// ```
    pub fn build(self) -> Arc<ServiceProvider> {
        Arc::new(ServiceProvider {
            collection: self,
            services: RwLock::new(HashMap::new()),
        })
    }
}

/// The root service provider for resolving services.
///
/// This is created by calling [`ServiceCollection::build()`] and provides methods
/// to resolve singleton and transient services, as well as create scoped providers.
///
/// # Example
///
/// ```no_run
/// use service_rs::ServiceCollection;
/// use std::sync::Arc;
///
/// # async fn example() {
/// let provider = ServiceCollection::new()
///     .add_singleton_with_factory::<i32, _, _>(|_| async {
///         Ok(Box::new(42) as Box<dyn std::any::Any + Send + Sync>)
///     })
///     .build();
///
/// let value: Arc<i32> = provider.get::<i32>().await.unwrap();
/// # }
/// ```
#[derive(Debug, Default)]
pub struct ServiceProvider {
    pub(crate) collection: ServiceCollection,
    pub(crate) services: RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl ServiceProvider {
    /// Creates a new service scope.
    ///
    /// Scopes are used to manage scoped service lifetimes. Services registered with
    /// [`ServiceCollection::add_scoped`] or [`ServiceCollection::add_scoped_with_factory`]
    /// will have one instance per scope.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use service_rs::ServiceCollection;
    ///
    /// # async fn example() {
    /// let provider = ServiceCollection::new().build();
    /// let scope = provider.create_scope();
    /// # }
    /// ```
    pub fn create_scope(self: &Arc<Self>) -> Arc<ScopedServiceProvider> {
        Arc::new(ScopedServiceProvider {
            provider: Arc::clone(self),
            services: RwLock::new(HashMap::new()),
        })
    }

    /// Resolves a service from the provider.
    ///
    /// Returns an [`Arc<T>`] containing the resolved service instance.
    ///
    /// # Errors
    ///
    /// - [`ServiceError::ServiceNotFound`] if the service type is not registered
    /// - [`ServiceError::ServiceInvalidScope`] if attempting to resolve a scoped service from the root provider
    /// - [`ServiceError::ServiceResolutionFailed`] if the service cannot be downcast to the requested type
    /// - [`ServiceError::ServiceInitializationFailed`] if the factory function returns an error
    ///
    /// # Example
    ///
    /// ```no_run
    /// use service_rs::ServiceCollection;
    /// use std::sync::Arc;
    ///
    /// # async fn example() {
    /// let provider = ServiceCollection::new()
    ///     .add_singleton_with_factory::<i32, _, _>(|_| async {
    ///         Ok(Box::new(42) as Box<dyn std::any::Any + Send + Sync>)
    ///     })
    ///     .build();
    ///
    /// let value: Arc<i32> = provider.get::<i32>().await.unwrap();
    /// assert_eq!(*value, 42);
    /// # }
    /// ```
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

/// A scoped service provider for resolving scoped services.
///
/// Created by calling [`ServiceProvider::create_scope()`]. Services registered with
/// scoped lifetime will have one instance per scope.
///
/// # Example
///
/// ```no_run
/// use service_rs::ServiceCollection;
/// use std::sync::Arc;
///
/// # async fn example() {
/// let provider = ServiceCollection::new()
///     .add_scoped_with_factory::<String, _, _>(|_| async {
///         Ok(Box::new("scoped".to_string()) as Box<dyn std::any::Any + Send + Sync>)
///     })
///     .build();
///
/// let scope = provider.create_scope();
/// let value: Arc<String> = scope.get::<String>().await.unwrap();
/// # }
/// ```
#[derive(Debug, Default)]
pub struct ScopedServiceProvider {
    pub(crate) provider: Arc<ServiceProvider>,
    pub(crate) services: RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl ScopedServiceProvider {
    /// Resolves a service from the scoped provider.
    ///
    /// Returns an [`Arc<T>`] containing the resolved service instance.
    ///
    /// - Singleton services are resolved from the root provider
    /// - Scoped services are resolved once per scope and cached
    /// - Transient services are resolved from the root provider (new instance each time)
    ///
    /// # Errors
    ///
    /// - [`ServiceError::ServiceNotFound`] if the service type is not registered
    /// - [`ServiceError::ServiceResolutionFailed`] if the service cannot be downcast to the requested type
    /// - [`ServiceError::ServiceInitializationFailed`] if the factory function returns an error
    ///
    /// # Example
    ///
    /// ```no_run
    /// use service_rs::ServiceCollection;
    /// use std::sync::Arc;
    ///
    /// # async fn example() {
    /// let provider = ServiceCollection::new()
    ///     .add_scoped_with_factory::<String, _, _>(|_| async {
    ///         Ok(Box::new("scoped".to_string()) as Box<dyn std::any::Any + Send + Sync>)
    ///     })
    ///     .build();
    ///
    /// let scope = provider.create_scope();
    /// let value1: Arc<String> = scope.get::<String>().await.unwrap();
    /// let value2: Arc<String> = scope.get::<String>().await.unwrap();
    /// assert_eq!(Arc::as_ptr(&value1), Arc::as_ptr(&value2)); // Same instance within scope
    /// # }
    /// ```
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
