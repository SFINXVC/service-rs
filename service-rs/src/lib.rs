// #[cfg(feature = "proc-macro")]
// pub use service_rs_proc_macro::{add_scoped, add_singleton, add_transient};

use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ServiceLifetime {
    Singleton,
    Scoped,
    Transient,
}

pub type Injectable<T> = Rc<T>;

type ServiceFactory =
    Box<dyn Fn(&ServiceProvider) -> Result<Box<dyn Any>, Box<dyn std::error::Error>>>;

pub(crate) struct ServiceDescriptor {
    pub(crate) lifetime: ServiceLifetime,
    pub(crate) factory: ServiceFactory,
    pub(crate) type_name: &'static str,
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

#[derive(Debug, Default)]
pub struct ServiceProvider {
    pub(crate) collection: ServiceCollection,
    pub(crate) services: RefCell<HashMap<TypeId, Rc<dyn Any>>>,
}

#[derive(Debug, Default)]
pub struct ScopedServiceProvider {
    pub(crate) provider: Rc<ServiceProvider>,
    pub(crate) services: RefCell<HashMap<TypeId, Rc<dyn Any>>>,
}

#[derive(Debug)]
pub enum Error {
    ServiceNotFound(String),
    ServiceInitializationError(Box<dyn std::error::Error>),
    InvalidScopeAccess(String),
    Unknown(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ServiceNotFound(service_name) => {
                write!(f, "Service not found: {}", service_name)
            }
            Error::ServiceInitializationError(error) => {
                write!(f, "Service initialization error: {}", error)
            }
            Error::InvalidScopeAccess(message) => {
                write!(f, "Invalid scope access: {}", message)
            }
            Error::Unknown(message) => write!(f, "Unknown error: {}", message),
        }
    }
}

impl std::error::Error for Error {}

impl From<Box<dyn std::error::Error>> for Error {
    fn from(error: Box<dyn std::error::Error>) -> Self {
        Error::ServiceInitializationError(error)
    }
}

impl ServiceCollection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_singleton_boxed<T: ?Sized + 'static, F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&ServiceProvider) -> Result<Box<T>, Box<dyn std::error::Error>> + 'static,
    {
        let key = TypeId::of::<Box<T>>();
        let type_name = std::any::type_name::<Box<T>>();

        self.services.insert(
            key,
            ServiceDescriptor {
                lifetime: ServiceLifetime::Singleton,
                factory: Box::new(move |provider| {
                    let result = factory(provider)?;
                    Ok(Box::new(result) as Box<dyn Any>)
                }),
                type_name,
            },
        );

        self
    }

    pub fn add_transient_boxed<T: ?Sized + 'static, F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&ServiceProvider) -> Result<Box<T>, Box<dyn std::error::Error>> + 'static,
    {
        let key = TypeId::of::<Box<T>>();
        let type_name = std::any::type_name::<Box<T>>();

        self.services.insert(
            key,
            ServiceDescriptor {
                lifetime: ServiceLifetime::Transient,
                factory: Box::new(move |provider| {
                    let result = factory(provider)?;
                    Ok(Box::new(result) as Box<dyn Any>)
                }),
                type_name,
            },
        );

        self
    }

    pub fn add_scoped_boxed<T: ?Sized + 'static, F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&ServiceProvider) -> Result<Box<T>, Box<dyn std::error::Error>> + 'static,
    {
        let key = TypeId::of::<Box<T>>();
        let type_name = std::any::type_name::<Box<T>>();

        self.services.insert(
            key,
            ServiceDescriptor {
                lifetime: ServiceLifetime::Scoped,
                factory: Box::new(move |provider| {
                    let result = factory(provider)?;
                    Ok(Box::new(result) as Box<dyn Any>)
                }),
                type_name,
            },
        );

        self
    }

    pub fn add_singleton<T: Any + 'static, F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&ServiceProvider) -> Result<T, Box<dyn std::error::Error>> + 'static,
    {
        self.services.insert(
            TypeId::of::<T>(),
            ServiceDescriptor {
                lifetime: ServiceLifetime::Singleton,
                factory: Box::new(move |provider| {
                    let result = factory(provider)?;
                    Ok(Box::new(result) as Box<dyn Any>)
                }),
                type_name: std::any::type_name::<T>(),
            },
        );

        self
    }

    pub fn add_transient<T: Any + 'static, F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&ServiceProvider) -> Result<T, Box<dyn std::error::Error>> + 'static,
    {
        self.services.insert(
            TypeId::of::<T>(),
            ServiceDescriptor {
                lifetime: ServiceLifetime::Transient,
                factory: Box::new(move |provider| {
                    let result = factory(provider)?;
                    Ok(Box::new(result) as Box<dyn Any>)
                }),
                type_name: std::any::type_name::<T>(),
            },
        );

        self
    }

    pub fn add_scoped<T: Any + 'static, F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&ServiceProvider) -> Result<T, Box<dyn std::error::Error>> + 'static,
    {
        self.services.insert(
            TypeId::of::<T>(),
            ServiceDescriptor {
                lifetime: ServiceLifetime::Scoped,
                factory: Box::new(move |provider| {
                    let result = factory(provider)?;
                    Ok(Box::new(result) as Box<dyn Any>)
                }),
                type_name: std::any::type_name::<T>(),
            },
        );

        self
    }

    pub fn build(self) -> ServiceProvider {
        ServiceProvider {
            collection: self,
            services: RefCell::new(HashMap::new()),
        }
    }
}

