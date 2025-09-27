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

type ServiceFactory = Box<dyn Fn(&ServiceProvider) -> Box<dyn Any>>;

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

#[derive(Debug, Clone)]
pub enum Error {
    ServiceNotFound(String),
    Unknown(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ServiceNotFound(service_name) => {
                write!(f, "Service not found: {}", service_name)
            }
            Error::Unknown(message) => write!(f, "Unknown error: {}", message),
        }
    }
}

impl ServiceCollection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_singleton_boxed<T: ?Sized + 'static, F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&ServiceProvider) -> Box<T> + 'static,
    {
        let key = TypeId::of::<Box<T>>();
        let type_name = std::any::type_name::<Box<T>>();

        self.services.insert(
            key,
            ServiceDescriptor {
                lifetime: ServiceLifetime::Singleton,
                factory: Box::new(move |provider| Box::new(factory(provider)) as Box<dyn Any>),
                type_name,
            },
        );

        self
    }

    pub fn add_transient_boxed<T: ?Sized + 'static, F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&ServiceProvider) -> Box<T> + 'static,
    {
        let key = TypeId::of::<Box<T>>();
        let type_name = std::any::type_name::<Box<T>>();

        self.services.insert(
            key,
            ServiceDescriptor {
                lifetime: ServiceLifetime::Transient,
                factory: Box::new(move |provider| Box::new(factory(provider)) as Box<dyn Any>),
                type_name,
            },
        );

        self
    }

    pub fn add_scoped_boxed<T: ?Sized + 'static, F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&ServiceProvider) -> Box<T> + 'static,
    {
        let key = TypeId::of::<Box<T>>();
        let type_name = std::any::type_name::<Box<T>>();

        self.services.insert(
            key,
            ServiceDescriptor {
                lifetime: ServiceLifetime::Scoped,
                factory: Box::new(move |provider| Box::new(factory(provider)) as Box<dyn Any>),
                type_name,
            },
        );

        self
    }

    pub fn add_singleton<T: Any + 'static, F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&ServiceProvider) -> Box<dyn Any> + 'static,
    {
        self.services.insert(
            TypeId::of::<T>(),
            ServiceDescriptor {
                lifetime: ServiceLifetime::Singleton,
                factory: Box::new(move |provider| Box::new(factory(provider)) as Box<dyn Any>),
                type_name: std::any::type_name::<T>(),
            },
        );

        self
    }

    pub fn add_transient<T: Any + 'static, F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&ServiceProvider) -> Box<dyn Any> + 'static,
    {
        self.services.insert(
            TypeId::of::<T>(),
            ServiceDescriptor {
                lifetime: ServiceLifetime::Transient,
                factory: Box::new(move |provider| Box::new(factory(provider)) as Box<dyn Any>),
                type_name: std::any::type_name::<T>(),
            },
        );

        self
    }

    pub fn add_scoped<T: Any + 'static, F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&ServiceProvider) -> Box<dyn Any> + 'static,
    {
        self.services.insert(
            TypeId::of::<T>(),
            ServiceDescriptor {
                lifetime: ServiceLifetime::Scoped,
                factory: Box::new(move |provider| Box::new(factory(provider)) as Box<dyn Any>),
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

    pub fn get<T: Any + 'static>(&self) -> Result<Rc<T>, Error> {
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
                        .as_ref()(self);

                    let rc_any = Rc::<dyn Any>::from(instance);

                    self.services
                        .borrow_mut()
                        .insert(type_id, Rc::from(rc_any.clone()));

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
                    .as_ref()(self);

                let rc_any = Rc::<dyn Any>::from(instance);

                rc_any
                    .downcast::<T>()
                    .map_err(|_| Error::ServiceNotFound(type_name.to_string()))
            }
            ServiceLifetime::Scoped => {
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
                        .as_ref()(self);

                    let rc_any = Rc::<dyn Any>::from(instance);

                    self.services
                        .borrow_mut()
                        .insert(type_id, Rc::from(rc_any.clone()));

                    rc_any
                        .downcast::<T>()
                        .map_err(|_| Error::ServiceNotFound(type_name.to_string()))
                }
            }
        }
    }
}

impl ScopedServiceProvider {
    pub fn get_boxed<T: ?Sized + Any + 'static>(&self) -> Result<Rc<Box<T>>, Error> {
        self.get::<Box<T>>()
    }

    pub fn get<T: Any + 'static>(&self) -> Result<Rc<T>, Error> {
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
                        .as_ref()(&self.provider);

                    let rc_any = Rc::<dyn Any>::from(instance);

                    self.services
                        .borrow_mut()
                        .insert(type_id, Rc::from(rc_any.clone()));

                    rc_any
                        .downcast::<T>()
                        .map_err(|_| Error::ServiceNotFound(type_name.to_string()))
                }
            }
            _ => self.provider.get::<T>(),
        }
    }
}