impl ServiceProvider {
    pub fn create_scope(self: &Rc<Self>) -> ScopedServiceProvider {
        ScopedServiceProvider {
            provider: self.clone(),
            services: RefCell::new(HashMap::new()),
        }
    }

    pub fn get_boxed<T: ?Sized + Any + 'static>(&self) -> Result<Rc<Box<T>>, Error> {
        self.get::<Box<T>>()
    }

    pub fn get<T: Any + 'static>(&self) -> Result<Injectable<T>, Error> {
        let type_id = TypeId::of::<T>();
        let type_name = std::any::type_name::<T>();

        let lifetime = self
            .collection
            .services
            .get(&type_id)
            .ok_or_else(|| Error::ServiceNotFound(type_name.to_string()))?
            .lifetime
            .clone();

        match lifetime {
            ServiceLifetime::Singleton => {
                if let Some(service) = self.services.borrow().get(&type_id) {
                    return service
                        .clone()
                        .downcast::<T>()
                        .map_err(|_| Error::ServiceNotFound(type_name.to_string()));
                } else {
                    let instance = self
                        .collection
                        .services
                        .get(&type_id)
                        .ok_or_else(|| Error::ServiceNotFound(type_name.to_string()))?
                        .factory
                        .as_ref()(self)
                    .map_err(|e| Error::Unknown(e.to_string()))?;

                    let rc_any = Rc::<dyn Any>::from(instance);

                    self.services.borrow_mut().insert(type_id, rc_any.clone());

                    rc_any
                        .downcast::<T>()
                        .map_err(|_| Error::ServiceNotFound(type_name.to_string()))
                }
            }
            ServiceLifetime::Transient => {
                let instance = self
                    .collection
                    .services
                    .get(&type_id)
                    .ok_or_else(|| Error::ServiceNotFound(type_name.to_string()))?
                    .factory
                    .as_ref()(self)
                .map_err(|e| Error::Unknown(e.to_string()))?;

                let rc_any = Rc::<dyn Any>::from(instance);

                rc_any
                    .downcast::<T>()
                    .map_err(|_| Error::ServiceNotFound(type_name.to_string()))
            }
            ServiceLifetime::Scoped => Err(Error::InvalidScopeAccess(format!(
                "Cannot resolve scoped service '{}' from root provider. Use create_scope() to create a scoped provider.",
                type_name
            ))),
        }
    }
}

impl ScopedServiceProvider {
    pub fn get_boxed<T: ?Sized + Any + 'static>(&self) -> Result<Injectable<Box<T>>, Error> {
        self.get::<Box<T>>()
    }

    pub fn get<T: Any + 'static>(&self) -> Result<Injectable<T>, Error> {
        let type_id = TypeId::of::<T>();
        let type_name = std::any::type_name::<T>();

        let lifetime = self
            .provider
            .collection
            .services
            .get(&type_id)
            .ok_or_else(|| Error::ServiceNotFound(type_name.to_string()))?
            .lifetime
            .clone();

        match lifetime {
            ServiceLifetime::Scoped => {
                if let Some(service) = self.services.borrow().get(&type_id) {
                    return service
                        .clone()
                        .downcast::<T>()
                        .map_err(|_| Error::ServiceNotFound(type_name.to_string()));
                } else {
                    let instance = self
                        .provider
                        .collection
                        .services
                        .get(&type_id)
                        .ok_or_else(|| Error::ServiceNotFound(type_name.to_string()))?
                        .factory
                        .as_ref()(&self.provider)
                    .map_err(|e| Error::Unknown(e.to_string()))?;

                    let rc_any = Rc::<dyn Any>::from(instance);

                    self.services.borrow_mut().insert(type_id, rc_any.clone());

                    rc_any
                        .downcast::<T>()
                        .map_err(|_| Error::ServiceNotFound(type_name.to_string()))
                }
            }
            _ => self.provider.get::<T>(),
        }
    }
}
